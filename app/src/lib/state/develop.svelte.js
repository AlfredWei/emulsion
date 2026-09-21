// Develop session state (RFC-0009 §3.3, P5a): which image is open, its live edit stack, the
// undo history and snapshots, the hover-preview thumbnail, copied settings, the canvas readouts
// DevelopCanvas reports up, and the GPU-fallback flags -- plus the persistence of the edit stack.
//
// Persistence lives here, not in an action module, because its variables (the debounce timer, the
// pending label, the in-flight edit save and the in-flight IPTC save) must move together and must
// not be reachable from outside (RFC-0009 §3.6, invariants I4/I5): everything that needs to
// cancel, schedule, flush or wait goes through the methods below, and the window-close flush uses
// `hasPendingWork` / `flushPending()` rather than reading the promises.
//
// What is NOT here: the 23 per-adjustment projections of `editStack` (developView.svelte.js), and
// the multi-store workflows that also touch masks/library/selection -- restore, snapshot restore,
// reset, open/close Develop (lib/actions, later steps).

import { library } from "./library.svelte.js";
import { setEditStack } from "$lib/api/develop.js";
import { convertFileSrc } from "@tauri-apps/api/core";

export class DevelopStore {
  /** @param {import('./library.svelte.js').LibraryStore} library */
  constructor(library) {
    // Set before any derived below is first read (deriveds are lazy).
    this.library = library;
  }

  versionId = $state(/** @type {number | null} */ (null));
  imagePath = $state("");
  // M4 Smart Previews: derived, not a second $state var kept manually in
  // sync with imagePath -- stays correct automatically as `images`
  // updates, and there's exactly one place (`openDevelop`) that ever
  // needs to change which image is open anyway.
  imageContentHash = $derived(
    this.library.images.find((img) => img.version_id === this.versionId)?.content_hash ?? null,
  );
  /** @type {import('$lib/api/develop.js').EditStack} */
  editStack = $state({ schema_version: 1, ops: [] });

  // History/Undo/Snapshots (M3). `history` is the current version's full
  // list (oldest first, matching Catalog::get_history's own ORDER BY id
  // ASC); `historyIndex` is a plain array index into it -- NOT a value
  // persisted anywhere -- representing "which entry does the live
  // editStack currently match." -1 means "before the first history
  // entry" (the version's untouched initial state, before any labeled
  // edit has ever been recorded). Reset to "newest" on every real edit
  // and whenever Develop (re)opens for an image; moved directly by
  // undo/redo/History-panel-click via restoreTo. See flushEditStack's own
  // doc comment for why no separate cursor concept needs to exist
  // server-side.
  history = $state(/** @type {import('$lib/api/develop.js').HistoryEntry[]} */ ([]));
  historyIndex = $state(-1);
  snapshots = $state(/** @type {import('$lib/api/develop.js').SnapshotEntry[]} */ ([]));

  // Undo is disabled at historyIndex 0 (the version's oldest-ever labeled
  // edit) -- there's no history row representing "before the first edit"
  // to restore TO (edit_history only ever gains a row once a real edit
  // happens; the version's original untouched state is never itself
  // stored as one). A named, accepted scope cut, not a bug: the very
  // first edit ever made to a photo simply can't be undone via Ctrl+Z,
  // matching this session's "mid-drag undo guard" precedent of a
  // documented limitation over unrequested complexity (a synthetic
  // "Import" seed row, which real Lightroom itself uses for exactly this
  // reason -- deliberately out of scope here).
  canUndo = $derived(this.historyIndex > 0);
  canRedo = $derived(this.historyIndex < this.history.length - 1);

  // History/Snapshot/Preset hover-preview (M4.5): a small rendered
  // thumbnail in the rail's own preview box, NOT a swap of the live
  // `editStack` -- an earlier version of this feature did swap `editStack`
  // (reusing DevelopCanvas's existing reactive WebGPU render for free),
  // but that meant every hover drove the full interactive canvas pipeline,
  // the same cost as a real edit, just to preview one. This instead
  // reuses the existing CPU-rendered "graded" preview tier
  // (`preview_cache::ensure_graded_preview_for_hash`, the same one
  // Library's Loupe view already uses) at draft resolution, cached by
  // content hash + stack hash -- cheap after the first hover of any given
  // entry, and never touches the main canvas at all. `previewToken`
  // invalidates a still-in-flight render once the user has moved to a
  // different row (or left) before it resolves; debounced so quickly
  // scanning across rows doesn't fire one render per row passed over.
  previewUrl = $state(/** @type {string | null} */ (null));
  #previewToken = 0;
  /** @type {ReturnType<typeof setTimeout> | null} */
  #previewDebounceTimer = null;

