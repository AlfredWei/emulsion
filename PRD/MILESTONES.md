# Milestone Plan

Companion to [PRD.md](PRD.md). Sequenced against Lightroom's own build order (see [lightroom-reference.md](lightroom-reference.md)) where practical, with one deliberate divergence: **M2 deepens photo management and cataloging** (multi-file import, broader format support, full metadata, catalog hygiene) **before the local-adjustment edit pipeline (M3)** — a product decision (2026-07-26) to establish a complete, trustworthy photo-management foundation before expanding editing capability, rather than following Lightroom's own v1→v2 chronology strictly. From M4 onward the sequencing returns to matching Lightroom's own build order: modern tone engine and output modules, then performance/GPU, then AI-assisted features last. Sizing assumes a solo/small indie team with no fixed deadline: month-ranges are rough relative sizing, not commitments.

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
- **Develop preview cache**: background/on-demand generation of a persistent, resized proxy per image so opening Develop doesn't require decoding the source RAW file fresh every time (Lightroom's Standard/1:1 Preview cache model — see [PRD §7.6](PRD.md#76-performance--data-integrity-cross-cutting-not-a-feature-but-a-requirement)). Distinct from M4's Smart Previews below: this is about redecode cost on a *locally available* file, not offline/disconnected-volume editing. Identified as a real gap during the Develop-pipeline slice, where every Develop open was measurably slow because it decoded the RAW from scratch — pulled into M1 scope rather than left implicit.
- **Export**: JPEG/TIFF export with resize, output sharpening, color space, quality, filename template; batch export in the background.
- Catalog persistence, crash-safe (no edit loss on crash).

### Explicitly deferred
- Local/masked adjustments (brush, gradients) → M3.
- Keywording, smart collections → M2; publish/print → M4.
- Any AI feature.

### Exit criteria
- A real shoot (hundreds of RAW files) can go: import → cull with flags/ratings → basic-develop each keeper → export delivery JPEGs, entirely in-app.
- No data loss across app restarts or crashes mid-edit.
- This is the first installable, dogfoodable build.

---

## M2 — Photo management & catalog depth
**Rough size:** 2–4 months · **Lightroom analog:** n/a — deliberately resequenced ahead of Lightroom's own chronology (see this document's intro); the closest spirit is Lightroom's own early cataloging/metadata refinements before v2.0's local-adjustment leap, not a literal version mapping.

