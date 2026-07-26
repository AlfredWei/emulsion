# PRD — [Working Title] Photo Library & RAW Editor
### A local-first Lightroom Classic replacement

Status: Draft v0.1 · Owner: TBD · Last updated: 2026-07-25

---

## 1. Vision

A desktop application for photographers to **import, organize, non-destructively edit, and export** their photo libraries — including camera RAW files — without depending on a subscription or a cloud account. It should feel like the tool Lightroom Classic used to be before every workflow assumed a Creative Cloud login: fast, local, and owned by the person using it.

The catalog and the pixels are always on the user's own disk. There is no server component in this product's roadmap.

## 2. Goals

- Replace Lightroom Classic's **core workflow** — import, cull, organize, develop, export — for a single photographer working on one machine.
- Be **non-destructive** end-to-end: originals are never modified; every edit is reversible instruction data.
- Support the **breadth of camera RAW formats** photographers actually use, via a proven decoding library rather than reinventing RAW decode.
- Be **fast** at library scale (tens of thousands of images) and **fast** per-image in Develop (sub-100ms adjustment feedback on modern hardware).
- Ship as a **cross-platform desktop app** (macOS + Windows) from a single codebase.

## 3. Non-goals (permanent, not just "later")

- No cloud sync, no hosted catalog, no account system.
- No mobile companion app.
- No social/sharing features (no built-in publish-to-web-service).
- No video editing beyond basic trim/organize (this is a *photo* tool).
- Not attempting pixel-perfect algorithmic parity with Adobe's proprietary demosaic/color science — "very good," not "bit-identical to Lightroom."

See [MILESTONES.md](MILESTONES.md) for what's deferred *within* scope vs. excluded permanently.

## 4. Target user

A single photographer (hobbyist through semi-pro) who:
- Shoots RAW on a dedicated camera (not just phone), possibly tens of thousands of images across years.
- Wants full local control of their library — no subscription, no forced cloud storage.
- Needs a real culling and organizing workflow (ratings, flags, keywords, collections), not just an editor.
- Cares about color-accurate, non-destructive RAW development and exporting finished files (JPEG/TIFF) for delivery, print, or web.

Out of scope as a persona: teams needing shared/collaborative catalogs, mobile-first shooters, videographers.

## 5. Assumptions & constraints

These were confirmed with the project owner and drive every downstream decision in this PRD:

| Decision | Choice | Implication |
|---|---|---|
| Platform | Cross-platform desktop (macOS + Windows), single codebase | Framework must support native-feeling UI + GPU access on both OSes (e.g., a native-ish cross-platform UI toolkit paired with a Rust/C++ core, or Qt/C++). Final stack choice is a **M0 spike deliverable**, not decided here. |
| Cloud / mobile | Permanently out of scope | Catalog format and sync model never need to account for multi-device conflict resolution. Simplifies the whole architecture. |
| RAW decoding | Use an existing open-source library (e.g., LibRaw) rather than build a proprietary decoder | De-risks the single hardest technical problem (hundreds of camera sensor formats); product effort goes into library/edit/export UX and our own color/tone pipeline built *on top of* decoded RAW data. |
| Team & timeline | Solo/small indie team, long-term project, no fixed deadline | Milestones are sized in relative order and rough month-ranges, not sprints. Scope within a milestone can be trimmed to hit a working release rather than slipping the whole roadmap. |

## 6. Competitive reference

[lightroom-reference.md](lightroom-reference.md) traces Lightroom's actual feature rollout from v1 (2007) to today, grouped into eras. The short version: cataloging + non-destructive RAW develop + export shipped first and alone; local adjustments came a year later; the modern tone engine and offline editing took until year 6–7; GPU/faces/merges around year 9; AI-assisted masking and enhancement didn't arrive until years 12–16; generative AI features are only the last few years. Our milestone order (Section 8 and [MILESTONES.md](MILESTONES.md)) follows that same relative sequence.

## 7. Product scope by module

