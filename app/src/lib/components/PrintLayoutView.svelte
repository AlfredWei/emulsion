<script>
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { getGradedDevelopPreview, getSoftProofPreview } from "$lib/api/develop.js";
  import { PAPER_SIZES } from "$lib/api/print.js";

  /**
   * @typedef {Object} PrintColorManagementView
   * @property {"srgb" | "adobe-rgb" | "prophoto-rgb" | "custom"} target
   * @property {string | null} customProfilePath
   * @property {"perceptual" | "relative" | "saturation" | "absolute"} intent
   */

  let {
    items = /** @type {{path: string, version_id: number}[]} */ ([]),
    template = /** @type {"single" | "contact-sheet"} */ ("single"),
    fitMode = /** @type {"fit" | "fill"} */ ("fit"),
    rows = 2,
    cols = 2,
    cellSpacing = 0.1,
    paperSize = "letter",
    orientation = /** @type {"portrait" | "landscape"} */ ("portrait"),
    margins = { top: 0.5, right: 0.5, bottom: 0.5, left: 0.5 },
    colorManagement = /** @type {PrintColorManagementView | null} */ (null),
    printReadyUrls = /** @type {Record<number, string>} */ ({}),
  } = $props();

  // Live WYSIWYG layout preview: reuses Develop's own graded-preview
  // machinery (already cached, 2048px is plenty for a page-layout
  // thumbnail) -- or, when a printer profile is chosen, the same
  // soft-proof preview DevelopPanel's own Soft Proof section already
  // fetches, so the on-screen layout roughly matches what printing will
  // produce. The exact full-resolution, ICC-transformed payload is only
  // generated once, right before printing (see +page.svelte's handlePrint),
  // supplied here as `printReadyUrls` and preferred over this live preview
  // when present.
  let previewUrls = $state(/** @type {Record<number, string>} */ ({}));

  $effect(() => {
    const list = items;
    const proof = colorManagement;
    let cancelled = false;

    (async () => {
      const next = /** @type {Record<number, string>} */ ({});
      for (const item of list) {
        try {
          const preview = proof
            ? await getSoftProofPreview(item.version_id, {
                target: proof.target,
                custom_profile_path: proof.customProfilePath,
                intent: proof.intent,
                gamut_warning: false,
              })
            : await getGradedDevelopPreview(item.version_id);
          if (cancelled) return;
          next[item.version_id] = convertFileSrc(preview.path);
        } catch {
          // Leave this item missing -- one bad/offline source shouldn't
          // block the rest of the layout from previewing.
        }
      }
      if (!cancelled) previewUrls = next;
    })();

    return () => {
      cancelled = true;
    };
  });

  let paper = $derived(PAPER_SIZES[/** @type {keyof typeof PAPER_SIZES} */ (paperSize)] ?? PAPER_SIZES.letter);
  let pageWidthIn = $derived(orientation === "landscape" ? paper.heightIn : paper.widthIn);
  let pageHeightIn = $derived(orientation === "landscape" ? paper.widthIn : paper.heightIn);

  // Dynamically-built @page/print CSS, injected via <svelte:head> below
  // rather than a CSS custom property -- @page doesn't reliably respond to
  // var() across engines (the accepted WKWebView-fidelity risk named in
  // the plan). The visibility:hidden/absolute-position trick hides
  // everything else in the app shell (module switch, panel, filmstrip)
  // during print without needing those siblings to know about Print mode.
  let pageCss = $derived(`
    @page {
      size: ${pageWidthIn}in ${pageHeightIn}in;
      margin: ${margins.top}in ${margins.right}in ${margins.bottom}in ${margins.left}in;
    }
    @media print {
      body * { visibility: hidden; }
      .print-page, .print-page * { visibility: visible; }
      .print-page {
        position: absolute;
        top: 0;
        left: 0;
        width: 100%;
        height: 100%;
        box-shadow: none !important;
      }
    }
  `);

  let contactCells = $derived.by(() => {
    if (template !== "contact-sheet") return [];
    const capacity = Math.max(1, rows) * Math.max(1, cols);
    return items.slice(0, capacity);
  });
</script>

<svelte:head>
  <style>{pageCss}</style>
</svelte:head>

<div class="layout-view">
  <div
    class="print-page"
    style="aspect-ratio: {pageWidthIn} / {pageHeightIn}; --margin-top: {margins.top}in; --margin-right: {margins.right}in; --margin-bottom: {margins.bottom}in; --margin-left: {margins.left}in;"
  >
    <div class="print-content">
      {#if template === "single"}
        {#if items[0]}
          <img
            class="single-image"
            style="object-fit: {fitMode === 'fill' ? 'cover' : 'contain'};"
            src={printReadyUrls[items[0].version_id] ?? previewUrls[items[0].version_id]}
            alt=""
          />
        {:else}
          <p class="empty-note">No photo selected.</p>
        {/if}
      {:else}
        <div
          class="contact-grid"
          style="grid-template-rows: repeat({Math.max(1, rows)}, 1fr); grid-template-columns: repeat({Math.max(
            1,
            cols,
          )}, 1fr); gap: {cellSpacing}in;"
        >
          {#each contactCells as item (item.version_id)}
            <img class="grid-cell" src={printReadyUrls[item.version_id] ?? previewUrls[item.version_id]} alt="" />
          {/each}
        </div>
      {/if}
    </div>
  </div>
  {#if template === "contact-sheet" && items.length > contactCells.length}
    <p class="overflow-note">
      Showing {contactCells.length} of {items.length} — increase the grid or reduce the selection to show them all.
    </p>
  {/if}
</div>

<style>
  .layout-view {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    overflow: auto;
    background: var(--bg-canvas);
  }
  .print-page {
    width: min(100%, 640px);
    background: #fff;
    box-shadow: var(--shadow-soft);
    display: flex;
    flex: none;
  }
  .print-content {
    flex: 1;
    display: flex;
    min-width: 0;
    padding: var(--margin-top) var(--margin-right) var(--margin-bottom) var(--margin-left);
  }
  .single-image {
    width: 100%;
    height: 100%;
  }
  .contact-grid {
    flex: 1;
    display: grid;
    min-width: 0;
  }
  .grid-cell {
    width: 100%;
    height: 100%;
    object-fit: contain;
    background: #eee;
  }
  .empty-note,
  .overflow-note {
    font-size: 11px;
    color: var(--text-tertiary);
  }
  .empty-note {
    margin: auto;
  }
</style>
