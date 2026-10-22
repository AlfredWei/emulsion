<script>
  import { convertFileSrc } from "@tauri-apps/api/core";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import LibrarySurveyView from "$lib/components/LibrarySurveyView.svelte";
  import { similar } from "$lib/state/similar.svelte.js";
  import { selection } from "$lib/state/selection.svelte.js";
  import { setSimilarity, openSimilarGroup, closeSimilarGroup, selectOnly, askRejectOthers, cancelRejectOthers, confirmRejectOthers } from "$lib/actions/similarActions.js";
  import { handleRatingChange, handleFlagChange, handleColorLabelChange } from "$lib/actions/metadataActions.js";
  import { openDevelop } from "$lib/actions/navigation.js";
  import { similarityLabel, rejectOthersMessage } from "$lib/similar.js";

  /**
   * The Similar view (RFC-0031 §3.4): frames of one moment as groups, each opening into the side-by-side Survey view
   * with the sharpest frame suggested and soft ones marked. Nothing is changed unless the user picks, rejects or
   * confirms "Reject the others".
   */

  let group = $derived(similar.openGroup);
  let photoCount = $derived(similar.groupViews.reduce((n, g) => n + g.frames.length, 0));
  /** @type {Map<number, { best: boolean, soft: boolean, relative: number }>} */
  let culling = $derived(new Map((group?.frames ?? []).map((f) => [f.image.version_id, { best: f.best, soft: f.soft, relative: f.relative }])));
  let keepVersionId = $derived(selection.selectedId !== null && group?.frames.some((f) => f.image.version_id === selection.selectedId) ? selection.selectedId : null);

  // A group that vanished from under the open view (its photos were removed) returns to the list.
  $effect(() => {
    if (similar.openKey !== null && similar.loaded && !similar.openGroup) similar.openKey = null;
  });

  function thumb(/** @type {import('$lib/api/catalog.js').ImageSummary} */ img) {
    return img.thumbnail_path ? convertFileSrc(img.thumbnail_path) : null;
  }
</script>