### 7.1 Catalog / Library engine (foundation, not user-facing on its own)
- Single local catalog file (embedded DB, e.g. SQLite) storing: image references, metadata cache, edit history, collections, keywords — mirrors Lightroom's catalog-next-to-photos model.
- Catalog never stores pixel data for originals — only references to files on disk (+ generated previews/thumbnails/smart-preview proxies).
- All edits stored as structured, versioned instruction data (not baked pixels), exportable as XMP sidecars for interoperability with other tools.
- Catalog must survive: moved/renamed folders (relink workflow), missing/offline volumes, corruption (backup/repair path).

### 7.2 Import
- Import from a folder, memory card, or attached camera volume.
- Copy vs. add-in-place; user-defined destination folder structure/naming templates (date-based, custom tokens).
- Duplicate detection at import time.
- Apply metadata (copyright, keywords) and a develop preset at import.
- Background thumbnail/preview generation that doesn't block the UI — two distinct tiers, both generated ahead of need rather than on demand: (1) a small Library-grid **thumbnail**, and (2) a **Develop preview cache**, a resized/demosaiced proxy per image that Develop reads from instead of decoding the source RAW fresh on every open. Mirrors Lightroom's own Standard/1:1 Preview cache — this is what makes Develop feel instant on a large catalog; see §7.6.
- Broad RAW format support via the chosen decode library, plus standard JPEG/TIFF/PNG, plus common HEIC.

### 7.3 Library / organization
- Grid (filmstrip/lightbox) and single-image (loupe) views.
- Flags (pick/reject), 1–5 star ratings, color labels — the core culling toolkit.
- Filtering and sorting by any combination of the above plus metadata (camera, lens, date, focal length, ISO, etc.) and keywords.
- Keywording with hierarchical keyword sets.
- Collections (manual) and Smart Collections (rule-based).
- Virtual copies and stacking (e.g., grouping burst shots or edit variants without duplicating files).
- Metadata panel: EXIF (read-only) + IPTC (editable: caption, copyright, contact).
- Search across filename, keyword, and metadata.

### 7.4 Develop (edit)
- Non-destructive, fully reversible edit stack per image (+ history panel, snapshots, before/after compare).
- Global adjustments: white balance, exposure, contrast, highlights/shadows/whites/blacks (modern tone model, not the old Recovery/Fill Light model), clarity/texture, dehaze, vibrance/saturation, tone curve, HSL/color mixer, split toning / color grading, sharpening, luminance & color noise reduction, grain, vignette, camera/creative color profiles.
- Local adjustments: linear gradient, radial gradient, adjustment brush (with auto-mask), all producing composable masks.
- Masking: range masking (color range, luminance range) as a same-milestone companion to brush/gradient — this is core Develop, not an "AI extra."
- Lens corrections (profile-based distortion/vignette correction, chromatic aberration removal), manual perspective/upright correction.
- Crop, straighten, rotate/flip.
- Spot removal / healing / clone.
- Retouch tools: red-eye removal.
- Presets: create, save, apply, import/export user presets; sync-free (local preset library).
- Virtual-copy-aware: edits are per-copy.
- Soft proofing against output color profiles before export/print.

### 7.5 Export
- Export to JPEG/TIFF/PNG/DNG with full control: file naming, resize (dimensions/long-edge/megapixels), resolution, output sharpening (by output type: screen/print/glossy/matte), color space (sRGB/Adobe RGB/ProPhoto), quality/compression, metadata stripping options, watermarking.
- Export presets (save/reuse settings).
- Batch export with progress + background processing that doesn't block Library/Develop use.
- Print module: layout templates, page setup, printer color management, soft-proofed output.

