// Photo metadata edits (RFC-0009 P5b, moved out of +page.svelte's script): rating / flag / color label
// (optimistic local patch, then the catalog write, on the whole selection or the acted-on cell) and
// the IPTC caption / copyright / contact fields that save on blur.

import { targetVersionIds } from "$lib/actions/selectionActions.js";
import { patchLocal } from "$lib/actions/libraryActions.js";
import { setRating, setFlag, setColorLabel, setCaption, setCopyright, setContact } from "$lib/api/catalog.js";
import { develop } from "$lib/state/develop.svelte.js";
import { library } from "$lib/state/library.svelte.js";

export async function handleRatingChange(/** @type {number | null | undefined} */ versionId, /** @type {number} */ rating) {
  const targets = targetVersionIds(versionId);
  if (targets.length === 0) return;
  for (const id of targets) patchLocal(id, { rating });
  await Promise.all(targets.map((id) => setRating(id, rating)));
}

export async function handleFlagChange(/** @type {number | null | undefined} */ versionId, /** @type {string} */ flag) {
  const targets = targetVersionIds(versionId);
  if (targets.length === 0) return;
  for (const id of targets) patchLocal(id, { flag });
  await Promise.all(targets.map((id) => setFlag(id, flag)));
}

export async function handleColorLabelChange(/** @type {number | null | undefined} */ versionId, /** @type {string} */ colorLabel) {
  const targets = targetVersionIds(versionId);
  if (targets.length === 0) return;
  for (const id of targets) patchLocal(id, { color_label: colorLabel });
  await Promise.all(targets.map((id) => setColorLabel(id, colorLabel)));
}

// M2 Slice 2: IPTC fields save on blur (MetadataPanel), not debounced --
// each is a single discrete edit rather than a slider drag, so there's no
// flood of writes to coalesce. Still tracked via develop.trackIptcSave so the
// close handler can wait for an in-flight write the same way it already
// does for the Develop edit stack.
export function handleCaptionChange(/** @type {number} */ versionId, /** @type {string} */ caption) {
  patchLocal(versionId, { caption });
  develop.trackIptcSave(setCaption(versionId, caption));
}

export function handleCopyrightChange(/** @type {number} */ imageId, /** @type {string} */ copyright) {
  library.images = library.images.map((img) => (img.image_id === imageId ? { ...img, copyright } : img));
  develop.trackIptcSave(setCopyright(imageId, copyright));
}

export function handleContactChange(/** @type {number} */ imageId, /** @type {string} */ contact) {
  library.images = library.images.map((img) => (img.image_id === imageId ? { ...img, contact } : img));
  develop.trackIptcSave(setContact(imageId, contact));
}
