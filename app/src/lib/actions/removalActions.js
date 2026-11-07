// Object removal (RFC-0032 slice E): arming the tool, the marks, running the job, and writing its patches into the
// edit stack. The job is the backend's (`remove_object`); what lands in the stack is one `remove` op whose items name
// the stored patches (api/develop.js), written like any edit so it gets a history row and can be undone.

import { develop } from "$lib/state/develop.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { removal, LARGE_MARK_FRACTION } from "$lib/state/removal.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { regenerateThumbnailFor } from "$lib/actions/importActions.js";
import { removalAvailable, removeObject, removalCancel, isRemovalCancelled, removalHole, removalsWithMask } from "$lib/api/removal.js";
import { segmentModelStatus, segmentPrepare, segmentDecode, segmentRelease, isNotPrepared } from "$lib/api/segment.js";
import { readCut } from "$lib/segmentCut.js";
import { addRemovals, getEditStack, getRemovals, removeRemoval, setEditStack, addMask, createSpotMask, listMasks, countBrushLayers, MAX_MASKS } from "$lib/api/develop.js";
import { planHeal } from "$lib/healFromMarks.js";
import { readMaskPng } from "$lib/maskRaster.js";

/** @param {unknown} e */
function message(e) {
  return String(e).replace(/^Error:\s*/, "");
}

/** The Remove button: arms the tool (checking the model once), or leaves it, dropping marks not yet removed. */
export async function handleRemoveToolToggle() {
  if (removal.active) {
    leaveRemoveTool();
    return;
  }
  masks.shapeTarget = null;
  masks.selectedMaskId = null;
  masks.activeTool = "remove";
  removal.error = "";
  removal.notice = "";
  removal.selectNote = "";
  removal.clearMarks();
  try {
    removal.available = await removalAvailable();
  } catch {
    removal.available = false;
  }
  if (removal.mode === "select" && removal.active) await startSelect();
}

/** Brush / Lasso / Select. The object being refined becomes an ordinary mark when the user changes mode.
 * @param {"brush" | "lasso" | "select"} mode */
export async function handleRemoveMode(mode) {
  if (removal.phase === "running") return;
  commitSelection();
  removal.mode = mode;
  if (mode === "select") await startSelect();
}

/** Keeps the object being refined as a mark of its own, so the next click starts another object. */
function commitSelection() {
  if (!removal.selection) return;
  removal.shapes = [...removal.shapes, removal.selection.mark];
  removal.selection = null;
}

/** Checks the click-select model and prepares the photograph for clicks. Without the model it says so (the model is
 * installed from Select Subject, which owns the consent dialog) and Brush and Lasso keep working. */
async function startSelect() {
  removal.selectNote = "";
  removal.selectPhase = "preparing";
  try {
    const status = await segmentModelStatus();
    if (!removal.active) return;
    if (status.state !== "ready") {
      removal.selectPhase = "idle";
      removal.selectNote = "Click-select needs its model: install it once from Select Subject, then come back.";
      return;
    }
    await prepareSelect();
  } catch (e) {
    if (!removal.active) return;
    removal.selectPhase = "idle";
    removal.selectNote = message(e);
  }
}

async function prepareSelect() {
  const hash = develop.imageContentHash;
  if (!develop.imagePath || !hash) {
    removal.selectPhase = "idle";
    removal.selectNote = "This photo has no preview to select from yet.";
    return false;
  }
  removal.selectPhase = "preparing";
  await segmentPrepare(develop.imagePath, hash);
  if (!removal.active) {
    // The tool was left while the encoder ran: let the helper go again.
    void segmentRelease().catch(() => {});
    return false;
  }
  removal.preparedHash = hash;
  removal.selectPhase = "ready";
  return true;
}

/** Frees the helper's embedding when the tool is left. Never starts the helper. */
function releaseSelect() {
  removal.selectPhase = "idle";
  removal.selectNote = "";
  if (removal.preparedHash !== null) {
    removal.preparedHash = null;
    void segmentRelease().catch(() => {});
  }
}

