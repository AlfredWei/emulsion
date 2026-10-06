/**
 * RFC-0026 slice 2a: the click-select commands and the AI helper binary are registered and behave
 * correctly **without the model files** (the CI-safe path; the 155 MB weights
 * are never downloaded here). The real-model checks are the Rust `--ignored`
 * test (`segment_matches_the_python_reference_...`).
 */
describe("Click-select model commands", () => {
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args = {}) =>
    browser.execute(
      async (c, a) => {
        try {
          return { ok: await window.__TAURI__.core.invoke(c, a) };
        } catch (e) {
          return { err: String(e) };
        }
      },
      cmd,
      args,
    );

  it("finds the AI helper binary next to the app executable, and does not start it", async () => {
    const r = await invoke("ai_helper_info");
    expect(r.err).toBeUndefined();
    expect(r.ok.exists).toBe(true);
    expect(r.ok.path).toMatch(/emulsion-ai(\.exe)?$/);
    expect(r.ok.running).toBe(false);
  });

  it("reports the model status with the download size and host", async () => {
    const r = await invoke("segment_model_status");
    expect(r.err).toBeUndefined();
    expect(["ready", "missing"]).toContain(r.ok.state);
    expect(r.ok.host).toBe("huggingface.co");
    if (r.ok.state === "missing") expect(r.ok.download_bytes).toBeGreaterThan(100_000_000);
  });

  it("rejects a file that is not one of the two model files, without touching the cache", async () => {
    const before = await invoke("segment_model_status");
    const r = await invoke("segment_import_model_files", { paths: ["/definitely/not/a/model.onnx"] });
    expect(r.err).toContain("not one of the expected model files");
    expect(await invoke("segment_model_status")).toEqual(before);
  });

  it("segment_prepare and segment_decode fail clearly while the model is missing", async function () {
    const status = await invoke("segment_model_status");
    if (status.ok.state !== "missing") this.skip(); // a developer machine that has the weights installed
    const prepare = await invoke("segment_prepare", { path: "/nope.jpg", contentHash: "abc" });
    expect(prepare.err).toContain("not installed");
    const decode = await invoke("segment_decode", { contentHash: "abc", prompts: [{ x: 0.5, y: 0.5, positive: true }], refine: null });
    expect(decode.err).toContain("not installed");
  });
});
