# RFC-0024: Opening an image in Develop — where the time goes, and what to change

- Status: Draft for review (investigation only; no product code changed)
- Date: 2026-10-02
- Relates to: [preview_cache.rs](../../app/src-tauri/src/preview_cache.rs) (M1 Slice 4), [ADR-0004](../adr/ADR-0004-rendering-and-color-pipeline.md), M5 performance budget

## 0. Trigger and scope

User report: switching to Develop is slow. Questions asked: do we decode all photos at once or one image, and how could cache / thumbnails / lazy loading help?

**Limits of this investigation, stated first.** The only RAW files available here are three 9–10 MB Canon CR2s whose Develop preview is 1953×1301 (2.5 MP). Real cameras produce 25–60 MB files and 24–60 MP frames. So the *shape* of the costs below is measured, but the *scaling* to the user's files is extrapolated, and the user's own slow case (which files, which drive, first open or every open) is not yet known. The webview-side timings were taken on a **debug** build (unoptimised Rust, real WKWebView/WebGPU); the Rust-side numbers were taken separately in release. Nothing was measured on Windows.

## 1. Answers to the two direct questions

1. **One image, not all.** `openDevelop` loads exactly one image's edit stack, history and snapshots, and `DevelopCanvas` asks for exactly one preview (`get_develop_preview`). There is no decode-everything step on the open path.
2. **But there *is* an all-photos background job.** `pregenerate_missing` walks the whole catalog and builds each image's 2048-px preview, **sequentially, one image at a time, with no priority** (after every import, and at startup; `preview_cache.rs`). Its own doc comment records the accepted trade-off: an interactive open of a not-yet-cached image competes with it for CPU. So an image opened *before* its preview exists pays a full RAW decode on the spot.

## 2. What opening Develop does (code path)

`openDevelop` → await `flushEditStack` → fetch stack/history/snapshots → switch module (**`DevelopModule`/`DevelopCanvas` are mounted fresh every time**, `+page.svelte` `{#if shell.activeModule === "develop"}`) → `lookup_lens_profile` → in the canvas: acquire WebGPU device → `get_develop_preview(path, content_hash)` → fetch the cached PNG over the asset protocol → `createImageBitmap` → upload to a GPU texture → first render.

`get_develop_preview` **reads the whole RAW file and BLAKE3-hashes it on every open, even on a cache hit** (`ensure_develop_preview`), then looks the PNG up by that hash. The frontend already passes the catalog's `content_hash`, but it is only used as a fallback when the source file is missing (Smart Previews). The existing comment calls this "negligible" because of the OS page cache.

## 3. Measurements

### 3.1 Webview side — debug dev build, 2.5 MP preview, ms from the start of `openDevelop`

| Step | 1st entry of the session | Later entries (module remount) | Switch image inside Develop |
|---|---|---|---|
| WebGPU device acquired (`initGpu`) | 28 | 13–22 | already held (0) |
| `lookup_lens_profile` returned | **489** | 13–23 | 7–15 |
| `get_develop_preview` returned | **494** | 137–147 | 132–134 |
| PNG fetched (3.7–5 MB) | +5 | +4 | +3 |
| `createImageBitmap` | +43 | +39–41 | +40 |
| GPU texture upload | +30 | +21 | +20 |
| First render GPU-complete | **676** | 322–331 | 301–306 |

(Full log: [appendix](RFC-0024-appendix/develop-open-trace.log).)

### 3.2 Rust side — release, 9–10 MB CR2

| | ms |
|---|---|
| `ensure_develop_preview` **cache hit** (read + hash + stat) | 6.4–7.4 (read 1.9–2.8, BLAKE3 5.5–6.3 → ~1.5 GB/s) |
| cache **miss**: LibRaw half-size decode | 187–193 |
| resize / PNG encode / PNG decode (`image` crate) | 0.5 / 13–15 / 17–19 |
| first `lens_db()` (bundled lensfun database) | 42 (debug build: ~450) |

### 3.3 What the numbers say