/** A click in select mode. A click takes the object under it (or, with an object already taken, adds to it); Alt-click
 * takes a part away; Shift-click starts another object. Refines from the candidate shown.
 * @param {{ x: number, y: number, positive: boolean, another?: boolean }} click */
export async function handleRemoveSelectClick(click) {
  if (removal.phase === "running" || removal.mode !== "select") return;
  if (removal.selectPhase === "idle") {
    // A failed or missing-model start: a click tries again.
    await startSelect();
    if (/** @type {string} */ (removal.selectPhase) !== "ready") return;
  }
  if (/** @type {string} */ (removal.selectPhase) !== "ready") return;
  if (click.another) commitSelection();
  if (!click.positive && !removal.selection) return;
  const gen = removal.generation;
  const hash = develop.imageContentHash;
  if (!hash) return;
  const sel = removal.selection;
  const prompts = [...(sel?.prompts ?? []), { x: click.x, y: click.y, positive: click.positive }];
  const refine = sel ? sel.candidates[sel.index]?.logits_png ?? null : null;
  removal.selectPhase = "deciding";
  removal.selectNote = "";
  removal.guarding = false;
  removal.error = "";
  removal.notice = "";
  try {
    /** @type {import('$lib/api/segment.js').SegmentCandidate[]} */
    let candidates;
    try {
      if (removal.preparedHash !== hash && !(await prepareSelect())) return;
      removal.selectPhase = "deciding";
      candidates = await segmentDecode(hash, prompts, refine);
    } catch (e) {
      if (!isNotPrepared(e)) throw e;
      // The helper restarted (idle exit) since the photograph was prepared: prepare again, decode once more.
      if (gen !== removal.generation || !(await prepareSelect())) return;
      removal.selectPhase = "deciding";
      candidates = await segmentDecode(hash, prompts, refine);
    }
    if (gen !== removal.generation) return;
    const first = candidates[0];
    const cut = first ? await readCut(first.logits_png) : null;
    if (gen !== removal.generation) return;
    if (!first || !cut) {
      // The click that found nothing is dropped; what was selected before stays.
      removal.selectNote = "Nothing was selected there.";
      return;
    }
    removal.selection = { prompts, candidates, index: 0, mark: { kind: "segment", logits: first.logits_png, bbox: cut.bbox, tint: cut.tint } };
  } catch (e) {
    if (gen !== removal.generation) return;
    removal.selectNote = message(e);
  } finally {
    if (removal.selectPhase === "deciding") removal.selectPhase = "ready";
  }
}

/** Shows the model's next guess for the clicks so far (Tab). */
export async function handleRemoveNextGuess() {
  const sel = removal.selection;
  if (!sel || sel.candidates.length < 2 || removal.phase === "running" || removal.selectPhase !== "ready") return;
  const gen = removal.generation;
  const index = (sel.index + 1) % sel.candidates.length;
  const cut = await readCut(sel.candidates[index].logits_png);
  if (!cut || gen !== removal.generation || removal.selection !== sel) return;
  removal.guarding = false;
  removal.selection = { ...sel, index, mark: { kind: "segment", logits: sel.candidates[index].logits_png, bbox: cut.bbox, tint: cut.tint } };
}

/** Leaves the tool. Marks not yet removed are dropped; a running job is stopped. */
export function leaveRemoveTool() {
  if (removal.phase === "running") void removalCancel();
  removal.clearMarks();
  releaseSelect();
  if (masks.activeTool === "remove") masks.activeTool = null;
}

/** @param {import('$lib/api/removal.js').MaskShape} shape */
export function handleRemoveMark(shape) {
  if (removal.phase === "running") return;
  commitSelection();
  removal.shapes = [...removal.shapes, shape];
  removal.guarding = false;
  removal.error = "";
  removal.notice = "";
}

/** Drops the last mark (Backspace / Undo while marks are pending). */
export function handleRemoveUnmark() {
  if (removal.phase === "running" || removal.count === 0) return;
  if (removal.selection) removal.selection = null;
  else removal.shapes = removal.shapes.slice(0, -1);
  removal.guarding = false;
}

