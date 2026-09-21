// People/Faces operations (RFC-0009 P3c, P4c, moved out of +page.svelte's script): everything that
// reads or writes the `faces` store plus IPC -- the People rail and rename/reassign/tag actions
// (P3c), and the current-photo face refresh plus the three manual detection entry points (P4c),
// which also read the selection and the filtered library.

import { faces } from "$lib/state/faces.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { library } from "$lib/state/library.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import {
  listPeople,
  renamePerson,
  reassignFace,
  createPerson,
  setFaceExcluded,
  cancelFaceDetection,
  getFacesForImage,
  detectFacesForImages,
} from "$lib/api/faces.js";
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

/** Refetches the faces for whichever photo is currently selected --
 * called on selection change (see the `$effect` in +page.svelte) and after any
 * tag/rename/reassign/exclude/detect action touches the selected photo. */
export async function refreshCurrentImageFaces() {
  if (!selection.selectedImage) {
    faces.currentImageFaces = [];
    return;
  }
  faces.currentImageFaces = await getFacesForImage(selection.selectedImage.image_id);
}

/** Shared runner behind all three manual detection entry points --
 * on-demand detection outside the import flow (People-tab UX fix,
 * 2026-09-16, relocated into Library by the 2026-09-17 redesign).
 * `cancelable` controls whether a Cancel affordance is shown; the tiny
 * single-photo case passes `false` (nothing worth canceling). */
export async function runFaceDetection(/** @type {number[]} */ imageIds, /** @type {boolean} */ cancelable) {
  if (faces.detectingFaces || imageIds.length === 0) return;
  faces.detectingFaces = true;
  faces.detectionCancelable = cancelable;
  faces.scanProgress = { current: 0, total: imageIds.length };
  try {
    await detectFacesForImages(imageIds);
    library.images = library.images.map((img) => (imageIds.includes(img.image_id) ? { ...img, faces_scanned: true } : img));
    await refreshCurrentImageFaces();
    await refreshPeople();
  } catch (/** @type {any} */ e) {
    shell.notify(`Face detection failed: ${e}`);
  } finally {
    faces.detectingFaces = false;
    faces.detectionCancelable = false;
    faces.scanProgress = null;
  }
}

/** MetadataPanel's per-photo "Face" button. */
export function handleDetectFacesForSelected() {
  if (!selection.selectedImage) return;
  runFaceDetection([selection.selectedImage.image_id], false);
}

/** Library's multi-select batch action -- every selected photo not yet
 * scanned (already-scanned photos are silently skipped, not re-run). */
export function handleDetectFacesForSelection() {
  const targets = selection.selectedImages.filter((img) => !img.faces_scanned).map((img) => img.image_id);
  if (targets.length === 0) {
    shell.notify("Every selected photo has already been scanned for faces.");
    return;
  }
  runFaceDetection(targets, true);
}

/** "Detect Faces in Folder" -- every not-yet-scanned photo in the
 * current CatalogRail scope (a folder, a collection, or "All Photos" --
 * whatever `filteredImages` already reflects), same scope Library's
 * other bulk actions use. */
export function handleDetectFacesForFolder() {
  const targets = library.filteredImages.filter((img) => !img.faces_scanned).map((img) => img.image_id);
  if (targets.length === 0) {
    shell.notify("Every photo in this view has already been scanned for faces.");
    return;
  }
  runFaceDetection(targets, true);
}
