// Similar view workflows (RFC-0031 slice C): entering and leaving the view, fetching groups (on entry, on a slider
// change, and while the background analysis is still measuring photos), opening a group, and "Reject the others".

import { listSimilarGroups, cullAnalysisStatus } from "$lib/api/cull.js";
import { setFlag } from "$lib/api/catalog.js";
import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { similar } from "$lib/state/similar.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { patchLocal, refresh } from "$lib/actions/libraryActions.js";
import { rejectOthersTargets } from "$lib/similar.js";

const POLL_MS = 2000;
const SLIDER_DEBOUNCE_MS = 150;

/** @type {ReturnType<typeof setTimeout> | null} */
let sliderTimer = null;
/** @type {ReturnType<typeof setInterval> | null} */
let pollTimer = null;
/** Bumped by every fetch so a slow answer for an old slider position never replaces a newer one. */
let fetchSeq = 0;

/** Fetches the groups for the current slider position and the analysis progress. Failures leave what is shown as it is. */
export async function refreshSimilar() {
  const seq = ++fetchSeq;
  try {
    const [groups, status] = await Promise.all([listSimilarGroups(similar.similarity), cullAnalysisStatus()]);
    if (seq !== fetchSeq) return;
    similar.groups = groups;
    similar.status = status;
    similar.loaded = true;
  } catch (e) {
    console.error("similar groups failed", e);
  }
}

/** Turns the Similar view on: leaves any other rail source and starts keeping the groups current. */
export function showSimilar() {
  library.activeCollectionId = null;
  library.activeFolderKey = null;
  library.showLastImportOnly = false;
  library.activePersonId = null;
  library.activeMapImageIds = null;
  similar.active = true;
  similar.openKey = null;
  refreshSimilar();
  startPolling();
}

/** Leaves the view (the rail's other sources do the same through `similar.leave()`, and the poll then stops itself). */
export function leaveSimilar() {
  similar.leave();
  stopPolling();
}

/** While the view is on and photos are still being measured, groups are fetched again as the count moves. */
function startPolling() {
  stopPolling();
  pollTimer = setInterval(async () => {
    if (!similar.active) return stopPolling();
    const before = similar.status?.analysed ?? -1;
    try {
      const status = await cullAnalysisStatus();
      if (status.analysed !== before || status.total !== similar.status?.total) await refreshSimilar();
    } catch {
      // the next tick tries again
    }
  }, POLL_MS);
}

function stopPolling() {
  if (pollTimer !== null) clearInterval(pollTimer);
  pollTimer = null;
}

/** The slider moved: remember it and fetch the groups for it once it rests. */
export function setSimilarity(/** @type {number} */ value) {
  similar.similarity = Math.min(1, Math.max(0, value));
  similar.saveSimilarity();
  if (sliderTimer !== null) clearTimeout(sliderTimer);
  sliderTimer = setTimeout(() => {
    sliderTimer = null;
    refreshSimilar();
  }, SLIDER_DEBOUNCE_MS);
}

/** Opens a group in the side-by-side view with its suggested frame (or the first) selected. */
export function openSimilarGroup(/** @type {number} */ key) {
  const group = similar.groupViews.find((g) => g.key === key);
  if (!group) return;
  similar.openKey = key;
  selectOnly((group.best ?? group.frames[0]).image.version_id);
}

export function closeSimilarGroup() {
  similar.openKey = null;
  similar.pendingReject = null;
}

/** The one selected frame: P / X and the toolbar act on exactly it. */
export function selectOnly(/** @type {number} */ versionId) {
  selection.selectedId = versionId;
  selection.selectedIds = new Set([versionId]);
}

/** Asks to reject every frame of the open group but `keepVersionId`; the confirmation dialog is shown from the store. */
export function askRejectOthers(/** @type {number} */ keepVersionId) {
  const group = similar.openGroup;
  if (!group) return;
  const versionIds = rejectOthersTargets(group, keepVersionId);
  if (versionIds.length === 0) {
    shell.notify("Nothing to reject: the other photos are already rejected");
    return;
  }
  similar.pendingReject = { keepVersionId, versionIds };
}

export function cancelRejectOthers() {
  similar.pendingReject = null;
}

/** Sets the Reject flag on the pending photos: optimistic local patch, then the catalog write (the flag is all it changes). */
export async function confirmRejectOthers() {
  const pending = similar.pendingReject;
  similar.pendingReject = null;
  if (!pending) return;
  for (const id of pending.versionIds) patchLocal(id, { flag: "reject" });
  try {
    await Promise.all(pending.versionIds.map((id) => setFlag(id, "reject")));
    shell.notify(`Rejected ${pending.versionIds.length} photo${pending.versionIds.length === 1 ? "" : "s"}`);
  } catch (e) {
    // The catalog is the truth: put the list back from it rather than leave a flag that was not saved.
    console.error("reject others failed", e);
    shell.notify("Could not save the Reject flags");
    await refresh();
  }
}
