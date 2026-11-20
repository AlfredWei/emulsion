import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURES = process.env.LANE_FIXTURES ?? path.resolve(__dirname, "../../../test_image");
const OUT = process.env.LANE_OUT ?? fs.mkdtempSync(path.join(os.tmpdir(), "perf-probe-"));
const IMAGE = /\.(jpe?g|png|tiff?|cr2|dng)$/i;

const invoke = (cmd, args) =>
  browser.execute(async (c, a) => window.__TAURI__.core.invoke(c, a), cmd, args);

describe("perf probe", () => {
  const perf = {};
  let files;
  let exportDir;

  before(async () => {
    await browser.setTimeout({ script: 180000 });
    fs.mkdirSync(OUT, { recursive: true });
    files = fs.readdirSync(FIXTURES).filter((f) => IMAGE.test(f)).sort().map((f) => path.join(FIXTURES, f));
    exportDir = fs.mkdtempSync(path.join(os.tmpdir(), "perf-probe-export-"));
    perf.files = files.length;
  });

  after(() => fs.writeFileSync(path.join(OUT, "perf.json"), JSON.stringify(perf, null, 2)));

  it("imports the fixtures and times it", async () => {
    const t0 = Date.now();
    const summary = await invoke("import_files", { paths: files });
    perf.import_ms = Date.now() - t0;
    perf.import_imported = summary.imported;
    perf.import_failed = summary.failed;
    expect(summary.failed).toBe(0);
    expect(summary.imported).toBe(files.length);
  });

  it("times a reload up to the first grid cell", async () => {
    await browser.refresh();
    // WebDriver cannot see the app's own launch, so the metric is a reload of a populated catalog. The page's own clock starts at
    // navigation, which excludes the service's delay on the first command after a reload. The poll can start after the cell
    // exists, so the number is an upper bound with 10 ms resolution.
    const result = await browser.execute(async () => {
      const deadline = performance.now() + 60000;
      while (!document.querySelector(".cell") && performance.now() < deadline) await new Promise((r) => setTimeout(r, 10));
      return { cell: !!document.querySelector(".cell"), at: performance.now() };
    });
    expect(result.cell).toBe(true);
    perf.first_grid_ms = Math.round(result.at);
  });

  it("exports one JPEG and times it", async () => {
    const images = await invoke("list_images");
    const jpeg = images.find((i) => /\.jpe?g$/i.test(i.path));
    expect(jpeg).toBeTruthy();
    const t0 = Date.now();
    const [result] = await browser.execute(
      async (p, id, dir) => window.__TAURI__.core.invoke("export_images", { items: [{ path: p, version_id: id }], options: { destination_dir: dir, long_edge: null, quality: 90 } }),
      jpeg.path,
      jpeg.version_id,
      exportDir,
    );
    perf.export_ms = Date.now() - t0;
    expect(result.error).toBeFalsy();
    expect(fs.readdirSync(exportDir).length).toBe(1);
  });
});
