# RFC-0030: Import and export speed: thumbnails visible as soon as possible

- Status: Study with measurements, proposal for review (nothing implemented)
- Date: 2026-10-19
- Asked by the user: "study how to accelerate import/export speed. The key is to let thumbnails ready as asap."
- Relates to: the import pipeline in `import.rs`, `jpeg_decode.rs`, `source_decode.rs`, `export.rs`, the `import-progress` / `thumbnail-progress` events in `lib.rs`, `importActions.js`

## 1. The finding in one paragraph

Thumbnails are slow to **appear**, mostly for a reason that is not decode speed: the grid is refreshed only twice (after cataloguing, and after the **whole** thumbnail pass), so with N photos nothing shows until all N thumbnails exist. The pass itself is serial, one core of ten. Measured on this machine, 8 workers finish a 40-JPEG set 7.8x sooner than one, and the first thumbnail of any worker pool exists after about 20 ms. So the order of work is: stream each thumbnail to the grid as it is written (no algorithm change, the first picture arrives in ~20 ms instead of at the end), then parallelise, then overlap thumbnailing with cataloguing, and only then tune the pixel work (worth ~25%, not 10x).

## 2. How it works today

1. `import_folder` / `import_files` take the catalog lock for the **whole** import (`lib.rs`) and run `import_paths_with_progress` serially: read the file, blake3, dedupe, EXIF, catalog row. A RAW file's embedded JPEG is written as its thumbnail right there (cheap, §4.2). A JPEG gets **no thumbnail yet**.
2. When that loop ends the frontend (`runImport`) calls `refresh()`; every JPEG shows as a placeholder.
3. It then calls `backfill_missing_thumbnails`: a sequential FIFO pass over the batch (`generate_missing_thumbnails_with_progress`): full `image::open` decode, EXIF orientation applied to the **full-size** image, `Triangle` resize to ≤1024 px, `image`-crate JPEG encode. It emits `thumbnail-progress {current,total}`, which only moves a progress bar (`appEvents.js`).
4. Only when the pass returns does the frontend `refresh()` again. Every thumbnail appears at that moment.

So time-to-first-visible-thumbnail = the whole import + the whole thumbnail pass, and time-to-all = the same.

## 3. Method

An ignored test, `import::speed_probe` in `import.rs` (three tests: `jpeg_stages`, `raw_stages`, `parallel_scaling`), run with

```
IMPORT_PROBE_DIR=<dir> cargo test --release --lib speed_probe -- --ignored --nocapture --test-threads=1
```

Machine: 10 cores (8 performance), 32 GB, macOS, warm file cache (so these are CPU and memory costs, **not** a cold disk or card reader). Each stage is the best of three. Corpus: ten JPEGs (0.3 to 19 MB, 1 to 24 MP, six of them high-ISO noisy NIND files that are the worst case for decode) and three Canon CR2 files. "current" is a faithful copy of today's path; "candidate" is the cheaper variant of §5.5. `djpeg` (libjpeg-turbo 3.1.3) and `sips` are references. **Not measured:** the real app end to end (IPC, grid repaint, SQLite insert cost), a cold disk, Windows.

## 4. Numbers

### 4.1 JPEG, per file, ms

| file | MB | MP | read | hash | decode | orient | resize | encode | current total | candidate |
|---|---|---|---|---|---|---|---|---|---|---|
| panorama frame | 0.3 | 1.1 | 0.1 | 0.2 | 5.4 | 0.1 | 9.2 | 5.1 | 20 | 11 |
| landscape | 0.5 | 3.1 | 0.0 | 0.3 | 24.7 | 0.2 | 41.4 | 7.7 | 74 | 34 |
| portrait | 0.6 | 5.1 | 0.1 | 0.3 | 13.9 | 0.4 | 24.9 | 5.7 | 45 | 27 |
| 24 MP clean | 1.7 | 24.0 | 0.1 | 0.9 | 56.7 | 1.7 | 80.8 | 6.0 | 143 | 83 |
| chapel (ISO 6400) | 3.5 | 9.8 | 0.3 | 1.9 | 52.9 | 0.7 | 40.3 | 7.7 | 101 | 70 |
| stairs (ISO 6400) | 11.6 | 7.0 | 1.6 | 6.3 | 131.0 | 0.5 | 35.1 | 10.5 | 177 | 149 |
| Leonidas (ISO 6400) | 16.6 | 10.0 | 2.7 | 9.2 | 194.1 | 0.7 | 44.9 | 9.8 | 246 | 210 |
| fruits (ISO 6400) | 18.9 | 12.3 | 2.9 | 10.3 | 224.7 | 0.9 | 51.0 | 10.1 | 286 | 246 |

(EXIF parse is 0.0 to 0.4 ms, the catalog-side work not shown here is small by comparison.) What this says:

