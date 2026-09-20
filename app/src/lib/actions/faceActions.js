// People/Faces operations (RFC-0009 P3c, moved out of +page.svelte's script): everything that
// reads or writes only the `faces` store plus IPC. The detection runners
// (`runFaceDetection`, `handleDetectFaces…`) and `refreshCurrentImageFaces` stay in the page until
// the library/selection stores exist (P4), since they read the selected/filtered photos.

import { faces } from "$lib/state/faces.svelte.js";
import { listPeople, renamePerson, reassignFace, createPerson, setFaceExcluded, cancelFaceDetection } from "$lib/api/faces.js";
import { getDevelopPreview } from "$lib/api/develop.js";
import { convertFileSrc } from "@tauri-apps/api/core";

/** Refetches the People rail AND resolves any newly-seen cover photo to
 * a real decoded-preview URL (source files are often RAW/HEIC -- not
 * directly renderable by an `<img>`/CSS `background-image`, so this
 * reuses the same Develop-preview decode Library/Develop already rely
 * on). Cached by path across calls -- renaming/reassigning people
 * doesn't change whose photo is whose cover most of the time, so this
 * only ever decodes a genuinely new cover photo. */
export async function refreshPeople() {
  faces.people = await listPeople();
  const uniquePaths = [...new Set(faces.people.map((p) => p.cover_image_path).filter((p) => p !== null))];
  const missing = uniquePaths.filter((p) => !(p in faces.avatarSourceUrls));
  if (missing.length === 0) return;
  const resolved = await Promise.all(
    missing.map((path) =>
      getDevelopPreview(/** @type {string} */ (path), null)
        .then((info) => convertFileSrc(info.path))
        .catch(() => null),
    ),
  );
  const next = { ...faces.avatarSourceUrls };
  missing.forEach((path, i) => (next[/** @type {string} */ (path)] = resolved[i]));
  faces.avatarSourceUrls = next;
}

export async function handleRenamePerson(/** @type {number} */ personId, /** @type {string | null} */ name) {
  faces.people = faces.people.map((p) => (p.id === personId ? { ...p, name } : p));
  faces.currentImageFaces = faces.currentImageFaces.map((f) => (f.person_id === personId ? { ...f, person_name: name } : f));
  await renamePerson(personId, name);
}

/** Existing-person pick from the tag popover, or the face menu's
 * "Reassign to…" -- `personId: null` (from `setFaceExcluded`'s own
 * caller below) clears the face back to unclustered. */
export async function handleReassignFace(/** @type {number} */ faceId, /** @type {number | null} */ personId) {
  const personName = personId === null ? null : (faces.people.find((p) => p.id === personId)?.name ?? null);
  faces.currentImageFaces = faces.currentImageFaces.map((f) =>
    f.id === faceId ? { ...f, person_id: personId, person_name: personName } : f,
  );
  await reassignFace(faceId, personId);
  await refreshPeople();
}

/** The tag popover's "+ New person…" row: creates the person, names it,
 * then assigns this face to it -- three IPC calls composed here rather
 * than inside a single backend command, matching how the rest of this
 * app composes existing collection/keyword commands from the frontend. */
export async function handleCreatePersonAndTagFace(/** @type {number} */ faceId, /** @type {string} */ name) {
  const personId = await createPerson(faceId);
  await renamePerson(personId, name);
  await handleReassignFace(faceId, personId);
}

/** "Not a face" (kept, not deleted) -- removes it from the current
 * photo's face list immediately, matching `get_faces_for_image`'s own
 * `excluded = 0` filter. */
export async function handleSetFaceExcluded(/** @type {number} */ faceId, /** @type {boolean} */ excluded) {
  if (excluded) faces.currentImageFaces = faces.currentImageFaces.filter((f) => f.id !== faceId);
  await setFaceExcluded(faceId, excluded);
  await refreshPeople();
}

export function handleCancelFaceDetection() {
  cancelFaceDetection().catch(() => {});
}