1. **Re-initialising the GPU on every Develop entry is *not* the problem.** I expected the remount to be the cost; the device is acquired in ~2–20 ms. (Pipeline compilation is lazy, so it lands in the first render, ~100 ms, which is paid even when only the image changes — a render cost, not a mount cost.)
2. **The first Develop open of a session is ~2× slower** than later ones (676 vs ~330 ms): `lookup_lens_profile` is a *synchronous* Tauri command that lazily loads the bundled lensfun database. Sync commands run on the main thread, and in this trace the preview request returned at the same instant (494 vs 489 ms), i.e. it waited behind it. In release the load is 42 ms, so this is a ~40 ms hiccup there, not 450 ms — but it is a main-thread block either way.
3. **In steady state the cost is split between four things:** the Rust command (125 ms in this debug build; ~7 ms in release for these files), PNG decode in the webview (~40 ms), texture upload (~20 ms), and the first full-pipeline render (~75–110 ms). A release-build estimate for these small files is therefore **~200 ms** (330 − 125 + 7) — *estimated, not measured*; the installed release app *felt* fine on these files.
4. **None of this scales with megapixels except the Rust command.** The preview is capped at 2048 px, so PNG decode, upload and render cost the same for a 60 MP file as a 2.5 MP one. What grows with the user's files:
   - **Every open:** read + hash of the *whole RAW*. At ~1.5 GB/s hashing plus read, a 60 MB file is ≈ 60 ms warm and as slow as the disk when cold (external drive, network share, file not in the page cache). This is the only per-open cost proportional to file size.
   - **Cache miss:** a full LibRaw decode, ≈ 190 ms per 10 MP file here; **extrapolated** linearly to ≈ 0.8–1 s for 45 MP (release), several times that in a debug build. This is the multi-second "slow" a user sees on a newly imported photo opened before background pregeneration reaches it.

## 4. Options

| # | Change | Expected effect | Cost / risk |
|---|---|---|---|
| A | **Stop reading + hashing the RAW on open.** Use the catalog's `content_hash` to find the cached preview directly; verify freshness with file size + mtime (record mtime at import; size is already stored) instead of re-hashing | Removes the only per-open cost that grows with file size; removes the cold read of the RAW on every open (matters most for external/network drives) | Small. Behaviour change: a file modified in place with the same size and mtime would no longer be noticed; needs a fallback (rehash when mtime/size differ) and a migration for rows without mtime |
| B | **Embedded-JPEG placeholder on a cache miss.** The import path already extracts the RAW's embedded JPEG cheaply for thumbnails (`unpack_thumb`); show it immediately in Develop while the real decode runs, then swap | Turns a 1–3 s blank wait on an uncached image into an instant image | Medium. The camera-rendered colour differs from this app's decode, so the swap will show a colour shift (fine as a "loading" state, bad if it lingers); needs a "preview quality" indicator |
| C | **Prioritise and parallelise `pregenerate_missing`.** Order by the current selection / visible grid first; 2–3 workers instead of one; pre-generate the selected image's neighbours first | Fewer cache misses at all, and a miss no longer fights a backlog | Medium. CPU/RAM spikes were the stated reason it is sequential (multi-ten-MB RAW decodes); cap workers and keep it below interactive priority |
| D | **Prefetch neighbours and keep a small LRU of decoded textures.** Fetch + decode ±2 filmstrip neighbours as `ImageBitmap`s, keep the last few GPU source textures | Filmstrip stepping and "back to the image I just left" skip IPC + PNG decode + upload (~200 ms of the ~330) | Medium. GPU memory per texture (2048×1365×4 ≈ 11 MB each) — cap at ~4; invalidate on file change |
| E | **Warm the lens database at startup and make `lookup_lens_profile` async** (and audit other sync commands that take locks) | Removes the first-open main-thread stall (42 ms release, ~450 ms debug) | Tiny |
| F | **Skip PNG decode by caching the preview as raw RGBA**, uploaded with `writeTexture` | Saves ~40 ms per open | Larger cache (≈ 11 MB vs ≈ 3–5 MB per image) and a custom read path; modest win — **not recommended first** |
| G | **First render (~75–110 ms)** — check whether passes with an identity setting (dehaze, clarity, texture, sharpen, both NRs) are skipped for an unedited image | Possibly the largest remaining fixed cost | Unknown: I did not investigate the render graph; this is a separate (M5-budget) item and needs GPU timestamps |
| — | *Not recommended:* keeping `DevelopModule` permanently mounted to save GPU re-init (measured at ~2–20 ms), or switching the cached preview to lossy JPEG (precision under heavy grading) | | |

## 5. Recommendation

Do **A + E first** (small, safe, remove the only size-proportional and the only first-open costs), then **B + C** if the user's slow case turns out to be "just imported, opened an uncached photo" — which the code makes the most likely multi-second case — and **D** for navigation feel. **G** deserves its own measurement.

## 6. What I need from the user to choose

- Is the slow case **the first time you open a freshly imported photo**, **every open**, or **only the first open after launching the app**? (Predictions: B/C, A, and E respectively.)
- A folder with your **typical RAW files** (camera model, 24–45 MP, internal vs external drive) so cache-miss and hash costs can be measured on real data instead of extrapolated from 10 MP files.
- Whether the slow build is the installed release app or `tauri dev` — the debug build's Rust side is ~10–20× slower and would explain most of a "slow" report on its own.

## 7. Non-goals

Interactive slider render latency (M5's own budget), export speed, and Library grid scrolling.
