<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  // Throwaway manual verification for M2 Slice 1 (multi-file import +
  // JPEG support) -- same pattern as prior slices. Can't click the native
  // OS file-picker dialog, so this exercises the real backend commands
  // (get_supported_extensions, import_files) directly with an explicit
  // path list, same as a real multi-select would produce.

  async function run() {
    /** @type {Record<string, any>} */
    const report = { error: null };
    try {
      const params = new URLSearchParams(window.location.search);
      const rawPath = params.get("raw");
      const jpegPath = params.get("jpeg");
      if (!rawPath || !jpegPath) throw new Error("missing ?raw=&jpeg= query params");

      const extensions = await invoke("get_supported_extensions");
      report.supportedExtensions = extensions;
      report.includesRaw = extensions.includes("dng");
      report.includesJpeg = extensions.includes("jpg") && extensions.includes("jpeg");

      const summary = await invoke("import_files", { paths: [rawPath, jpegPath] });
      report.importSummary = summary;

      const images = await invoke("list_images");
      report.catalogedCount = images.length;
      const jpegImage = images.find((/** @type {any} */ i) => i.path === jpegPath);
      const rawImage = images.find((/** @type {any} */ i) => i.path === rawPath);
      report.jpegCataloged = !!jpegImage;
      report.rawCataloged = !!rawImage;
      report.jpegThumbnailInitiallyNull = jpegImage ? jpegImage.thumbnail_path === null : null;

      if (jpegImage) {
        const preview = await invoke("get_develop_preview", { path: jpegImage.path });
        report.jpegDevelopPreview = preview;
      }

      // The background thumbnail pass is fire-and-forget after import_files
      // returns -- give it a moment, then check whether it landed.
      await new Promise((r) => setTimeout(r, 1500));
      const imagesAfterWait = await invoke("list_images");
      const jpegAfterWait = imagesAfterWait.find((/** @type {any} */ i) => i.path === jpegPath);
      report.jpegThumbnailAfterBackgroundPass = jpegAfterWait ? jpegAfterWait.thumbnail_path : null;
    } catch (/** @type {any} */ e) {
      report.error = String(e && e.stack ? e.stack : e);
    }

    await invoke("report_spike_result", { resultJson: JSON.stringify(report) });
  }

  onMount(() => {
    run();
  });
</script>

<main style="font-family: ui-monospace, monospace; padding: 2rem;">
  <h1>M2 Slice 1 smoke test</h1>
</main>