- **Decode is the largest cost and tracks file size, not pixel count**: ~12 ms per MB of file on noisy files (Huffman decoding), ~2.4 ms per MP on clean ones. There is no decoder in the tree faster than the one used; `djpeg` full decode is 185 ms for `fruits` where `zune-jpeg` takes 225 ms.
- **DCT-domain downscaling does not rescue it.** `djpeg -scale 1/8` was only 5 to 20% faster than full on these files (fruits 185 → 156 ms; the 24 MP clean file 48 → 30 ms), because entropy decoding, not the inverse DCT, is the floor. The macOS `sips -Z 512` took 56 to 117 ms, slower than the app's own path on small files. **There is no decode shortcut worth a dependency.**
- **Resize is the second cost** (35 to 81 ms): a `Triangle` filter from the full image to 1024. A cheap exact box average first (÷2, ÷3, … until the image is under 2x the target) then `Triangle` for the rest, orienting the **small** picture and using `jpeg-encoder` (already a dependency), cut the total by 15 to 60% (the candidate column) and shrank the thumbnail file 3 to 4x because the target is 512 px here (§5.5).
- Hashing (blake3, 1.8 GB/s) and reading are not worth touching: ≤13 ms for a 19 MB file.

### 4.2 RAW (three Canon CR2, 9 to 10 MB)

Reading + hashing 6 to 7 ms; opening 0.0 ms; extracting the three embedded JPEGs (160x120, 486x324, 1936x1288) **2.3 ms**. The app already writes the large embedded JPEG as the thumbnail with no decode at all, so a RAW's thumbnail is ready the moment the file is read: **import of RAW is already thumbnail-fast**. Producing a 512 px thumbnail from the large embedded JPEG instead would cost 12 ms more per file (§5.5), a later option, not a need.

### 4.3 Parallel scaling (40 JPEG files, 284 MB, read + hash + EXIF + thumbnail, one shared queue)

| workers | current: all done | files/s | candidate: all done | files/s | first thumbnail (current / candidate) |
|---|---|---|---|---|---|
| 1 | 5782 ms | 6.9 | 4494 ms | 8.9 | 20 / 12 ms |
| 2 | 2999 | 13.3 | 2332 | 17.1 | 21 / 12 |
| 4 | 1587 | 25.2 | 1232 | 32.5 | 21 / 12 |
| 8 | 935 | 42.8 | 737 | 54.3 | 25 / 17 |
| 10 | 916 | 43.7 | 737 | 54.3 | 29 / 18 |

Scaling is near-linear to 4 workers, 6.2x at 8 (the 8 performance cores), flat at 10 (the extra two are efficiency cores). **The first thumbnail is available after 12 to 30 ms whatever the worker count**; the only thing keeping it from the screen today is the end-of-pass refresh.

### 4.4 What this means for a real import (a model, not a measurement of the app)

Using the per-file averages above (mean 139 ms per file for this noisy set): 500 photos today show their first thumbnail after ~70 s and all of them at ~70 s. The same 500 with streaming + 8 workers: first after ~25 ms, all done in ~11 s (current pixel code) or ~9 s (candidate). For clean 24 MP files (143 ms) the picture is the same shape. Reading from a card or a USB disk is the limit once this is done: 284 MB of photos is 3 s at 90 MB/s, which is why read and hash must overlap with decoding rather than precede it (§5.3).

### 4.5 Export

Not re-measured here; the numbers already in the project are 0.33 s of CPU per megapixel and about 60 MB per megapixel in flight. `export_images` renders and encodes **one image after another** (`export_batch`), returns **only when the last one is written** (no per-file event, so the dialog cannot say "3 of 40"), and does hold the catalog lock only briefly (good, it needs no change). A 24 MP photo is therefore about 8 s and 1.4 GB, which is also why an unbounded worker pool is not acceptable: parallelism has to be bounded by memory as well as by cores (§5.6).

## 5. Proposal

Each slice is independently shippable, in this order. Slices A to C are the user's goal ("thumbnails ready asap") and carry no visual change to the thumbnails themselves.

### 5.1 Slice A: show each thumbnail the moment it exists (frontend + events, no pixel change)

The backfill emits a `thumbnail-ready {image_id, thumbnail_path}` event (batched every ~50 ms to avoid flooding the webview) and the grid patches just those rows. `runImport` no longer waits for the pass to `refresh()`. The grid cell already shows a placeholder and loads `convertFileSrc(image.thumbnail_path)`; a thumbnail simply arrives later than the row. Expected: first thumbnail on screen ~25 ms after the first file is processed, then a steady trickle, instead of nothing for the length of the import. Risk: low. Test: a unit test for the patching function, an integration test that events arrive before the command returns.

### 5.2 Slice B: a worker pool for the thumbnail pass

