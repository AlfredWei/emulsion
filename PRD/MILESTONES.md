# Milestone Plan

Companion to [PRD.md](PRD.md). Sequenced against Lightroom's own build order (see [lightroom-reference.md](lightroom-reference.md)) — cataloging and basic RAW develop first, local adjustments next, modern tone engine and output modules after that, then performance/GPU, then AI-assisted features last. Sizing assumes a solo/small indie team with no fixed deadline: month-ranges are rough relative sizing, not commitments.

Each milestone lists **scope**, **explicitly deferred**, and **exit criteria** (what "done" means before starting the next one).

---

## M0 — Foundations & tech spike
**Rough size:** 1–3 months · **Lightroom analog:** pre-v1 engineering, not a shipped release

### Scope
- Choose the cross-platform UI/app framework (native-ish toolkit + Rust/C++ core, vs. Qt/C++, vs. alternatives) — evaluate on: native look/feel on both macOS and Windows, GPU access story, packaging/distribution story.
- Integrate the chosen RAW decode library (e.g., LibRaw) and prove end-to-end: read a RAW file → demosaic → get raw pixel buffer.
- Design the catalog schema v0 (images, folders, edit-instruction records, collections, keywords) in an embedded DB (e.g., SQLite).
- Design the non-destructive edit model: how an "edit stack" is represented, versioned, and replayed against decoded RAW data to produce a rendered preview.
- Prove a basic color-managed render path: RAW → linear working space → display-profile-correct on-screen preview.
- Spike GPU-accelerated rendering feasibility on both target OSes.

### Explicitly deferred
- Any user-facing UI polish.
- Any feature from M1+.

### Exit criteria
- A throwaway prototype can open a folder of RAW files, decode one, apply a hardcoded exposure/WB adjustment, and display a color-correct preview on both macOS and Windows.
- Catalog schema v0 is written down and can store an image reference + one edit record.
- Framework, RAW library, and DB choices are documented as decisions (with rationale) in this repo.

---

## M1 — MVP: Import → Library → Basic Develop → Export
**Rough size:** 3–6 months · **Lightroom analog:** v1.0, 2007

