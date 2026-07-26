<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  // Throwaway manual verification for M1 Slice 6: reports "STARTED" as
  // early as possible so an external script can SIGKILL this process
  // mid-import, then (separately, after relaunch) verify the catalog
  // survived intact. This process is expected to die mid-run -- there is
  // no "DONE" report for the killed run by design.

  async function run() {
    const params = new URLSearchParams(window.location.search);
    const dir = params.get("dir");
    if (!dir) {
      await invoke("report_spike_result", { resultJson: JSON.stringify({ error: "missing ?dir=" }) });
      return;
    }

    // Fire-and-forget so the STARTED report isn't delayed by import itself.
    invoke("report_spike_result", { resultJson: JSON.stringify({ started: true, dir }) });

    const summary = await invoke("import_folder", { path: dir });
    await invoke("report_spike_result", { resultJson: JSON.stringify({ done: true, summary }) });
  }

  onMount(() => {
    run();
  });
</script>

<main style="font-family: ui-monospace, monospace; padding: 2rem;">
  <h1>M1 Slice 6 import-kill smoke test</h1>
</main>