### Scope
- **Import**: multi-file selection through a picker dialog (choose specific files, not just an entire folder) as an alternative to M1's whole-folder import.
- **Format support**: extend the RAW-only decode/import pipeline to also catalog, thumbnail, and display standard JPEG files alongside RAW.
- **Full metadata handling**: read and apply each file's embedded color profile (the first real consumer of ADR-0004's still-unwired lcms2 color-management step), read full EXIF (camera, lens, exposure settings) into the catalog and Library's metadata panel (deferred from M1 Slice 2's "metadata panel: EXIF read-only" scope-cut), and IPTC read/write (caption, copyright, contact) matching [PRD §7.3](PRD.md#73-library--organization).
- **Basic photo management, completed**: flags/star ratings (already in M1) plus multi-select in the Library grid (rate/flag/color-label/remove many at once — M1 only supports single selection) and "remove from catalog" (a non-destructive catalog-only removal, confirmed via dialog, that never touches the original file on disk — see [PRD §7.1](PRD.md#71-catalog--library-engine-foundation-not-user-facing-on-its-own)'s "never modify originals").
- Keywording (hierarchical).
- Collections (manual) and Smart Collections (rule-based).
- Catalog backup, Lightroom-style: prompt-on-close with a configurable frequency (every time / daily / weekly / monthly / never), optional integrity check, timestamped copy written to a user-chosen backup folder separate from the working catalog — catalog file only, not the photos (see [PRD §7.6](PRD.md#76-performance--data-integrity-cross-cutting-not-a-feature-but-a-requirement)).

### Explicitly deferred
- The full local-adjustment edit pipeline (masks, gradients, brush, tone curve, HSL, presets) → M3. Deliberately resequenced ahead of Lightroom's own chronology (see this document's intro) — a complete, trustworthy photo-management foundation before expanding editing capability.
- Any AI feature.

### Exit criteria
- Import supports picking specific files (not just a whole folder), and both RAW and JPEG land in the catalog correctly.
- Every cataloged image shows real EXIF (camera/lens/exposure) in the metadata panel, IPTC caption/copyright/contact are editable, and the Library thumbnail/preview reflects the file's own embedded color profile rather than assuming one.
- The Library grid supports multi-select for rating/flagging/color-labeling/removing many images at once, and removing an image from the catalog never touches the original file on disk.
- Keyword-based search and smart collections work over a multi-thousand-image catalog without noticeable lag.
- A deliberately corrupted or deleted catalog file can be recovered from a scheduled backup with at most one backup-interval's worth of edits lost.

---

## M3 — Local adjustments & non-destructive toolkit
**Rough size:** 3–5 months · **Lightroom analog:** v2.0–v3.0, 2008–2010

### Scope
- **Settings/Preferences dialog** (added 2026-07-29, real gap identified once M2's catalog backup shipped): a general, app-wide settings surface — none exists today; the only precedent is M2's backup dialog, deliberately scoped as "the close-prompt is the only settings surface, a future settings surface can add a manual trigger later." Establishes the pattern going forward: new configurable behavior gets a section in this dialog rather than a bespoke one-off dialog per feature. Backup settings (frequency/folder/integrity/optimize) migrate in as the first real section, on top of their current close-prompt-only editing surface (which stays as the prompt itself, just reads/writes the same settings).
- **Quick action area, completed** (added 2026-07-29): Library's flag/star/color-label controls are click-only per-cell badge-row buttons today (`GridCell.svelte`) — no keyboard-shortcut layer exists anywhere in this repo, not even as a documented-but-unbuilt spec. Adds the standard shortcut scheme (number keys for star rating, pick/reject/unflag, color-label keys) so culling speed matches the rest of the app's keyboard-first intent.
- **Basic pan/zoom & view controls** (added 2026-07-29): Develop's canvas is fit-to-window only today — no zoom-level state, no 100%/zoom-to-point, no click-drag pan. Adds the minimum viewing controls any image editor needs; also a practical prerequisite for this milestone's own brush/mask tools, which aren't usable at fit-to-window scale.
- **Extract the develop engine** (added 2026-07-29): consolidate edit-stack interpretation — today duplicated as two independently hand-synced implementations, `DevelopCanvas.svelte`'s WGSL shader (interactive GPU preview) and `export.rs`'s CPU code (final render) — into a single, focused module boundary. Done at the *start* of this milestone, before its own large batch of new ops (tone curve, HSL, split toning, dehaze, clarity/texture, vignette, grain, lens corrections) doubles the duplication surface; sets up M5's later "GPU pipeline... with correct, seamless CPU fallback" work rather than competing with it.
- Local adjustments: linear gradient, radial gradient, adjustment brush (with auto-mask), all as composable masks feeding the edit stack.
- Range masking: color range and luminance range selection.
- Tone curve, HSL/color mixer, split toning, dehaze, clarity/texture, vignette, grain.
- Lens corrections (profile-based distortion/vignette/CA).
- Presets: create/save/apply, import/export as files.
- Improved noise reduction (luma/chroma split), sharpening quality pass.

### Explicitly deferred
- Modern highlights/shadows tone *model* overhaul is already in M1 (pulled forward vs. Lightroom's actual timeline since it's foundational, not optional) — no change needed here.
- Healing/clone brush, perspective/upright correction, soft proofing, print/book/web output → M4.
- Tethered capture: evaluate at M4 planning time; not committed here.
- Library "Loupe" (single-image, non-Develop) view — a separate, already-deferred gap (see PROGRESS.md), not required for this milestone's pan/zoom scope, which is about the Develop canvas.

### Exit criteria
- A photographer can fully locally-adjust an image (e.g., dodge/burn a face, darken a sky, selectively desaturate) without leaving the app.
- Presets can be created, applied across a batch, and reused across sessions.
- A Settings dialog exists, holds at least backup settings plus any new preferences this milestone introduces, and is the documented pattern future milestones are expected to extend rather than bypass.
- Library culling (rate/flag/color-label) is fully keyboard-drivable via the standard shortcut scheme, no mouse required.
- Develop supports fit/100%/zoom + click-drag pan.
- The develop engine's edit-stack interpretation lives in one module boundary, not two independently-maintained implementations, before this milestone's op count grows further.

---

## M4 — Modern retouching & output modules
**Rough size:** 3–5 months · **Lightroom analog:** v4.0–v5.0, 2012–2013

### Scope
- **Develop Auto tools & Basic Tone completion** (added to forefront of M4, 2026-08-20):
  - White Balance system: Temperature & Tint adjustment pipeline (Rust CPU + WGSL GPU), White Balance eyedropper.
  - Auto White Balance (AWB): color constancy / gray-world automatic white balance estimation.
  - Basic tone expansion: Highlights, Shadows, Whites, Blacks adjustments.
  - Auto Tone: intelligent dynamic-range, midtone, and clipping analysis to compute and apply optimal tone settings with one click.
- **Library module depth & UX overhaul** (added to forefront of M4, 2026-08-20):
  - Drag-and-drop file/folder import directly into Library view.
  - Filename and full path display in grid and info bar with "Reveal in File Manager" capability.
  - Complete EXIF metadata extraction & display with customized floating tooltip bubbles for truncated text.
  - GPS / Geo-location extraction from EXIF, coordinate & altitude display, map viewing links, and manual geo-location entry/editing.
  - Library multi-dimensional filter bar (rating, flag, color label, camera, lens, date range, text search).
  - Library view modes: Grid, Loupe (Single View with full-resolution interactive Pan & Zoom), Compare View (synchronized side-by-side comparison with candidate navigation), and Survey View (multi-photo responsive matrix).
  - Multi-selection batch culling: batch setting of flags (Pick/Reject/Unflag), color labels (6 colors), and star ratings (0–5) with selection counter badges.
  - Comprehensive hotkeys & keyboard customization: arrow keys navigation with Shift range extension, view mode hotkeys (G/E/C/N/D/Space), and interactive shortcut configuration/rebinding in Settings dialog.
  - Modern UI icon buttons: SVG icons for Auto WB, Auto Tone, Pick, Reject, and Unflag.
- Healing/clone brush (advanced retouching beyond basic spot removal), red-eye removal.
- Perspective/upright correction (manual controls; auto-upright is a stretch goal, not required).
- Soft proofing against output profiles.
- Print module: layout templates, page setup, printer color management.
- Smart Previews: lightweight proxy generation so Develop works smoothly even against files on a disconnected/offline external drive (still valuable with zero cloud — this is about disk I/O, not sync). Distinct from M1's Develop preview cache: that one speeds up a *present* file's redecode cost; this one keeps Develop usable when the original isn't reachable at all.
- Decide and scope: Map/geotagging view, Book module — both optional based on user demand signal from M1–M3 dogfooding; not required for exit.

### Explicitly deferred
- GPU pipeline rewrite (if not already covered by M0 spike outcomes) → M5.
- Face recognition, HDR/pano merge → M5.

### Exit criteria
- Retouching quality is good enough that most images no longer need round-tripping to another tool for spot/object removal.
- One-click Auto Tone and Auto White Balance provide reliable, natural starting points for editing.
- Library allows drag-and-drop import, detailed EXIF and GPS inspection/editing, flexible multi-criteria filtering, full view modes (Grid, Loupe, Compare, Survey), batch culling, and customizable keyboard shortcuts.
- Print output is color-accurate (soft-proof matches physical print within reasonable tolerance).
- Editing against files on a slow/offline external drive doesn't degrade the Develop experience (Smart Previews working).

---

## M4.5 — Develop & workflow UX polish
**Rough size:** 1–2 months · **Lightroom analog:** n/a — workflow refinements identified from dogfooding M1–M4, not tied to a specific Lightroom version release

Inserted between M4 and M5 (2026-08-30, user request after M4's Print Module shipped) — these are lower-risk, high-frequency workflow wins surfaced by actually using the app day-to-day, worth landing before M5's heavier GPU/merge/faces work.

### Scope
- Copy/paste Develop settings: copy an edit stack (or a chosen subset of its groups) from one image, paste onto another.
- Batch apply: apply a copied edit stack or a saved preset across a multi-selected batch of images in one action.
- Export dialog and batch export UX: visible per-file/overall progress during a batch export, and a "reveal in file manager" action once export completes (extends M4's existing single-item Reveal-in-File-Manager, currently Library-only, to the export flow).
- Native OS menu bar (File/Edit/View/etc.), wired to existing app actions currently reachable only via in-UI controls.
- History panel preview-on-hover: hovering a history step (or a preset) shows a live preview of that step's result before committing to it; relocate the Presets panel into this same area.
- Resizable left/right Develop panel widths (drag to resize, replacing today's fixed widths).
- Step-nudge (up/down) micro-adjustment controls on Develop slider values, for fine single-step increments beyond drag precision.

### Exit criteria
- A user can copy an edit stack (or a chosen subset) from one image and apply it to a single image or a batch selection.
- Batch export shows real per-file/overall progress and offers a one-click reveal of the output folder when it completes.
- Core app actions (Import, Export, Undo/Redo, etc.) are reachable from a native OS menu bar, not just in-app controls.
- Hovering any history step or preset previews its result without committing to it.
- Develop's left/right panels can be resized by the user and the chosen width persists across sessions.
- Every Develop slider supports fine step-nudge adjustment via up/down controls, not drag-only.

---

## M5 — Performance, GPU, merges, faces
**Rough size:** 3–6 months · **Lightroom analog:** v6.0/CC, 2015

### Scope
- Full GPU-accelerated Develop rendering (building on the M0 spike) with correct, seamless CPU fallback.
- HDR merge (multi-exposure → single high-bit-depth composite).
- Panorama merge (multi-shot → stitched composite), including boundary/edge correction.
- Face detection/recognition for local face-grouping, surfaced in Library mode rather than a separate "People" view (fully local, no cloud model dependency; see RFC-0005 §7, 2026-09-17 redesign).
- Plugin/extensibility API v0 (even a minimal export-plugin hook is useful here and de-risks M8's extensibility work).

### Explicitly deferred
- Any AI masking/selection beyond face detection → M6.

### Exit criteria
- Develop-panel slider response stays under the PRD's ~100ms target on a 50k-image catalog with GPU acceleration active.
- HDR and panorama merges produce output quality comparable to dedicated tools for typical cases.
- Face-grouping is usable for culling/organizing a portrait- or event-heavy catalog.

---

## M5.5 — Map & geolocation
**Rough size:** 1–2 months · **Lightroom analog:** Map module, v4.0, 2012 (closes the "Map/geotagging view" item M4 flagged as optional-pending-demand-signal, its own §"Decide and scope" line, 2026-08-20) — user request (2026-09-18).

### Scope
- Google Maps integration: forward geocoding (search an address or place name → coordinates) to set or move one photo's or a batch's GPS location; reverse geocoding to label a photo's existing coordinates.
- Manual pin-drop / drag-to-adjust on the map as an alternative to text search, on top of M4's existing manual coordinate-entry fields.
- Batch: select multiple photos in Library, assign them all to the same searched/dropped location in one action.
- World map view: a new Library view mode plotting every geolocated photo in the catalog as pins/clusters; clicking a pin/cluster filters the Library grid to just those photos.
- Location edits (search-assigned, pin-dropped, or manually typed) all write the same EXIF GPS fields on export — no divergent write path depending on how the coordinates were set.
- **Prerequisite slice — EXIF/IPTC export writer** (2026-09-19, user request; built before any geo work): export can embed EXIF (camera/exposure/capture time, GPS) and IPTC (caption/copyright/contact/keywords), each chosen per export in the Export dialog. Geo features then only have to populate the catalog's GPS columns; the writer already carries them to the file.
- **Explicit, scoped exception to the "no cloud" non-goal** ([PRD §3](PRD.md#3-non-goals-permanent-not-just-later)): a user-initiated address/place-name search calls an external geocoding API — only the searched text leaves the device, never catalog or photo data, and only when the user actually searches. Documented here rather than silently contradicting the PRD's local-first framing.

### Explicitly deferred
- Any "travel map" storytelling feature (route lines, timeline scrubbing) beyond pin/cluster display.
- Offline map tiles/geocoding — online access for map tiles and address search is an accepted tradeoff for this one feature, not a precedent for reintroducing cloud dependency elsewhere.

### Exit criteria
- A user can search an address or place name and apply the resulting coordinates to one photo or a multi-selected batch.
- The world map view plots every geolocated photo in the catalog, and clicking a pin/cluster filters the Library grid to just those photos.
- Map-search-assigned and manually-entered GPS coordinates round-trip through export as identical EXIF GPS fields.

---

## M5.6 — Develop effects: quality & performance research
**Rough size:** 2–4 months · **Lightroom analog:** n/a — an internal quality-bar-raising pass, not a new user-facing module (closest spirit: Lightroom's own process-version revisions, e.g. PV2012, which reworked existing tone/effect algorithms without adding new panels) — user request (2026-09-18).

### Scope
- A per-effect research pass across every existing Develop op (white balance/auto-tone, tone curve, HSL/color mixer, split toning, dehaze, clarity/texture, sharpening, luminance/color noise reduction, grain, vignette, lens corrections) comparing this project's current algorithm/mapping against published professional-grade references (documented ACR/Lightroom process-version changes, academic image-processing literature, other open-source implementations) to find concrete, specific quality gaps — not a vague "make it better" pass.
- A written improvement plan per effect: the proposed algorithm/mapping change, its expected quality delta, and an explicit CPU+GPU performance budget check against M5's own ~100ms interactive target before any change ships.
- Implementation of whichever improvements the plan prioritizes, each shipped as its own slice with a CPU/GPU parity check (reusing M5 Slice 2's `develop-cpu-gpu-parity.e2e.js` harness) so no effect silently diverges between the two engines.

### Explicitly deferred
- Any AI-model-based effect (denoise, upscale, subject-aware processing) — that's M6; this milestone is scoped to the existing deterministic-algorithm effects only.

### Exit criteria
- Every existing Develop effect has a documented research finding (either "already good, no change" or a concrete planned change) — no silent gaps.
- Every shipped improvement stays under the ~100ms interactive budget and passes the CPU/GPU parity harness.
- At least a sample set of before/after comparisons against a professional reference shows a measurable quality improvement for whichever effects were changed.

---

## M5.7 — AI editing roadmap spike (M6 scoping)
**Rough size:** 1–2 months, research/decision only · **Lightroom analog:** n/a — a scoping spike, mirroring M0's own relationship to M1: produces the on-device-inference architecture decision M6 already names as a prerequisite ("Architecture decision required before scoping in detail," M6 below) — user request (2026-09-18) to "research the next Lightroom-era milestone (AI-assisted editing, dynamic object selection, masking, etc.)" before M6 build work starts.

### Scope
- Survey candidate on-device model families for each M6 feature area — subject/sky/landscape-element segmentation masks, AI denoise, AI super-resolution — plus, looking ahead, generative object removal/fill (M7's own open question): model size, license, accuracy, and minimum hardware (GPU/NPU) needed on both macOS and Windows.
- Resolve M6's own named "architecture decision required before scoping in detail": on-device-only model inference, confirmed feasible (or not) with real numbers on real target hardware, not assumed from a stated preference.
- A short written recommendation per M6 feature area (which model/approach, why, what M6's real implementation scope should be) — so M6 itself starts as a build milestone, not another research pass, the same division of labor M0 → M1 already established for this project.

### Explicitly deferred
- Any implementation — this milestone produces decisions and a written plan only, same discipline as M0.
- A final decision on M7's generative-fill approach — surveyed here for awareness of the landscape, but M7's own exit criteria keep ownership of that specific decision.

### Exit criteria
- M6's "architecture decision required before scoping in detail" line is resolved and documented (e.g., a new ADR), backed by real feasibility numbers.
- A written recommendation exists for each M6 feature area, sufficient for M6 to start as a build milestone without re-doing this research.

---

## M6 — AI-assisted selection & enhancement
**Rough size:** 4–8 months · **Lightroom analog:** Classic v8–v11, 2018–2022

### Scope
- Select Subject / Select Sky (and similar landscape-element masks: water, architecture, vegetation) as one-click AI-generated masks feeding the existing mask/adjustment system from M3.
- Composable masking: add/subtract/intersect multiple masks (AI-generated or manual) in one edit.
- AI-assisted denoise (distinct from and generally higher quality than M1/M3's traditional NR).
- AI super-resolution / upscaling.
- **Architecture decision required before scoping in detail**: on-device model inference only (no cloud calls, consistent with the "local-first, cloud out of scope" constraint) — this determines model size/quality tradeoffs and what hardware (e.g., minimum GPU/NPU) is required. Document the decision before implementation starts.

### Explicitly deferred
- Generative fill/remove → M7 (bigger model, bigger architecture question).

### Exit criteria
- One-click subject/sky selection is accurate enough to be faster than manual brush masking for common compositions.
- AI denoise/upscale run fully offline and complete in a reasonable time on target hardware (define the target machine spec as part of this milestone's kickoff).

---

## M7 — Generative & intelligent culling
**Rough size:** 4–8 months, **explicitly speculative** · **Lightroom analog:** Classic v12+, 2023–2026

### Scope (provisional — confirm feasibility before committing)
- Generative object removal/fill. Given the "no cloud" constraint, this requires either a bundled on-device generative model (quality/size/license tradeoffs, likely the single biggest open technical question in this whole roadmap) or a deliberate decision to ship a **non-generative** content-aware fill instead (extending M4's healing brush rather than true generative fill).
- Adaptive/context-aware auto-tone presets.
- Automatic duplicate/near-duplicate detection for culling.
- AI-assisted culling aids (e.g., blur/focus detection, blink detection) as a "Faces/culling panel."

### Explicitly deferred
- Nothing beyond this — M7 is the top of the AI feature set. Anything past this point (further Adobe Firefly-style features) is out of scope by the PRD's non-goals unless revisited.

### Exit criteria
- A concrete decision is made and documented on the generative-fill approach (bundled model vs. non-generative fallback) *before* implementation, since it changes the milestone's shape substantially.
- Duplicate detection and culling aids measurably reduce time-to-cull on a large event/burst-heavy import.

---

## M8 — Polish, extensibility, 1.0 launch
**Rough size:** 2–4 months · **Lightroom analog:** n/a — this is packaging/hardening, not a feature era

### Scope
- Harden the plugin/extensibility API from M5 into something documented and stable enough for third parties.
- Accessibility pass (keyboard navigation, screen reader labels, contrast).
- Localization scaffolding (even if only one language ships at 1.0).
- Licensing/distribution decision (open source? paid? — not decided in this PRD) and packaging for both target OSes (installers, code signing, auto-update mechanism if any).
- Full documentation: user-facing help, keyboard shortcuts reference.
- Performance/regression pass across the full feature set built in M1–M7.

### Exit criteria
- 1.0 release: installable on both macOS and Windows, passes a full manual regression pass of the M1–M7 feature set, has user-facing documentation.
