# Reference: Adobe Lightroom Feature Timeline (v1 → present)

This document is research background, not a spec. It traces how Lightroom's own feature set grew over ~19 years, grouped into eras. Our milestone plan (see [MILESTONES.md](MILESTONES.md)) uses this sequence as a sanity check on *build order* — Adobe shipped a cataloging + basic RAW tool first and layered advanced/AI features on top over a decade, which is good evidence for what a small team should also defer.

Sources: [Wikipedia — Adobe Lightroom](https://en.wikipedia.org/wiki/Adobe_Lightroom), [Lightroom Queen — 10 Years of Lightroom](https://www.lightroomqueen.com/10-years-lightroom/), [Greater Than Gatsby — Version History](https://www.greaterthangatsby.com/history-of-lightroom/), [Adobe Lightroom Classic release notes](https://helpx.adobe.com/lightroom-classic/help/whats-new/release-notes.html), [Photofocus — AI masking history](https://photofocus.com/software/lightroom-lightroom-classic-get-more-ai-masking-content-aware-remove/).

## Era 1 — Foundation (v1.0–v1.x, 2006–2008)
- Public beta Jan 2006 (Mac only) → Windows beta Jul 2006 → GA **Feb 19, 2007**, $299.
- Five modules: **Library, Develop, Slideshow, Print, Web**.
- Catalog = SQLite database next to the photos; edits stored as **non-destructive** instructions, exportable as **XMP sidecars**. This decoupling of "edit instructions" from "pixels" is the architectural idea the whole product is built on.
- Library: folders (replacing an earlier "Shoots" concept), flags, star ratings, color labels, pick/reject, Survey view, filtering by metadata, **virtual copies**, **stacking**, **snapshots**.
- Develop: RAW processing via Camera Raw engine, white balance, tone controls, HSL, spot removal, red-eye removal, the "TAT" (Targeted Adjustment Tool).
- Broad RAW format support (150+ cameras) from day one — RAW compatibility was never treated as optional.

## Era 2 — Local adjustments & production tools (v2.0–v3.0, 2008–2010)
- **v2.0 (Jul 2008)**: 64-bit support, **graduated filter** and **adjustment brush** (the first local/masked adjustments), multi-monitor support, better print layouts.
- **v3.0 (Oct 2009)**: much better noise reduction (luma/chroma), film grain simulation, sharpening overhaul, point curve, **tethered capture**, **watermarking**, **Publish Services** (sync collections to Flickr/etc.), basic video *file* support (import/organize, no editing).

## Era 3 — New tone engine & output breadth (v4.0–v5.x, 2012–2014)
- **v4.0 (Mar 2012)**: new Process Version with overhauled **Highlights/Shadows/Whites/Blacks** tone model (replacing Recovery/Fill Light/Brightness), **Map module** (GPS/geotagging), **Book module**, **soft proofing**, email export, basic video trimming.
- **v5.0 / "Lightroom CC" branding (Jun 2013)**: advanced **healing/clone brush**, **upright** (automatic perspective correction), radial filter, smart collections, **Smart Previews** (lightweight proxies enabling edits on offline/disconnected originals).
- **2014**: iPad app + mobile sync introduced — the beginning of the cloud/mobile track (explicitly out of scope for us).

## Era 4 — Performance, faces, merges (v6.0 / CC 2015)
- GPU-accelerated Develop pipeline.
- **Face recognition / People view**.
- **HDR merge** and **Panorama merge** (multi-shot composites produced as new DNGs).
- Filter brush, improved lens profile support, Boundary Warp for pano edge correction.

## Era 5 — Cloud split (2017–2019)
- Oct 2017: Lightroom CC (cloud-native) launches; the original desktop app is renamed **Lightroom Classic**.
- 2019: naming settles — "Lightroom Classic" (local catalog, power-user tool) vs. "Lightroom" (cloud-synced, cross-device). **This is the fork point we are deliberately not following** — we are building the Classic-style local-catalog lineage only.

## Era 6 — AI-assisted selection & enhancement (Classic v8–v11, 2018–2022)
- **Range Masking** (color range, luminance range) — 2018.
- **Enhance Details / Raw Details** — AI-improved demosaicing, 2018 onward.
- **Select Subject** and **Select Sky** — AI-driven one-click masks, debuted **Classic 11, Oct 2021**.
- **Super Resolution** — AI 2x upscale, 2021.
- Masking panel rebuilt around composable masks (add/subtract/intersect multiple masks) — Oct 2021.
- **AI Denoise** — 2023.
- Masking extended to landscape-specific subjects: sky, water, mountains, architecture, vegetation, people (by individual), objects.

## Era 7 — Generative & intelligent culling (2023–2026)
- **Generative Remove** (Adobe Firefly-powered content-aware fill for object removal) — 2023, iterated through "Remove Tool 3" with non-generative-credit options.
- **Adaptive Profiles** — context-aware auto starting points (e.g., HDR landscape presets).
- **Point Color**, **Lens Blur**, HDR-native editing (16-bit HDR round-trip with Photoshop) — 2024.
- **Faces panel** for culling, **automatic duplicate detection**, cross-device keyword sync, more precise AI subject selection — 2025–2026 (Classic 15.x).

## What this tells us about build order

| Priority | Category | Lightroom shipped it in |
|---|---|---|
| 1 | Non-destructive catalog + RAW import/develop/export | v1.0 (year 1) |
| 2 | Local/masked adjustments (brush, gradient) | v2.0 (year 2) |
| 3 | Noise reduction, tethering, publish/export polish | v3.0 (year 3) |
| 4 | Modern tone model, geo, books, soft proof | v4.0 (year 6) |
| 5 | Advanced retouching, perspective correction, offline editing | v5.0 (year 7) |
| 6 | GPU pipeline, faces, HDR/pano merge | v6.0 (year 9) |
| 7 | Cloud sync, mobile | 2014–2017 (**out of scope for us**) |
| 8 | AI selection masks, denoise, upscale | 2018–2022 (year 12–16) |
| 9 | Generative fill, adaptive AI, AI culling | 2023–2026 (year 17–20) |

Adobe took roughly a decade to reach "advanced local adjustments + modern tone engine + offline editing," and another decade to add AI. A small team should expect the same *relative* ordering, compressed but not skipped — the catalog/RAW/develop/export core has to be rock solid before masking, and masking has to exist before AI-assisted masking is worth building.
