import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE = path.resolve(__dirname, "../../../test_image/Field-corn-Liechtenstein-landscape.jpg");
const BUNDLE_ID = process.env.LANE_BUNDLE_ID;
const OUT = process.env.LANE_OUT ?? fs.mkdtempSync(path.join(os.tmpdir(), "sandbox-smoke-"));
if (!BUNDLE_ID) throw new Error("LANE_BUNDLE_ID must name the sandboxed app's bundle identifier");

const CONTAINER = path.join(os.homedir(), "Library", "Containers", BUNDLE_ID, "Data");

const invoke = (cmd, args) =>
  browser.execute(
    async (c, a) => {
      try {
        return { ok: await window.__TAURI__.core.invoke(c, a) };
      } catch (e) {
        return { err: String(e && e.message ? e.message : e) };
      }
    },
    cmd,
    args,
  );

describe("sandbox smoke", () => {
  const facts = {};
  let inside;
  let outside;

  before(async () => {
    await browser.setTimeout({ script: 180000 });
    fs.mkdirSync(OUT, { recursive: true });
    inside = path.join(CONTAINER, "Documents", "lane-fixtures");
    fs.mkdirSync(inside, { recursive: true });
    fs.copyFileSync(FIXTURE, path.join(inside, path.basename(FIXTURE)));
    outside = fs.mkdtempSync(path.join(os.tmpdir(), "sandbox-smoke-outside-"));
    fs.copyFileSync(FIXTURE, path.join(outside, path.basename(FIXTURE)));
  });

  after(() => fs.writeFileSync(path.join(OUT, "sandbox-smoke.json"), JSON.stringify(facts, null, 2)));

  it("keeps the data directory inside the container", async () => {
    const info = await invoke("get_storage_info");
    expect(info.err).toBeUndefined();
    facts.container = CONTAINER;
    facts.data_dir = info.ok.effective_dir;
    expect(fs.existsSync(CONTAINER)).toBe(true);
    expect(fs.realpathSync(info.ok.effective_dir).startsWith(fs.realpathSync(CONTAINER) + path.sep)).toBe(true);
  });

  it("starts the AI helper", async () => {
    const before = await invoke("ai_helper_info");
    expect(before.ok.exists).toBe(true);
    const imported = await invoke("import_files", { paths: [path.join(inside, path.basename(FIXTURE))] });
    expect(imported.ok.imported).toBe(1);
    const image = (await invoke("list_images")).ok[0];
    const prepared = await invoke("segment_prepare", { path: image.path, contentHash: image.content_hash });
    facts.segment_prepare = prepared;
    expect(prepared.err).toBeUndefined();
    facts.helper = (await invoke("ai_helper_info")).ok;
    expect(facts.helper.running).toBe(true);
  });

  it("refuses an import from outside the container", async () => {
    const summary = await invoke("import_files", { paths: [path.join(outside, path.basename(FIXTURE))] });
    facts.import_outside = summary;
    expect(summary.err).toBeUndefined();
    expect(summary.ok.imported).toBe(0);
    expect(summary.ok.failed).toBe(1);
  });
});