  // Copy/Paste Settings (M4.5): an unsaved, in-memory analog of Presets --
  // "Copy Settings" snapshots a filtered subset of the CURRENTLY OPEN
  // Develop image's own edit stack (via copySettingsOps, over the exact
  // same preset-eligible op universe presetEligibleOps already defines);
  // "Paste Settings" applies that snapshot with the SAME applyPresetOps
  // upsert-by-name merge Presets use, so it carries the identical
  // whole-op-replace limitation documented there. Deliberately reuses
  // this machinery rather than inventing a second merge strategy.
  copiedSettings = $state(/** @type {import('$lib/api/develop.js').EditStack | null} */ (null));

  // The crop rect (crop.x/y/width/height) lives in NORMALIZED space --
  // fractions of the source image's own width/height, which are generally
  // NOT equal. A preset ratio like 1 (1:1) or 16/9 describes a PIXEL
  // aspect ratio, so it has to be corrected by the image's own native
  // aspect ratio before it's usable as a normalized width:height target --
  // otherwise "1:1" only looks square in normalized space, which is a
  // real square only when the source image itself happens to be square.
  // DevelopCanvas.svelte already tracks the decoded bitmap's pixel
  // dimensions (it needs them for the committed-crop CSS preview's own
  // `aspect-ratio` style); it reports them up here via onSourceDimensions
  // since this preset math needs them too.
  sourceWidth = $state(0);
  sourceHeight = $state(0);

  // Develop histogram: fed live from DevelopCanvas's own GPU readback
  // (see that component's onHistogramUpdate/readHistogramIfIdle) --
  // deliberately NOT derived from editStack/exposure/etc. here, since the
  // actual graded pixel values (masks, curves, every spatial op) aren't
  // reproducible from JS-side state alone; DevelopCanvas is the only
  // place that ever sees the real rendered output.
  histogramData = $state(/** @type {{r: Uint32Array, g: Uint32Array, b: Uint32Array} | null} */ (null));

  // Histogram clipping-overlay toggle: purely a display preference (not
  // part of the edit stack), reset on openDevelop like histogramData
  // itself since it's meaningless outside a Develop session.
  showClippingOverlay = $state(false);

  // Histogram "value under cursor" readout, fed live from DevelopCanvas's
  // own pointer handling (see reportHoverPixel there) -- same
  // GPU-readback-can't-be-reproduced-from-JS-state reasoning as
  // histogramData above.
  hoverPixel = $state(/** @type {{r: number, g: number, b: number} | null} */ (null));

  // Aspect-ratio lock: UI-only, NOT persisted to the edit stack (real
  // Lightroom's own crop ratio lock is a tool-state preference, not part
  // of the photo's own edit history) -- shared between DevelopCanvas.svelte
  // (corner-handle drag math) and MaskToolStrip.svelte (the preset
  // buttons' own active-state display), so it has to live here, their
  // nearest common ancestor.
  cropAspectLock = $state(/** @type {number | null} */ (null));

  // HSL band-jump eyedropper's transient navigation target -- NOT persisted
  // edit-stack state, purely a "which band should the panel scroll to and
  // highlight" signal, self-clearing after a fixed delay rather than on
  // "the next unrelated interaction" (which would mean hooking an unbounded
  // set of DOM listeners across the panel). Same fixed-timeout-reset-on-
  // retrigger idiom as persistTimer's own debounce, just for UI feedback
  // instead of persistence.
  highlightedHslBand = $state(/** @type {string | null} */ (null));

  // M4 Slice 2: before/after preview -- when true, DevelopCanvas shows the
  // image as it would look with NO edits applied (skips the masks pass
  // entirely) instead of the live graded result, toggleable via the \
  // hotkey (handleGlobalKeydown) for a quick before/after comparison,
  // matching real Lightroom's own \ convention. Deliberately a toggle, not
  // a press-and-hold -- simpler and more reliable to implement correctly,
  // and matches Lightroom's own default behavior for this exact key.
  showOriginal = $state(false);