export function handleRemoveClear() {
  if (removal.phase === "running") return;
  removal.clearMarks();
}

/** Stops the running job; nothing is stored. */
export function handleRemoveCancel() {
  if (removal.phase === "running") void removalCancel();
}

/** The photograph's width / height as the canvas knows it (1 until it is known). */
function photoAspect() {
  return develop.imageNativeWidth && develop.imageNativeHeight ? develop.imageNativeWidth / develop.imageNativeHeight : 1;
}

/** The Spot Removal (heal) mask for `plan`, or null (after saying why) when the stack has no room for another mask. */
function healMask(/** @type {{ dabs: import('$lib/healFromMarks.js').HealDab[], sourceOffset: { dx: number, dy: number } }} */ plan) {
  if (listMasks(develop.editStack).length >= MAX_MASKS || countBrushLayers(develop.editStack) >= MAX_MASKS) {
    shell.notify(`Maximum ${MAX_MASKS} masks reached: delete one to heal this area`);
    return null;
  }
  return { ...createSpotMask(plan.dabs[0]), dabs: plan.dabs, sourceOffset: plan.sourceOffset };
}

/** "Heal instead": the pending marks become a Spot Removal (heal) mask, the manual tool, instead of going to the model. A
 * Brush or Lasso mark becomes dabs from its geometry, a selected object from its cut. The mask is selected for the user to
 * adjust its source and softness; the marks are dropped and the tool is left. */
export async function handleRemoveHealInstead() {
  if (develop.versionId === null || removal.phase === "running" || removal.count === 0) return;
  const shapes = removal.allShapes();
  const gen = removal.generation;
  /** @type {import('$lib/healFromMarks.js').MaskGrid[]} */
  const grids = [];
  for (const s of shapes) {
    if (s.kind !== "segment") continue;
    const cut = await readMaskPng(s.logits);
    if (gen !== removal.generation) return; // the marks were cleared meanwhile
    if (!cut) {
      shell.notify("The selected object could not be read");
      return;
    }
    grids.push({ box: [0, 0, 1, 1], ...cut });
  }
  const plan = planHeal(shapes, photoAspect(), grids);
  if ("reason" in plan) {
    shell.notify(plan.reason);
    return;
  }
  const mask = healMask(plan);
  if (!mask) return;
  develop.editStack = addMask(develop.editStack, mask);
  leaveRemoveTool();
  masks.selectedMaskId = mask.id;
  develop.scheduleFlush("Heal instead");
  shell.notify("Healing with Spot Removal: adjust the source or the softness in the mask panel");
}

/** "Heal instead" for a removal that is already in the picture: its stored hole becomes a Spot Removal (heal) mask and the
 * removal comes out of the stack, in one edit (History brings the removal back). The removal stays stored.
 * @param {number} patch */
export async function handleRemoveHealRemoval(patch) {
  const versionId = develop.versionId;
  if (versionId === null || removal.phase === "running" || !getRemovals(develop.editStack).some((i) => i.patch === patch)) return;
  let hole;
  try {
    hole = await removalHole(versionId, patch);
  } catch (e) {
    shell.notify(message(e));
    return;
  }
  if (develop.versionId !== versionId) return;
  const cut = hole ? await readMaskPng(hole.png) : null;
  if (!hole || !cut) {
    shell.notify(hole ? "The stored hole could not be read" : "This removal has no stored hole to heal from");
    return;
  }
  if (develop.versionId !== versionId || !getRemovals(develop.editStack).some((i) => i.patch === patch)) return;
  const box = /** @type {[number, number, number, number]} */ ([hole.x / hole.src_w, hole.y / hole.src_h, (hole.x + cut.w) / hole.src_w, (hole.y + cut.h) / hole.src_h]);
  const plan = planHeal([], hole.src_w / hole.src_h, [{ box, ...cut }]);
  if ("reason" in plan) {
    shell.notify(plan.reason);
    return;
  }
  const mask = healMask(plan);
  if (!mask) return;
  develop.editStack = addMask(removeRemoval(develop.editStack, patch), mask);
  removal.notice = "";
  masks.selectedMaskId = mask.id;
  await develop.flushEditStack("Heal instead");
  regenerateThumbnailFor(versionId);
  shell.notify("Healing with Spot Removal: adjust the source or the softness in the mask panel");
}

