import fs from "node:fs";
import path from "node:path";

const DIR = process.env.ADAPTIVE_EVAL_DIR ?? "";
const REGION = process.env.ADAPTIVE_EVAL_REGION ?? "person";
const OUT = process.env.ADAPTIVE_EVAL_OUT ?? "";
const LIMIT = Number(process.env.ADAPTIVE_EVAL_LIMIT ?? "1000");

/**
 * An evaluation harness, not a regression test (LOCAL ONLY). Imports every JPEG of ADAPTIVE_EVAL_DIR into the throwaway
 * profile, resolves ADAPTIVE_EVAL_REGION ("person", "face" or "sky") on each with the REAL resolvers and click-select
 * model (the bundled one is seeded on first launch), and writes `<name>.png` (the stored field) and one `results.json`
 * line per photograph to ADAPTIVE_EVAL_OUT, to be looked at on contact sheets. It asserts nothing about quality.
 */
describe("Adaptive region evaluation (local)", function () {
  this.timeout(3600000);
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);

  it(`resolves ${REGION} on every photograph of the set`, async function () {
    if (!DIR || !OUT) this.skip();
    await browser.setTimeout({ script: 600000 });
    const files = fs.readdirSync(DIR).filter((f) => /\.jpe?g$/i.test(f)).sort().slice(0, LIMIT).map((f) => path.join(DIR, f));
    const t0 = Date.now();
    const note = (/** @type {string} */ m) => fs.appendFileSync(path.join(OUT, "progress.log"), `${((Date.now() - t0) / 1000).toFixed(0)}s ${m}\n`);
    fs.mkdirSync(OUT, { recursive: true });
    note(`importing ${files.length}`);
    await invoke("import_files", { paths: files });
    note("imported");
    const images = await invoke("list_images", {});
    const imgs = files.map((f) => images.find((/** @type {any} */ i) => i.path === f)).filter(Boolean);
    if (REGION === "person" || REGION === "face") {
      // One photo per call: one WebDriver request must stay well under its ~300 s headers timeout.
      for (let i = 0; i < imgs.length; i += 1) {
        await invoke("detect_faces_for_images", { imageIds: imgs.slice(i, i + 1).map((/** @type {any} */ x) => x.image_id) });
        note(`faces ${Math.min(i + 1, imgs.length)}/${imgs.length}`);
      }
    }
    note("faces detected");
    const lines = [];
    for (const img of imgs) {
      const started = Date.now();
      let r;
      try {
        r = await invoke("resolve_adaptive_region", { imageId: img.image_id, path: img.path, contentHash: img.content_hash, region: REGION });
      } catch (e) {
        r = { outcome: "error", coverage: 0, reason: String(e), mask: null };
      }
      const name = path.basename(img.path).replace(/\.jpe?g$/i, "");
      if (r.mask) fs.writeFileSync(path.join(OUT, `${name}.png`), Buffer.from(r.mask.logits, "base64"));
      const line = { name, outcome: r.outcome, coverage: Number((r.coverage ?? 0).toFixed(3)), reason: r.reason, ms: Date.now() - started };
      lines.push(line);
      note(JSON.stringify(line));
    }
    fs.writeFileSync(path.join(OUT, "results.json"), lines.map((l) => JSON.stringify(l)).join("\n"));
    console.log(`EVAL_DONE ${REGION} ${lines.length} photographs`);
  });
});
