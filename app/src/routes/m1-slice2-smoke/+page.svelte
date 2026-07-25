<script>
  import { convertFileSrc, invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { listImages } from "$lib/api/catalog.js";

  // Throwaway diagnostic for M1 Slice 2's new, previously-unverified piece:
  // does Tauri's asset protocol (tauri.conf.json app.security.assetProtocol)
  // actually serve a real thumbnail file given its on-disk path, via
  // convertFileSrc()? This can't be checked visually (no tool available to
  // screenshot the native window), so it fetches the asset URL directly and
  // reports the real HTTP-level result, same pattern as /m0-spike.

  let status = $state("running...");
  let detail = $state("");

  async function run() {
    /** @type {Record<string, any>} */
    const report = { images: null, assetFetch: null, error: null };
    try {
      const images = await listImages();
      report.images = images.map((i) => ({
        version_id: i.version_id,
        path: i.path,
        thumbnail_path: i.thumbnail_path,
      }));

      const withThumb = images.find((i) => i.thumbnail_path);
      if (!withThumb) {
        report.assetFetch = "no cataloged image has a thumbnail_path yet -- import something first";
      } else {
        const assetUrl = convertFileSrc(/** @type {string} */ (withThumb.thumbnail_path));
        const res = await fetch(assetUrl);
        const blob = await res.blob();
        report.assetFetch = {
          assetUrl,
          status: res.status,
          contentType: res.headers.get("content-type"),
          byteLength: blob.size,
        };
      }
    } catch (/** @type {any} */ e) {
      report.error = String(e && e.stack ? e.stack : e);
    }

    status = report.error ? "FAILED" : "DONE";
    detail = JSON.stringify(report, null, 2);
    await invoke("report_spike_result", { resultJson: JSON.stringify(report) });
  }

  onMount(() => {
    run();
  });
</script>

<main style="font-family: ui-monospace, monospace; padding: 2rem; white-space: pre-wrap;">
  <h1>M1 Slice 2 smoke test: asset protocol</h1>
  <p>Status: {status}</p>
  <pre>{detail}</pre>
</main>