### 7.6 Performance & data integrity (cross-cutting, not a "feature" but a requirement)
- GPU-accelerated Develop rendering where available, with a correct CPU fallback.
- Responsive UI at 50k+ image catalogs: virtualized grid, background indexing, cached previews.
- **Persistent Develop preview cache is a first-class requirement, not an incidental optimization**: the demosaiced proxy that seeds the Develop canvas is generated once (in the background at/after import, or lazily on first open) and persisted on disk, then reused on every later Develop entry for that image — never re-decoded from the source RAW file just to open the editor. Invalidated and regenerated only when the source file itself changes (moved/re-imported/replaced), not on every edit-stack update. This is the single biggest lever on perceived Develop-open latency.
- **Catalog backup**, modeled directly on Lightroom's own long-standing behavior: prompt on app close with a user-configurable frequency (every time / once a day / once a week / once a month / never), write a timestamped copy of the catalog file to a user-chosen backup location (kept separate from the working catalog — a different folder or drive), with an optional integrity check before backing up and an optional catalog optimization (vacuum/compact) as part of the backup step. Backs up the catalog file only, not the photos — originals are assumed to have their own backup story, since this product never touches them (§7.1).
- Crash-safe edit persistence (no data loss on crash mid-edit).

## 8. Milestone summary

Full detail, exit criteria, and Lightroom-era mapping in [MILESTONES.md](MILESTONES.md). Summary:

| # | Milestone | Lightroom-era analog |
|---|---|---|
| M0 | Foundations & tech spike | — (pre-v1 engineering) |
| M1 | MVP: Import → Library → Basic Develop → Export | v1.0 (2007) |
| M2 | Photo management & catalog depth | — (deliberately resequenced ahead of Lightroom's own chronology; see MILESTONES.md) |
| M3 | Local adjustments & non-destructive toolkit | v2.0–v3.0 (2008–2010) |
| M4 | Modern tone engine, retouching, output modules | v4.0–v5.0 (2012–2013) |
| M5 | Performance, GPU, merges, faces | v6.0/CC (2015) |
| M6 | AI-assisted selection & enhancement | Classic v8–v11 (2018–2022) |
| M7 | Generative & intelligent culling | Classic v12+ (2023–2026) |
| M8 | Polish, extensibility, 1.0 launch | — |

## 9. Non-functional requirements

- **Color accuracy**: color-managed pipeline throughout (working space → display profile → output profile); soft proofing must be trustworthy.
- **Data safety**: never modify originals; catalog corruption must be recoverable from backup; every destructive-feeling action (delete from disk, overwrite) requires explicit confirmation.
- **Performance targets**: import 1,000 RAW files without UI freeze; Develop slider feedback ≤100ms on a 5-year-old mid-range machine; catalog with 50k images opens in a few seconds.
- **Portability**: catalogs and presets are portable between the macOS and Windows builds without conversion.
- **Offline**: fully functional with no network connection, always.

## 10. Success metrics

- A user can go from "insert memory card" to "exported, delivery-ready JPEGs" entirely within the app, for a real shoot, without needing another tool.
- Catalog and edit data survive a full app reinstall (pointed at the same catalog file) with zero edit loss.
- Time-to-first-edit-visible on a newly imported RAW file is comparable to or faster than Lightroom Classic on equivalent hardware.

## 11. Key risks

- **RAW library coverage/quality**: even a mature library like LibRaw may lag on brand-new camera models or have color-science gaps vs. Adobe's proprietary profiles — mitigate with camera-profile support and a documented "supported cameras" list per release.
- **Scope creep toward Lightroom feature parity**: Lightroom is a 19-year, large-team product (see [lightroom-reference.md](lightroom-reference.md)). This PRD deliberately cuts cloud/mobile/video-editing/social to keep scope achievable for a small team — resist re-adding them.
- **Cross-platform GPU pipeline**: performant, color-correct GPU rendering that behaves identically on macOS (Metal) and Windows (Direct3D/Vulkan) is genuinely hard; M0's spike must de-risk this before M1 commits to an architecture.
- **Generative AI features (M7)**: on-device generative fill/remove may require a bundled model or a decision to omit this permanently given "no cloud" constraint — flagged explicitly in M7, decision not pre-made here.
