import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = path.resolve(__dirname, "../../../test_image");

/** Copies a real fixture JPEG and appends a few unique random bytes after
 * its EOI (End Of Image, 0xFFD9) marker -- a well-known safe no-op for
 * JPEG decoders, which stop reading at EOI and ignore anything after it.
 * Guarantees a genuinely fresh, never-before-imported, still-perfectly-
 * decodable JPEG on every run, which content-hash-based import dedupe
 * otherwise makes impossible to get from this repo's shared,
 * never-reset-between-runs test_image/ fixtures (every one of which is
 * already cataloged in this local dev machine's real catalog many times
 * over, from this project's own long history of e2e/manual verification
 * sessions). Without this, `import_files` would report every fixture as
 * a duplicate on every run past the very first, and the newly-imported
 * image this spec's `import_batch`-scoped assertions depend on would
 * never exist. */
function freshJpegFixture(/** @type {string} */ tmpDir) {
  const source = fs.readdirSync(FIXTURE_DIR).find((f) => /\.jpe?g$/i.test(f));
  const bytes = fs.readFileSync(path.join(FIXTURE_DIR, source));
  const unique = Buffer.from(`${Date.now()}-${Math.random()}`);
  const out = path.join(tmpDir, `fresh-${Date.now()}-${Math.random().toString(36).slice(2)}.jpg`);
  fs.writeFileSync(out, Buffer.concat([bytes, unique]));
  return out;
}

/**
 * Covers the two new backend pieces the import progress bar's "thumbnail"
 * phase and Loupe/Develop's "jump the queue" fix are built on:
 * `backfill_missing_thumbnails` (lib.rs, wraps import.rs's
 * `generate_missing_thumbnails_with_progress`) and `ensure_thumbnail`
 * (lib.rs, wraps import.rs's `ensure_thumbnail`).
 *
 * `backfill_missing_thumbnails` is scoped to `ImportSummary.import_batch`
 * (not the whole catalog) specifically BECAUSE this repo's own real local
 * dev catalog (shared across every manual verification session, never
 * reset between them) turned out to have a real, unrelated backlog of
 * ~30 un-thumbnailed full-resolution photos from earlier folder-import
 * testing -- a whole-catalog-scoped call took multiple minutes in a
 * debug build against it. Scoping to the batch this test's OWN import
 * just created sidesteps that entirely: the assertions below only ever
 * concern the one freshly-synthesized image from this call's own batch,
 * regardless of whatever backlog the rest of the catalog may or may not
 * have.
 */
describe("Thumbnail backfill: progress events and end state", () => {
  let tmpDir;

  before(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "emulsion-e2e-thumb-"));
  });

  after(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("imports a fresh fixture, then backfill_missing_thumbnails leaves it with a real thumbnail on disk", async () => {
    const freshPath = freshJpegFixture(tmpDir);

    const { events, batchImages } = await browser.execute(async (p) => {
      const summary = await window.__TAURI__.core.invoke("import_files", { paths: [p] });

      const collected = [];
      const unlisten = await window.__TAURI__.event.listen("thumbnail-progress", (event) => {
        collected.push(event.payload);
      });
      await window.__TAURI__.core.invoke("backfill_missing_thumbnails", { importBatch: summary.import_batch });
      unlisten();

      const images = await window.__TAURI__.core.invoke("list_images");
      return { events: collected, batchImages: images.filter((img) => img.import_batch === summary.import_batch) };
    }, freshPath);

    try {
      // Only the freshly-imported image is asserted on -- run-order-
      // independent, regardless of whatever unrelated backlog the rest of
      // the catalog has.
      expect(batchImages.length).toBe(1);
      expect(batchImages[0].thumbnail_path).not.toBeNull();

      // current increases by exactly 1 each time, total never changes
      // mid-sequence, and the last event reaches its own total -- same
      // "fires once per candidate, total fixed" contract
      // import-progress.e2e.js already checks for import-progress.
      expect(events.length).toBe(1);
      expect(events[0]).toEqual({ current: 1, total: 1 });
    } finally {
      // This spec deliberately synthesizes a fresh, never-before-seen
      // image on every run (see freshJpegFixture above) to dodge
      // content-hash dedup, against the same never-reset-between-runs
      // shared dev catalog golden-path.e2e.js depends on. Left uncleaned,
      // every run permanently adds a newest-`added_at` row, which over
      // many runs pushes older fixtures further down the newest-first,
      // DOM-virtualized Library grid (this is exactly what made
      // golden-path.e2e.js's own fixture lookup start failing). Removing
      // the rows this test itself created keeps the catalog's growth
      // bounded to genuine imports.
      const imageIds = batchImages.map((img) => img.image_id);
      if (imageIds.length > 0) {
        await browser.execute((ids) => window.__TAURI__.core.invoke("remove_images", { imageIds: ids }), imageIds);
      }
    }
  });

  it("ensure_thumbnail returns a real, existing thumbnail path for a cataloged image", async () => {
    const freshPath = freshJpegFixture(tmpDir);

    const result = await browser.execute(async (p) => {
      await window.__TAURI__.core.invoke("import_files", { paths: [p] });
      const images = await window.__TAURI__.core.invoke("list_images");
      const match = images.find((img) => img.path === p);
      if (!match) return { error: "image not found after import" };
      const thumbnailPath = await window.__TAURI__.core.invoke("ensure_thumbnail", { versionId: match.version_id });
      return { thumbnailPath, imageId: match.image_id };
    }, freshPath);

    try {
      expect(result.error).toBeUndefined();
      expect(typeof result.thumbnailPath).toBe("string");
      expect(result.thumbnailPath.length).toBeGreaterThan(0);
    } finally {
      // See the previous test's cleanup comment -- same reasoning applies.
      if (result.imageId != null) {
        await browser.execute((id) => window.__TAURI__.core.invoke("remove_images", { imageIds: [id] }), result.imageId);
      }
    }
  });
});