`std::thread::scope` with a shared atomic index over the pending list (the shape the probe uses; no new dependency, `rayon` is not needed). Workers = `min(performance cores, 8)` with at least one core left for the UI; each thread holds one image (≈4 bytes × MP × 2) so memory is bounded at ~0.4 GB for 24 MP files. Ordering: the pass takes the **rows currently visible in the grid first** (the frontend sends their ids; the rest follow in import order), so the part of the grid the user is looking at fills first. The catalog is only touched through the existing `thumbnail_path` update, serialised on the existing mutex. Expected: 5 to 8x on the whole pass (§4.3). Risk: `LIBRAW_LOCK` is global, so a **RAW thumbnail** must keep going through the existing serial path (it is 2 ms anyway); JPEG only is parallel.

### 5.3 Slice C: thumbnails during import, per-insert locking

Import becomes a two-stage pipeline: stage 1 workers read + hash + extract EXIF + (JPEG) decode and write the thumbnail; stage 2 (one thread) dedupes by hash and inserts the catalog row with a **per-row** lock, not one lock for the whole import. The row is announced with `image-added` (batched), which lets the grid fill **during** the import instead of after it; the thumbnail event of Slice A follows. This removes the "import, then backfill" phases and lets disk reads (the actual limit on a card) overlap with CPU. Larger change, so only after A and B are measured in the app. A thumbnail for a file that turns out to be a duplicate is deleted again (rare, bounded waste).

### 5.4 Slice D (optional): a tier-0 picture for JPEGs

Camera JPEGs carry a 160x120 EXIF thumbnail (RAW files have one too). Extracting it costs well under 1 ms. It would show as a blurred placeholder in the first frame of an import for every camera JPEG, replaced by the real thumbnail. Only worth doing if A and B leave a visible gap on slow disks; edited or exported JPEGs often have a stale or missing EXIF thumbnail, so it can never be the only one.

### 5.5 Slice E: cheaper pixel work (≈25%, plus a decision for you)

Box prefilter before `Triangle`, orient after the resize, `jpeg-encoder` instead of the `image` encoder: 15 to 60% less time per file with a result that should look the same in the grid (**not yet compared by eye**: the probe checks times and file sizes only, a visual A/B on the test set is part of the slice). Separately, the thumbnail's **maximum size** (1024 px today) drives half of the cost and the file size: at 512 px the files shrink from 60 to 260 KB to 20 to 65 KB (3 to 4x), which also makes the grid scroll lighter. **Decision for you:** the largest cell the grid can show and whether Loupe/Develop already use their own preview (they do, via `preview_cache`). If nothing needs 1024 px, 640 is the proposed value; if the biggest grid zoom on a 5K display is shown at 1024 px, keep it and take the 25% only.

### 5.6 Slice F: export in parallel, with progress

`export_batch` renders with a pool bounded by **memory**: `workers = min(cores − 2, floor(budget / (60 MB × MP)))` where the budget is a fraction of the system's free RAM (a 24 MP photo is about 1.4 GB, so 4 at once on a 32 GB machine). An `export-progress {done,total,current_name}` event per finished file makes the dialog show real progress and lets Cancel stop between files; results still come back in input order. Expected: the same 5 to 7x as thumbnails on a multi-photo export, 1x on a single photo. The first output file shows up as soon as the first render ends, not when the last does. Plugins stay sequential per file (they are fire-and-forget already).

### 5.7 Not proposed

- **A new JPEG decoder or a native library** (§4.1: the floor is entropy decoding).
- **GPU decode/resize**: the transfer and setup cost loses against 20 ms of CPU for a thumbnail.
- **Lower-quality thumbnails** (the candidate encodes at quality 82 where the `image` crate defaults to 75; sizes above reflect that, not a quality cut).

## 6. Expected result

| | today | after A | after A+B | after A+B+C |
|---|---|---|---|---|
| first thumbnail on screen | after the whole import + pass | ~25 ms after the first file | same | same, and rows appear during the import |
| 500 photos, all thumbnails | ~70 s | ~70 s but visibly filling | ~11 s | ~9 s |
| export of many photos | serial, no progress until the end | | | F (independent of A to C): 5 to 7x, with per-file progress |

(All of this is the model of §4.4, from stage timings on a warm cache; the first slice to land measures it in the real app on the user's machine and replaces these numbers.)

## 7. Plan and verification

Four to five PRs: A (+ a small harness script that times first-thumbnail in the bundled app), B, C, E, F. Each lands with the probe re-run and its numbers in `PROGRESS.md`. The M6 milestone-end backlog gets: Windows timing (the two slow cores and Defender scanning the cache dir change the picture), a cold-disk/card import, and a 2,000-photo import for memory.

## 8. Open questions for you

1. Order: A → B → C → E → F as above, or export (F) earlier?
2. Thumbnail size: keep 1024, or 640 (or 512) if nothing needs more? (§5.5)
3. Is a placeholder from the EXIF thumbnail (D) wanted, or is "blank cell, then picture" fine once pictures arrive within ~50 ms each?
