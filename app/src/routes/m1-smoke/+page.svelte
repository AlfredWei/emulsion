<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  // Throwaway manual smoke test for M1 Slice 1 (catalog v1 + import
  // backend) — not part of the app, same pattern as /m0-spike. Reports
  // results to the Rust process's stdout since there's no tool available
  // to screenshot the native window in this environment.

  let status = $state("running...");
  let detail = $state("");

  async function run() {
    const report = { importFolderResult: null, listImagesResult: null, error: null };
    try {
      // Path is injected by the temporary smoke-test harness at build time
      // via a query param, since this page has no UI of its own.
      const params = new URLSearchParams(window.location.search);
      const dir = params.get("dir");
      if (!dir) throw new Error("missing ?dir= query param");

      report.importFolderResult = await invoke("import_folder", { path: dir });
      report.listImagesResult = await invoke("list_images");
    } catch (e) {
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
  <h1>M1 Slice 1 smoke test</h1>
  <p>Status: {status}</p>
  <pre>{detail}</pre>
</main>
