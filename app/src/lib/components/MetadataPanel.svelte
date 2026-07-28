<script>
  import { onMount } from "svelte";
  import {
    assignKeywordPath,
    removeKeywordFromImage,
    getImageKeywords,
    listKeywords,
  } from "$lib/api/catalog.js";

  /**
   * `targetImageIds` is who a newly-typed keyword gets assigned to --
   * the whole current Library selection when there is one, matching the
   * batch rate/flag/color-label behavior. The chip list below always
   * reflects `image` alone (the anchor), matching the existing IPTC
   * caption/copyright/contact precedent -- only *assignment* batches.
   * @type {{
   *   image: import('$lib/api/catalog.js').ImageSummary | null,
   *   targetImageIds: number[],
   *   onCaptionChange: (caption: string) => void,
   *   onCopyrightChange: (copyright: string) => void,
   *   onContactChange: (contact: string) => void,
   *   onKeywordAssigned: (name: string, imageCount: number) => void,
   * }}
   */
  let { image, targetImageIds, onCaptionChange, onCopyrightChange, onContactChange, onKeywordAssigned } =
    $props();

  let cameraLine = $derived(
    image ? [image.camera_make, image.camera_model].filter(Boolean).join(" ") || "—" : "—",
  );
  let lensLine = $derived(image?.lens_model || "—");

  // "1/125 · f/8 · ISO 100" -- matches the reviewed mockup's combined
  // Exposure row. shutter_speed is stored in seconds; sub-1s values
  // display as a fraction (the common convention), 1s+ as "Ns".
  function formatShutter(/** @type {number} */ seconds) {
    if (seconds >= 1) return `${seconds}s`;
    const denominator = Math.round(1 / seconds);
    return `1/${denominator}`;
  }
  let exposureLine = $derived.by(() => {
    if (!image) return "—";
    const parts = [];
    if (image.shutter_speed) parts.push(formatShutter(image.shutter_speed));
    if (image.aperture) parts.push(`f/${image.aperture.toFixed(1)}`);
    if (image.iso) parts.push(`ISO ${image.iso}`);
    return parts.length > 0 ? parts.join(" · ") : "—";
  });

  // captured_at is stored as "YYYY-MM-DDTHH:MM:SS[+offset]" -- display as
  // "YYYY-MM-DD HH:MM", matching the mockup's format.
  let capturedLine = $derived(
    image?.captured_at ? image.captured_at.replace("T", " ").slice(0, 16) : "—",
  );

  // Keywording (M2 Slice 4). This panel owns its own async state, matching
  // ExportDialog's pattern -- +page.svelte only supplies `image`,
  // `targetImageIds`, and a status-message callback.
  let allKeywords = $state(/** @type {import('$lib/api/catalog.js').KeywordNode[]} */ ([]));
  let imageKeywords = $state(/** @type {import('$lib/api/catalog.js').KeywordRef[]} */ ([]));
  let keywordInput = $state("");
  let suggestionsOpen = $state(false);

  onMount(async () => {
    allKeywords = await listKeywords();
  });

  // Full "parent/child/leaf" display paths built client-side from the flat
  // parent_id-linked list, for the assignment input's suggestions.
  let keywordSuggestions = $derived.by(() => {
    const byId = new Map(allKeywords.map((k) => [k.id, k]));
    return allKeywords.map((node) => {
      const segments = [node.name];
      let current = node;
      while (current.parent_id !== null) {
        const parent = byId.get(current.parent_id);
        if (!parent) break;
        segments.unshift(parent.name);
        current = parent;
      }
      return segments.join("/");
    });
  });

  // A small manually-rendered dropdown rather than an HTML <datalist> --
  // WebKit's native datalist ignores page theming entirely (no CSS control
  // over it at all), which would render as a plain white system dropdown
  // against this panel's otherwise fully dark-themed inputs. This app has
  // no way to screenshot its native window to verify that empirically, so
  // rather than ship something unverifiable with a well-known theming
  // problem, this uses the same themed styling as everything else here.
  let filteredSuggestions = $derived.by(() => {
    const query = keywordInput.trim().toLowerCase();
    if (!query) return [];
    return keywordSuggestions.filter((path) => path.toLowerCase().includes(query)).slice(0, 8);
  });

  function pickSuggestion(/** @type {string} */ path) {
    keywordInput = path;
    suggestionsOpen = false;
    handleAssignKeyword();
  }

  // Race-guarded fetch on image change: rapid Library arrow-key/click
  // navigation can have several of these in flight at once -- this
  // codebase's one other prop-triggered fetch (DevelopCanvas's image
  // loader) has no such guard, but that's a much rarer, more deliberate
  // action than clicking through Library cells, so a late response here
  // could plausibly overwrite a newer selection's chips with a stale
  // image's. Guarded by re-checking the target id still matches when the
  // response lands, not assigning otherwise.
  $effect(() => {
    const targetId = image?.image_id ?? null;
    if (targetId === null) {
      imageKeywords = [];
      return;
    }
    getImageKeywords(targetId).then((keywords) => {
      if (image?.image_id === targetId) imageKeywords = keywords;
    });
  });

  async function handleAssignKeyword() {
    const segments = keywordInput
      .split("/")
      .map((s) => s.trim())
      .filter(Boolean);
    if (segments.length === 0 || targetImageIds.length === 0) return;
    await assignKeywordPath(targetImageIds, segments);
    keywordInput = "";
    suggestionsOpen = false;
    allKeywords = await listKeywords();
    onKeywordAssigned(segments[segments.length - 1], targetImageIds.length);
    if (image) imageKeywords = await getImageKeywords(image.image_id);
  }

  async function handleRemoveKeyword(/** @type {number} */ keywordId) {
    if (!image) return;
    await removeKeywordFromImage(image.image_id, keywordId);
    imageKeywords = await getImageKeywords(image.image_id);
  }
