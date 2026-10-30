import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PORTRAIT = path.resolve(__dirname, "../../../test_image/Smiling-woman-pink-shirt-portrait.jpg");
const MODELS_DIR = process.env.SEGMENT_MODELS_DIR ?? "";
const SKY_PHOTOS = ["Field-corn-Liechtenstein-landscape.jpg", "Cavenagh-Bridge-Singapore-panorama-frame-a.jpg", "Szechenyi-Chain-Bridge-Budapest-night.jpg"].map((f) => path.resolve(__dirname, "../../../test_image", f));
const OUT_DIR = process.env.ADAPTIVE_PERSON_OUT ?? "";

/**
 * RFC-0033 section 3.1, the `person` region with the REAL click-select model: the stored face boxes become a click on
 * the face and one on the torso, SAM 2 cuts each person, and the cuts are merged into one 256 x 256 field. LOCAL ONLY:
 * it needs SEGMENT_MODELS_DIR (the folder holding sam2_hiera_tiny.encoder.onnx and .decoder.onnx) and the face models.
 * It prints what it measured and, with ADAPTIVE_PERSON_OUT set, writes the field as a PNG to look at; the assertions are
 * loose (the model's exact mask is not under test, the whole path is).
 */
describe("Adaptive person region, real model", function () {
  this.timeout(300000);
  const state = /** @type {Record<string, any>} */ ({});
  const invoke = (/** @type {string} */ cmd, /** @type {any} */ args) => browser.execute((c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);

  before(async function () {
    if (!MODELS_DIR) this.skip();
    await browser.setTimeout({ script: 280000 });
    await invoke("import_files", { paths: [PORTRAIT, ...SKY_PHOTOS] });
    const images = await invoke("list_images", {});
    state.img = images.find((/** @type {any} */ i) => i.path === PORTRAIT);
    expect(state.img).toBeTruthy();
    await invoke("detect_faces_for_images", { imageIds: [state.img.image_id] });
  });

  it("the sky outlined by the model, on three photographs (the colour rule alone: cargo test adaptive_eval)", async function () {
    const images = await invoke("list_images", {});
    const sky = async (/** @type {any} */ img) => invoke("resolve_adaptive_region", { imageId: img.image_id, path: img.path, contentHash: img.content_hash, region: "sky" });
    const photos = SKY_PHOTOS.map((p) => images.find((/** @type {any} */ i) => i.path === p));
    // A fresh profile already holds the bundled model (seeded on first launch); importing it again checks the offline path.
    const status = await invoke("segment_import_model_files", {
      paths: [path.join(MODELS_DIR, "sam2_hiera_tiny.encoder.onnx"), path.join(MODELS_DIR, "sam2_hiera_tiny.decoder.onnx")],
    });
    expect(status.state).toBe("ready");
    for (let i = 0; i < photos.length; i++) {
      const started = Date.now();
      const after = await sky(photos[i]);
      const name = path.basename(photos[i].path, ".jpg");
      console.log(`SKY_E2E ${name} model=${after.outcome}/${after.coverage?.toFixed(3)} ms=${Date.now() - started} reason=${after.reason}`);
      if (OUT_DIR) {
        fs.mkdirSync(OUT_DIR, { recursive: true });
        if (after.mask) fs.writeFileSync(path.join(OUT_DIR, `${name}-sky-model.png`), Buffer.from(after.mask.logits, "base64"));
      }
    }
  });

  it("finds the person of a portrait from the face and the torso click", async function () {
    const started = Date.now();
    const r = await invoke("resolve_adaptive_region", { imageId: state.img.image_id, path: PORTRAIT, contentHash: state.img.content_hash, region: "person" });
    const ms = Date.now() - started;
    console.log(`PERSON_E2E outcome=${r.outcome} coverage=${r.coverage?.toFixed(3)} reason=${r.reason} ms=${ms}`);
    expect(["found", "check"]).toContain(r.outcome);
    expect(r.coverage).toBeGreaterThan(0.05);
    expect(r.coverage).toBeLessThan(0.9);
    expect(r.mask.op).toBe("segment_mask");
    if (OUT_DIR) {
      fs.mkdirSync(OUT_DIR, { recursive: true });
      fs.writeFileSync(path.join(OUT_DIR, "portrait-person-field.png"), Buffer.from(r.mask.logits, "base64"));
    }
    // A second call reuses the prepared embedding or prepares again; either way it must agree with the first.
    const again = await invoke("resolve_adaptive_region", { imageId: state.img.image_id, path: PORTRAIT, contentHash: state.img.content_hash, region: "person" });
    expect(again.outcome).toBe(r.outcome);
    expect(Math.abs(again.coverage - r.coverage)).toBeLessThan(0.01);
  });
});
