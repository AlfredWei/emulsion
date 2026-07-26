<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  // Throwaway manual verification for M1 Slice 6 (crash-safety hardening)
  // -- same pattern as /m0-spike, /m1-smoke, /m1-slice*-smoke. Exercises
  // the *real* debounce/flush/close-hook mechanism from +page.svelte
  // (copied here rather than imported, matching m1-slice3-smoke's own
  // precedent of testing "the exact logic that ships" via a self-
  // contained diagnostic page): schedule a debounced edit-stack write,
  // then close the window *before* the debounce timer would have fired,
  // and confirm the flush-on-close path saves it anyway. The real proof
  // is inspecting the catalog file directly (via the sqlite3 CLI) after
  // this process has actually exited -- report_spike_result fires early,
  // before the close, since nothing can report *after* the process dies.

  const TEST_VALUE = 0.42; // distinctive, unlikely to appear by coincidence

  async function run() {
    /** @type {Record<string, any>} */
    const report = { error: null };
    try {
      const images = await invoke("list_images");
      if (images.length === 0) throw new Error("no cataloged images to test against");
      const image = images[0];
      report.versionId = image.version_id;

      const stack = await invoke("get_edit_stack", { versionId: image.version_id });
      const ops = stack.ops.filter((/** @type {any} */ o) => o.op !== "exposure");
      ops.push({ op: "exposure", value: TEST_VALUE });
      const newStack = { ...stack, ops };

      // Schedule a debounced write, same 250ms/persistTimer shape as
      // +page.svelte's handleAdjustmentChange -- deliberately NOT awaited,
      // so it's still pending when close() is called just below.
      /** @type {ReturnType<typeof setTimeout> | null} */
      let persistTimer = null;
      /** @type {Promise<void> | null} */
      let pendingSave = null;
      function flushEditStack() {
        if (persistTimer !== null) {
          clearTimeout(persistTimer);
          persistTimer = null;
          pendingSave = invoke("set_edit_stack", { versionId: image.version_id, stack: newStack }).finally(() => {
            pendingSave = null;
          });
        }
        return pendingSave ?? Promise.resolve();
      }
      persistTimer = setTimeout(flushEditStack, 250);

      let unlisten;
      unlisten = await getCurrentWindow().onCloseRequested(async (event) => {
        if (persistTimer === null && pendingSave === null) return;
        event.preventDefault();
        report.closeInterceptedWithPendingWrite = true;
        await flushEditStack();
        report.flushCompletedBeforeDestroy = true;
        await getCurrentWindow().destroy();
      });

      report.scheduledValue = TEST_VALUE;
      report.note = "closing window now, ~10ms after scheduling a 250ms-debounced write -- if the fix works, flush-on-close saves it anyway";
      await invoke("report_spike_result", { resultJson: JSON.stringify(report) });

      await new Promise((r) => setTimeout(r, 10));
      unlisten(); // not needed once we're about to close ourselves
      await getCurrentWindow().close();
      return; // unreachable if close() actually closes the window
    } catch (/** @type {any} */ e) {
      report.error = String(e && e.stack ? e.stack : e);
      await invoke("report_spike_result", { resultJson: JSON.stringify(report) });
    }
  }

  onMount(() => {
    run();
  });
</script>

<main style="font-family: ui-monospace, monospace; padding: 2rem; white-space: pre-wrap;">
  <h1>M1 Slice 6 smoke test: close-flush</h1>
  <p>Triggers a close ~10ms after scheduling a debounced write. Check report_spike_result output, then inspect catalog.sqlite after this process exits.</p>
</main>