  // M4 Slice 3: holding Space temporarily overrides whatever tool is
  // active so the user can pan a zoomed-in view without switching tools --
  // real Photoshop/Lightroom convention. Set by handleGlobalKeydown/
  // handleGlobalKeyup (a press-and-hold, unlike showOriginal/
  // maskOverlaysVisible's own toggles, since panning only makes sense
  // while the key is physically down); also cleared on window blur so an
  // Alt-Tab away mid-hold can't leave this stuck true forever with no
  // keyup ever arriving to clear it.
  spacePanning = $state(false);

  // M5 Slice 1: GPU/CPU fallback. `gpuFallbackActive` is reported up by
  // DevelopCanvas's own `onGpuFallback` callback the moment its WebGPU
  // device acquisition fails (or, symmetrically, flips back false if a
  // later image's acquisition succeeds) -- this component never probes
  // `navigator.gpu` itself. `cpuFallbackPreviewUrl` is populated by the
  // debounced effect in +page.svelte, same "compute a static preview CPU-side and
  // hand DevelopCanvas a URL" shape as `softProofPreviewUrl` above, just
  // driven by GPU availability instead of a proofing toggle.
  gpuFallbackActive = $state(false);
  cpuFallbackPreviewUrl = $state(/** @type {string | null} */ (null));

  // Persistence is debounced (not written on every slider tick) so a drag
  // doesn't flood the catalog with writes -- flushed immediately whenever
  // navigation could otherwise lose the pending change (UX-DESIGN.md §5's
  // "coalesced/debounced slider events" rule, applied to catalog writes
  // rather than the WebGPU frame loop).
  /** @type {ReturnType<typeof setTimeout> | null} */
  #persistTimer = null;
  // The label for whatever edit is currently sitting behind persistTimer's
  // debounce -- set by scheduleFlush, consumed (and cleared) by the next
  // flushEditStack call, whichever call site triggers it (the timer
  // itself, or an early explicit flush like openDevelop's). Kept as a
  // module-level variable rather than a flushEditStack parameter so every
  // existing `await flushEditStack()` call site (switchModule, openDevelop,
  // Export, window-close) keeps working unchanged: it always means "flush
  // whatever's actually pending, under whatever label it was scheduled
  // with -- or nothing, if nothing is pending."
  /** @type {string | null} */
  #pendingLabel = null;
  // Tracks an in-flight (already-fired, not-yet-resolved) save separately
  // from the debounce timer -- a flush can be triggered again (e.g. by the
  // window-close handler) while a previous flush's write is still in
  // flight, and callers need to be able to wait for *that* too, not just
  // "is a timer currently pending".
  /** @type {Promise<void> | null} */
  #pendingSave = null;

  // M2 Slice 2: same shape as pendingSave above, but for the IPTC fields'
  // save-on-blur writes -- tracked separately since it's a different
  // in-flight write than the Develop edit stack's, and the close handler
  // below needs to wait on whichever (or both) are actually pending.
  /** @type {Promise<void> | null} */
  #pendingIptcSave = null;