### Scope
- **Import**: pick a folder/volume, copy or add-in-place, background thumbnail generation, basic duplicate detection.
- **Library**: grid + loupe view, flags/star ratings/color labels, filter/sort by those + basic metadata, folder-based browsing, virtual copies, basic stacking.
- **Develop**: single non-destructive edit stack per image with history/undo and snapshots; global adjustments only — white balance, exposure, contrast, highlights/shadows/whites/blacks, saturation/vibrance, basic sharpening and noise reduction, crop/straighten/rotate.
- **Develop preview cache**: background/on-demand generation of a persistent, resized proxy per image so opening Develop doesn't require decoding the source RAW file fresh every time (Lightroom's Standard/1:1 Preview cache model — see [PRD §7.6](PRD.md#76-performance--data-integrity-cross-cutting-not-a-feature-but-a-requirement)). Distinct from M3's Smart Previews below: this is about redecode cost on a *locally available* file, not offline/disconnected-volume editing. Identified as a real gap during the Develop-pipeline slice, where every Develop open was measurably slow because it decoded the RAW from scratch — pulled into M1 scope rather than left implicit.
- **Export**: JPEG/TIFF export with resize, output sharpening, color space, quality, filename template; batch export in the background.
- Catalog persistence, crash-safe (no edit loss on crash).

### Explicitly deferred
- Local/masked adjustments (brush, gradients) → M2.
- Keywording, smart collections, publish/print → M2/M3.
- Any AI feature.

### Exit criteria
- A real shoot (hundreds of RAW files) can go: import → cull with flags/ratings → basic-develop each keeper → export delivery JPEGs, entirely in-app.
- No data loss across app restarts or crashes mid-edit.
- This is the first installable, dogfoodable build.

---

## M2 — Local adjustments & non-destructive toolkit
**Rough size:** 3–5 months · **Lightroom analog:** v2.0–v3.0, 2008–2010

### Scope
- Local adjustments: linear gradient, radial gradient, adjustment brush (with auto-mask), all as composable masks feeding the edit stack.
- Range masking: color range and luminance range selection.
- Tone curve, HSL/color mixer, split toning, dehaze, clarity/texture, vignette, grain.
- Lens corrections (profile-based distortion/vignette/CA).
- Presets: create/save/apply, import/export as files.
- Keywording (hierarchical), full IPTC metadata editing.
- Collections (manual) and Smart Collections (rule-based).
- Improved noise reduction (luma/chroma split), sharpening quality pass.
- Catalog backup, Lightroom-style: prompt-on-close with a configurable frequency (every time / daily / weekly / monthly / never), optional integrity check, timestamped copy written to a user-chosen backup folder separate from the working catalog — catalog file only, not the photos (see [PRD §7.6](PRD.md#76-performance--data-integrity-cross-cutting-not-a-feature-but-a-requirement)).

### Explicitly deferred
- Modern highlights/shadows tone *model* overhaul is already in M1 (pulled forward vs. Lightroom's actual timeline since it's foundational, not optional) — no change needed here.
- Healing/clone brush, perspective/upright correction, soft proofing, print/book/web output → M3.
- Tethered capture: evaluate at M3 planning time; not committed here.

### Exit criteria
- A photographer can fully locally-adjust an image (e.g., dodge/burn a face, darken a sky, selectively desaturate) without leaving the app.
- Presets can be created, applied across a batch, and reused across sessions.
- Keyword-based search and smart collections work over a multi-thousand-image catalog without noticeable lag.
- A deliberately corrupted or deleted catalog file can be recovered from a scheduled backup with at most one backup-interval's worth of edits lost.

---

## M3 — Modern retouching & output modules
**Rough size:** 3–5 months · **Lightroom analog:** v4.0–v5.0, 2012–2013

### Scope
- Healing/clone brush (advanced retouching beyond basic spot removal), red-eye removal.
- Perspective/upright correction (manual controls; auto-upright is a stretch goal, not required).
- Soft proofing against output profiles.
- Print module: layout templates, page setup, printer color management.
- Smart Previews: lightweight proxy generation so Develop works smoothly even against files on a disconnected/offline external drive (still valuable with zero cloud — this is about disk I/O, not sync). Distinct from M1's Develop preview cache: that one speeds up a *present* file's redecode cost; this one keeps Develop usable when the original isn't reachable at all.
- Decide and scope: Map/geotagging view, Book module — both optional based on user demand signal from M1/M2 dogfooding; not required for exit.

### Explicitly deferred
- GPU pipeline rewrite (if not already covered by M0 spike outcomes) → M4.
- Face recognition, HDR/pano merge → M4.

### Exit criteria
- Retouching quality is good enough that most images no longer need round-tripping to another tool for spot/object removal.
- Print output is color-accurate (soft-proof matches physical print within reasonable tolerance).
- Editing against files on a slow/offline external drive doesn't degrade the Develop experience (Smart Previews working).

---

## M4 — Performance, GPU, merges, faces
**Rough size:** 3–6 months · **Lightroom analog:** v6.0/CC, 2015

### Scope
- Full GPU-accelerated Develop rendering (building on the M0 spike) with correct, seamless CPU fallback.
- HDR merge (multi-exposure → single high-bit-depth composite).
- Panorama merge (multi-shot → stitched composite), including boundary/edge correction.
- Face detection/recognition for a local "People" browsing view (fully local, no cloud model dependency).
- Basic video handling: import/organize/trim (explicitly not a video editor — see PRD non-goals).
- Plugin/extensibility API v0 (even a minimal export-plugin hook is useful here and de-risks M7's extensibility work).

### Explicitly deferred
- Any AI masking/selection beyond face detection → M5.

### Exit criteria
- Develop-panel slider response stays under the PRD's ~100ms target on a 50k-image catalog with GPU acceleration active.
- HDR and panorama merges produce output quality comparable to dedicated tools for typical cases.
- Face-grouping is usable for culling/organizing a portrait- or event-heavy catalog.

---

## M5 — AI-assisted selection & enhancement
**Rough size:** 4–8 months · **Lightroom analog:** Classic v8–v11, 2018–2022

### Scope
- Select Subject / Select Sky (and similar landscape-element masks: water, architecture, vegetation) as one-click AI-generated masks feeding the existing mask/adjustment system from M2.
- Composable masking: add/subtract/intersect multiple masks (AI-generated or manual) in one edit.
- AI-assisted denoise (distinct from and generally higher quality than M1/M2's traditional NR).
- AI super-resolution / upscaling.
- **Architecture decision required before scoping in detail**: on-device model inference only (no cloud calls, consistent with the "local-first, cloud out of scope" constraint) — this determines model size/quality tradeoffs and what hardware (e.g., minimum GPU/NPU) is required. Document the decision before implementation starts.

### Explicitly deferred
- Generative fill/remove → M6 (bigger model, bigger architecture question).

### Exit criteria
- One-click subject/sky selection is accurate enough to be faster than manual brush masking for common compositions.
- AI denoise/upscale run fully offline and complete in a reasonable time on target hardware (define the target machine spec as part of this milestone's kickoff).

---

## M6 — Generative & intelligent culling
**Rough size:** 4–8 months, **explicitly speculative** · **Lightroom analog:** Classic v12+, 2023–2026

### Scope (provisional — confirm feasibility before committing)
- Generative object removal/fill. Given the "no cloud" constraint, this requires either a bundled on-device generative model (quality/size/license tradeoffs, likely the single biggest open technical question in this whole roadmap) or a deliberate decision to ship a **non-generative** content-aware fill instead (extending M3's healing brush rather than true generative fill).
- Adaptive/context-aware auto-tone presets.
- Automatic duplicate/near-duplicate detection for culling.
- AI-assisted culling aids (e.g., blur/focus detection, blink detection) as a "Faces/culling panel."

### Explicitly deferred
- Nothing beyond this — M6 is the top of the AI feature set. Anything past this point (further Adobe Firefly-style features) is out of scope by the PRD's non-goals unless revisited.

### Exit criteria
- A concrete decision is made and documented on the generative-fill approach (bundled model vs. non-generative fallback) *before* implementation, since it changes the milestone's shape substantially.
- Duplicate detection and culling aids measurably reduce time-to-cull on a large event/burst-heavy import.

---

## M7 — Polish, extensibility, 1.0 launch
**Rough size:** 2–4 months · **Lightroom analog:** n/a — this is packaging/hardening, not a feature era

### Scope
- Harden the plugin/extensibility API from M4 into something documented and stable enough for third parties.
- Accessibility pass (keyboard navigation, screen reader labels, contrast).
- Localization scaffolding (even if only one language ships at 1.0).
- Licensing/distribution decision (open source? paid? — not decided in this PRD) and packaging for both target OSes (installers, code signing, auto-update mechanism if any).
- Full documentation: user-facing help, keyboard shortcuts reference.
- Performance/regression pass across the full feature set built in M1–M6.

### Exit criteria
- 1.0 release: installable on both macOS and Windows, passes a full manual regression pass of the M1–M6 feature set, has user-facing documentation.
