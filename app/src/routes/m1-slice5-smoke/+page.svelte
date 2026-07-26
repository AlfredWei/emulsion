<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  // Throwaway manual verification for M1 Slice 5 (Export pipeline) --
  // same pattern as /m0-spike, /m1-smoke, /m1-slice2-smoke, /m1-slice3-smoke.
  // Exports a real cataloged image to a real destination folder (injected
  // via ?dest=) and confirms the result on disk, not just "the command
  // returned ok".

  let status = $state("running...");
  let detail = $state("");

  async function run() {
    /** @type {Record<string, any>} */
    const report = { error: null };
    try {
      const params = new URLSearchParams(window.location.search);
      const dest = params.get("dest");
      if (!dest) throw new Error("missing ?dest= query param");

      const images = await invoke("list_images");
      if (images.length === 0) throw new Error("no cataloged images to export");
      const image = images[0];
      report.sourceImage = { path: image.path, version_id: image.version_id };

      const results = await invoke("export_images", {
        items: [{ path: image.path, version_id: image.version_id }],
        options: { destination_dir: dest, long_edge: 800, quality: 85 },
      });
      report.exportResults = results;
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
  <h1>M1 Slice 5 smoke test</h1>
  <p>Status: {status}</p>
  <pre>{detail}</pre>
</main>