  // M1 Slice 6: this used to NOT return setEditStack's promise, so every
  // `await flushEditStack()` call site (switchModule, openDevelop, Export)
  // resolved on the next microtask regardless of whether the write had
  // actually reached Rust/SQLite yet -- silently fire-and-forget. Fixed to
  // return the real promise; this is the fix the window-close flush below
  // actually depends on to mean anything.
  //
  // Reset-button slice: the write itself used to be gated on
  // `persistTimer !== null` -- correct for every slider-driven caller
  // (handleAdjustmentChange etc. always set persistTimer right before a
  // later flush), but a REAL bug for handleMaskDeleted and
  // handleResetEditStack below, which mutate `editStack` directly and call
  // this expecting an immediate write: since neither ever sets
  // persistTimer first, the old gate silently skipped the write entirely.
  // Verified empirically (a mask deletion's own persist was being silently
  // dropped -- the mask would incorrectly reappear after a full quit and
  // reopen, since the catalog was never actually updated). Fixed by always
  // writing whenever a develop image is open, regardless of whether a
  // timer happened to be pending -- harmless when nothing changed (an
  // idempotent re-write of the same stack), correct when something did.
  // History/Undo/Snapshots (M3): `label` is the human-readable name for
  // whatever edit is being flushed IMMEDIATELY (mask delete, Reset --
  // callers that never go through scheduleFlush's debounce at all).
  // Everything else (the setTimeout(flushEditStack, 250) debounce settle,
  // and every "flush whatever's pending before doing X" call site below)
  // omits it, falling back to `pendingLabel` -- whatever scheduleFlush
  // last recorded, or null if nothing is actually pending, in which case
  // this is the same harmless idempotent no-op-content rewrite it always
  // was. Only a truthy label ever moves `historyIndex` -- a no-op flush
  // must never disturb the undo cursor.
  flushEditStack = (/** @type {string=} */ label) => {
    if (this.#persistTimer !== null) {
      clearTimeout(this.#persistTimer);
      this.#persistTimer = null;
    }
    const effectiveLabel = label ?? this.#pendingLabel;
    this.#pendingLabel = null;
    if (this.versionId !== null) {
      const versionId = this.versionId;
      const stack = this.editStack;
      this.#pendingSave = setEditStack(versionId, stack, effectiveLabel ?? undefined)
        .then((freshHistory) => {
          this.history = freshHistory;
          if (effectiveLabel) this.historyIndex = freshHistory.length - 1;
        })
        .finally(() => {
          this.#pendingSave = null;
        });
    }
    return this.#pendingSave ?? Promise.resolve();
  };

  /** Schedules a debounced, LABELED flush -- the replacement for every
   * former `if (persistTimer) clearTimeout(persistTimer); persistTimer =
   * setTimeout(flushEditStack, 250);` call site. Records `label` for
   * flushEditStack to pick up whenever it actually fires (the debounce
   * settling, or an earlier explicit flush elsewhere pre-empting it). */
  scheduleFlush = (/** @type {string} */ label) => {
    this.#pendingLabel = label;
    if (this.#persistTimer) clearTimeout(this.#persistTimer);
    this.#persistTimer = setTimeout(this.flushEditStack, 250);
  };

  /** Cancels a debounced write that has not fired yet (a restore or a removal is about to make the
   * pending edit stale). The label recorded by `scheduleFlush` is left alone -- see
   * `discardPendingLabel`. */
  cancelScheduledFlush() {
    if (this.#persistTimer !== null) {
      clearTimeout(this.#persistTimer);
      this.#persistTimer = null;
    }
  }

  /** Forgets the label of the edit that was waiting behind the debounce, so a later flush is not
   * recorded under it. */
  discardPendingLabel() {
    this.#pendingLabel = null;
  }

  /** IPTC fields save on blur, not debounced -- each is a single discrete edit rather than a slider
   * drag. Tracked here so the window-close flush can wait for an in-flight write the same way it
   * does for the edit stack. */
  trackIptcSave(/** @type {Promise<void>} */ save) {
    this.#pendingIptcSave = save.finally(() => {
      this.#pendingIptcSave = null;
    });
  }

  /** Is an edit-stack write still scheduled or in flight? */
  get hasPendingEdit() {
    return this.#persistTimer !== null || this.#pendingSave !== null;
  }

  /** Is any write -- edit stack or IPTC -- still scheduled or in flight? (The window-close check.) */
  get hasPendingWork() {
    return this.#persistTimer !== null || this.#pendingSave !== null || this.#pendingIptcSave !== null;
  }

  /** Flushes whatever edit is scheduled and waits for it and for any in-flight IPTC write: the two
   * promises the window-close handler awaits. */
  flushPending() {
    return Promise.all([this.flushEditStack(), this.#pendingIptcSave ?? Promise.resolve()]);
  }

  /** mouseleave handler, AND the first thing every real commit path
   * (restoreTo, handleRestoreSnapshot, handleApplyPreset) calls -- a click
   * can land before a stray mouseleave fires, and the stale preview
   * thumbnail should disappear the instant the click actually commits,
   * not linger until the pointer happens to leave. */
  clearPreview = () => {
    this.#previewToken++;
    if (this.#previewDebounceTimer !== null) {
      clearTimeout(this.#previewDebounceTimer);
      this.#previewDebounceTimer = null;
    }
    this.previewUrl = null;
  };

  schedulePreview = (/** @type {() => Promise<import('$lib/api/develop.js').DevelopPreviewInfo>} */ fetchPreview) => {
    if (this.imagePath === null) return;
    const token = ++this.#previewToken;
    if (this.#previewDebounceTimer !== null) clearTimeout(this.#previewDebounceTimer);
    this.#previewDebounceTimer = setTimeout(() => {
      fetchPreview()
        .then((info) => {
          if (token === this.#previewToken) this.previewUrl = convertFileSrc(info.path);
        })
        .catch(() => {});
    }, 120);
  };
}

export function createDevelopStore() {
  return new DevelopStore(library);
}

export const develop = createDevelopStore();