/** Reads which of the open photograph's removals have a stored hole into `removal.healable`. Call when the list changes.
 * @param {number | null} versionId @param {number[]} patchIds */
export async function refreshHealable(versionId, patchIds) {
  if (versionId === null || patchIds.length === 0) {
    removal.healable = [];
    return;
  }
  try {
    const ids = await removalsWithMask(versionId, patchIds);
    if (develop.versionId === versionId) removal.healable = ids;
  } catch {
    if (develop.versionId === versionId) removal.healable = [];
  }
}

/** Runs the removal on the marks. Large marks ask once first (`confirmLarge` is the second press). */
export async function handleRemoveRun(confirmLarge = false) {
  const versionId = develop.versionId;
  if (versionId === null || removal.phase === "running" || removal.count === 0) return;
  const aspect = photoAspect();
  if (!confirmLarge && removal.extent(aspect) > LARGE_MARK_FRACTION) {
    removal.guarding = true;
    return;
  }
  removal.guarding = false;
  removal.phase = "running";
  removal.error = "";
  removal.notice = "";
  // A plain copy for the IPC call (the marks are reactive state).
  // The job reads a selected object's cut; its box and tint are for the screen.
  const shapes = removal.allShapes().map((m) => (m.kind === "segment" ? { kind: m.kind, logits: m.logits } : JSON.parse(JSON.stringify(m))));
  try {
    const results = await removeObject(versionId, shapes);
    const added = results.map((r) => ({ patch: r.patch_id, engine: r.engine }));
    await writeRemovals(versionId, added);
    removal.clearMarks();
    const n = results.length;
    removal.notice = `Removed ${n} ${n === 1 ? "thing" : "things"}${results.some((r) => r.reduced) ? " (a large area: the fill may look soft)" : ""}`;
  } catch (e) {
    if (isRemovalCancelled(e)) removal.notice = "Cancelled";
    else removal.error = message(e);
  } finally {
    removal.phase = "idle";
  }
}

/** Names the patches in the stack of `versionId`: through the open photo's store when it is still the open one (so the
 * canvas, the history list and the flush all follow), straight to the catalog when the user moved on meanwhile.
 * @param {number} versionId @param {{ patch: number, engine: string }[]} added */
async function writeRemovals(versionId, added) {
  if (develop.versionId === versionId) {
    develop.editStack = addRemovals(develop.editStack, added);
    await develop.flushEditStack("Remove object");
  } else {
    await setEditStack(versionId, addRemovals(await getEditStack(versionId), added), "Remove object");
    shell.notify("Removed from the photo you had open");
  }
  regenerateThumbnailFor(versionId);
}

/** Takes the newest removal out of the stack again (its patch stays stored, so History can bring it back). */
export async function handleRemoveUndoLast() {
  const items = getRemovals(develop.editStack);
  if (items.length === 0 || removal.phase === "running") return;
  await takeRemovalOut(items[items.length - 1].patch, "Undo remove");
}

/** Takes one listed removal out of the stack, whichever it is (its patch stays stored, so History can bring it back).
 * @param {number} patch */
export async function handleRemoveDelete(patch) {
  if (removal.phase === "running" || !getRemovals(develop.editStack).some((i) => i.patch === patch)) return;
  await takeRemovalOut(patch, "Delete removal");
}

/** @param {number} patch @param {string} label */
async function takeRemovalOut(patch, label) {
  develop.editStack = removeRemoval(develop.editStack, patch);
  removal.notice = "";
  await develop.flushEditStack(label);
  regenerateThumbnailFor(develop.versionId);
}