</script>

<div class="panel">
  {#if !image}
    <p class="empty">Select a photo to see its details.</p>
  {:else}
    <div class="section-label">Metadata</div>
    <div class="row"><span class="row-label">Camera</span><span>{cameraLine}</span></div>
    <div class="row"><span class="row-label">Lens</span><span>{lensLine}</span></div>
    <div class="row"><span class="row-label">Exposure</span><span class="mono">{exposureLine}</span></div>
    <div class="row"><span class="row-label">Captured</span><span class="mono">{capturedLine}</span></div>

    <div class="section-label">Keywords</div>
    <div class="keyword-chips">
      {#each imageKeywords as keyword (keyword.id)}
        <span class="chip" title={keyword.path}>
          {keyword.name}
          <button
            type="button"
            class="chip-remove"
            aria-label="Remove keyword {keyword.name}"
            onclick={() => handleRemoveKeyword(keyword.id)}
          >×</button>
        </span>
      {:else}
        <span class="empty-hint">No keywords yet</span>
      {/each}
    </div>
    <div class="field keyword-input-field">
      <input
        id="md-keyword-input"
        type="text"
        placeholder="Add a keyword (nature/birds/owl)…"
        aria-label="Add a keyword"
        bind:value={keywordInput}
        onfocus={() => (suggestionsOpen = true)}
        onblur={() => (suggestionsOpen = false)}
        oninput={() => (suggestionsOpen = true)}
        onkeydown={(e) => e.key === "Enter" && (e.preventDefault(), handleAssignKeyword())}
      />
      {#if suggestionsOpen && filteredSuggestions.length > 0}
        <ul class="suggestions">
          {#each filteredSuggestions as path (path)}
            <li>
              <button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => pickSuggestion(path)}>
                {path}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="section-label">Info</div>
    <div class="field">
      <label for="md-caption">Caption</label>
      <textarea
        id="md-caption"
        rows="2"
        value={image.caption ?? ""}
        onblur={(e) => onCaptionChange(e.currentTarget.value)}
      ></textarea>
    </div>
    <div class="field">
      <label for="md-copyright">Copyright</label>
      <input
        id="md-copyright"
        type="text"
        value={image.copyright ?? ""}
        onblur={(e) => onCopyrightChange(e.currentTarget.value)}
      />
    </div>
    <div class="field">
      <label for="md-contact">Contact</label>
      <input
        id="md-contact"
        type="text"
        value={image.contact ?? ""}
        onblur={(e) => onContactChange(e.currentTarget.value)}
      />
    </div>
  {/if}
</div>

<style>
  .panel {
    width: 240px;
    flex: none;
    background: var(--bg-panel);
    border-left: 1px solid var(--border-subtle);
    overflow-y: auto;
    overflow-x: hidden;
    padding: 14px 12px;
  }
  .empty {
    color: var(--text-tertiary);
    font-size: 12px;
    padding: 4px;
  }
  .section-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-tertiary);
    padding: 10px 4px 6px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 4px;
    font-size: 12px;
  }
  .row-label {
    width: 62px;
    flex: none;
    color: var(--text-secondary);
  }
  .row span {
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mono {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 5px 4px;
  }
  .field label {
    font-size: 10.5px;
    color: var(--text-tertiary);
  }
  .field input,
  .field textarea {
    box-sizing: border-box;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    font-family: inherit;
    color: var(--text-primary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    resize: vertical;
  }
  .field input:focus,
  .field textarea:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .keyword-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 2px 4px 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px 3px 9px;
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
  }
  .chip-remove {
    all: unset;
    cursor: pointer;
    line-height: 1;
    padding: 0 2px;
    color: var(--text-tertiary);
  }
  .chip-remove:hover {
    color: var(--label-red);
  }
  .empty-hint {
    font-size: 11px;
    color: var(--text-tertiary);
  }
  .keyword-input-field {
    position: relative;
  }
  .suggestions {
    all: unset;
    position: absolute;
    top: calc(100% + 2px);
    left: 4px;
    right: 4px;
    z-index: 10;
    display: block;
    max-height: 160px;
    overflow-y: auto;
    background: var(--bg-panel-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-s);
    box-shadow: var(--shadow-soft);
    padding: 4px;
  }
  .suggestions li {
    list-style: none;
  }
  .suggestions button {
    all: unset;
    box-sizing: border-box;
    display: block;
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    color: var(--text-primary);
    border-radius: var(--radius-s);
    cursor: pointer;
  }
  .suggestions button:hover {
    background: var(--accent-soft);
    color: var(--accent-strong);
  }
</style>
