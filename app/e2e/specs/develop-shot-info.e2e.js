import path from "node:path";
import { fileURLToPath } from "node:url";
import { openDevelopFor } from "../helpers.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// A 320x200 JPEG written with known EXIF (ISO 200, 50 mm, f/8, 1/250 s) -- none of the other fixtures carries any.
const FIXTURE_PATH = path.resolve(__dirname, "../../../test_image/exif-shot-info.jpg");
const FIXTURE_NAME = "exif-shot-info";

/**
 * The Develop histogram's caption row (Histogram.svelte `shot-info`, fed by DevelopModule from the catalog row):
 * ISO, focal length, aperture and shutter of the open photo, in that order.
 */
describe("Develop histogram shows the capture settings", () => {
  before(async () => {
    await browser.setTimeout({ script: 90000 });
    await browser.execute(
      async (fixturePath) => window.__TAURI__.core.invoke("import_files", { paths: [fixturePath] }),
      FIXTURE_PATH,
    );
    await browser.refresh();
    await openDevelopFor(FIXTURE_NAME);
    await (await $("#exposure")).waitForExist({ timeout: 20000 });
  });

  it("lists ISO, focal length, aperture and shutter under the histogram", async () => {
    const deadline = Date.now() + 15000;
    let spans = /** @type {string[]} */ ([]);
    while (Date.now() < deadline) {
      spans = await browser.execute(() =>
        Array.from(document.querySelectorAll('[data-testid="histogram-shot-info"] span')).map((s) => s.textContent?.trim() ?? ""),
      );
      if (spans.length) break;
      await browser.pause(300);
    }
    expect(spans).toEqual(["ISO 200", "50 mm", "f/8.0", "1/250s"]);
  });
});