<div class="similar" role="region" aria-label="Similar photos">
  <div class="bar">
    {#if group}
      <button type="button" class="btn" onclick={closeSimilarGroup}>‹ Similar</button>
      <span class="title">Group of {group.frames.length}</span>
      <span class="hint">{group.best ? "The sharpest frame is suggested; nothing changes until you choose." : "No frame is clearly sharper than the rest."} P pick · X reject</span>
      <span class="spacer"></span>
      <button
        type="button"
        class="btn danger"
        disabled={keepVersionId === null}
        title="Keep the selected photo and reject the rest of this group"
        onclick={() => keepVersionId !== null && askRejectOthers(keepVersionId)}
      >Reject the others…</button>
    {:else}
      <span class="title">
        {similar.groupViews.length} group{similar.groupViews.length === 1 ? "" : "s"} · {photoCount} photo{photoCount === 1 ? "" : "s"}
      </span>
      {#if similar.analysing && similar.status}
        <span class="hint" role="status">Analysing {similar.status.analysed} / {similar.status.total}…</span>
      {/if}
      <span class="spacer"></span>
      <label class="slider">
        <span class="slider-end">stricter</span>
        <input
          type="range"
          min="0"
          max="100"
          value={Math.round(similar.similarity * 100)}
          aria-label="Similarity"
          aria-valuetext={similarityLabel(similar.similarity)}
          oninput={(e) => setSimilarity(Number(e.currentTarget.value) / 100)}
        />
        <span class="slider-end">looser</span>
      </label>
      <span class="hint now">{similarityLabel(similar.similarity)}</span>
    {/if}
  </div>

  {#if group}
    <LibrarySurveyView
      images={group.frames.map((f) => f.image)}
      primaryId={selection.selectedId}
      onSetPrimary={selectOnly}
      onDeselect={() => {}}
      onOpen={(vid) => openDevelop(vid)}
      onRatingChange={handleRatingChange}
      onFlagChange={handleFlagChange}
      onColorLabelChange={handleColorLabelChange}
      {culling}
      showDeselect={false}
    />
  {:else if !similar.loaded}
    <div class="empty"><p>Looking for similar photos…</p></div>
  {:else if similar.groupViews.length === 0}
    <div class="empty">
      <p>No similar photos at this setting.</p>
      <span class="hint">
        {#if similar.analysing && similar.status}
          Photos are still being analysed ({similar.status.analysed} / {similar.status.total}). Groups appear as they are measured.
        {:else}
          Frames count as similar when they come from one camera within a few seconds and look alike. Try the slider towards looser.
        {/if}
      </span>
    </div>
  {:else}
    <div class="groups">
      {#each similar.groupViews as g (g.key)}
        {@const lead = g.best ?? g.frames[0]}
        <button type="button" class="cell" aria-label="Group of {g.frames.length} similar photos" onclick={() => openSimilarGroup(g.key)}>
          {#if g.frames.length > 2}<span class="layer l2"></span>{/if}
          <span class="layer l1"></span>
          <span class="front">
            {#if thumb(lead.image)}
              <img src={thumb(lead.image)} alt="" draggable="false" />
            {/if}
          </span>
          {#if g.best}<span class="badge best">Suggested</span>{/if}
          <span class="badge count">{g.frames.length}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<ConfirmDialog
  open={similar.pendingReject !== null}
  title={`Reject ${similar.pendingReject?.versionIds.length ?? 0} other photo${similar.pendingReject?.versionIds.length === 1 ? "" : "s"}?`}
  message={rejectOthersMessage(similar.pendingReject?.versionIds.length ?? 0)}
  confirmLabel={`Reject ${similar.pendingReject?.versionIds.length ?? 0}`}
  onConfirm={confirmRejectOthers}
  onCancel={cancelRejectOthers}
/>

<style>
  .similar {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11.5px;
    color: var(--text-secondary);
  }
  .title {
    color: var(--text-primary);
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .hint {
    color: var(--text-tertiary);
    font-size: 11px;
  }
  .hint.now {
    min-width: 170px;
  }
  .btn {
    all: unset;
    cursor: pointer;
    padding: 4px 10px;
    border-radius: var(--radius-s);
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel-raised);
    color: var(--text-secondary);
    font-size: 11.5px;
  }
  .btn:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .btn:focus-visible {
    outline: 2px solid var(--accent-strong);
    outline-offset: 1px;
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .btn.danger {
    border-color: var(--label-red);
    color: var(--label-red);
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .slider input {
    width: 140px;
  }
  .slider-end {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .groups {
    flex: 1;
    padding: 14px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    grid-auto-rows: 150px;
    gap: 16px;
    overflow: auto;
    align-content: start;
  }
  .cell {
    all: unset;
    position: relative;
    cursor: pointer;
    border-radius: var(--radius-s);
  }
  .cell:focus-visible {
    outline: 2px solid var(--accent-strong);
    outline-offset: 2px;
  }
  .layer,
  .front {
    position: absolute;
    inset: 0;
    border-radius: var(--radius-s);
    overflow: hidden;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
  }
  .layer.l1 {
    transform: translate(4px, -4px);
    opacity: 0.8;
  }
  .layer.l2 {
    transform: translate(8px, -8px);
    opacity: 0.55;
  }
  .front {
    background: #000;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .front img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .badge {
    position: absolute;
    font-size: 9.5px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 9px;
    letter-spacing: 0.02em;
  }
  .badge.count {
    right: 6px;
    bottom: 6px;
    background: rgba(0, 0, 0, 0.7);
    color: var(--text-primary);
  }
  .badge.best {
    left: 6px;
    top: 6px;
    background: var(--accent);
    color: var(--accent-on, #1b1310);
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text-secondary);
    text-align: center;
    padding: 0 24px;
  }
</style>
