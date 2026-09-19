# User Guide

What Emulsion does today and how to use it — written for someone using the app, not building it. For *why* it's built the way it is, see [README.md](../README.md) and the [ADRs](adr/); for what's planned next, see [PRD/MILESTONES.md](../PRD/MILESTONES.md).

This guide covers what's actually shipped. Where a feature is only partly built or still planned, it says so explicitly rather than describing the eventual goal as if it exists.

## Getting started

Launch the app (`make dev` in development, or the built app once packaged). You'll land in **Library**.

- **Import a folder…** copies (or adds in place — your choice) every supported RAW/JPEG file under a folder into the catalog, generating thumbnails in the background.
- **Import files…** lets you pick specific files instead of a whole folder.
- Duplicate files (by content, not just filename) are detected and skipped automatically.
- A "Detect faces?" prompt appears after import — face detection runs as a visible third phase of the import progress bar if you say yes (see [Faces & People](#faces--people)).

The catalog (a single SQLite file) and your originals stay on your own disk — nothing is uploaded anywhere, ever, except the one narrow exception noted under [Map & geolocation](#map--geolocation-planned).

## The Library rail

The left-hand rail (Library and People sections both use it) has four navigation sources, exactly one active at a time:

- **All Photos** / **Last Import** — always visible.
- **Folders** — every folder your imported photos actually live in, collapsible via its header.
- **Collections** — manual collections (drag photos in) and Smart Collections (rule-based, auto-updating), collapsible. Use the **+** / **⚡+** buttons to create one.
- **People** — every person face detection has found, collapsible, hidden entirely until at least one exists. See [Faces & People](#faces--people) for what you can do here.

Clicking a Folder or Collection scopes the grid to it; clicking **All Photos** clears any active scope.

## Library module

Four view modes (toolbar or `G`/`E`/`C`/`N`):

| View | What it's for |
|---|---|
| **Grid** | The main thumbnail grid — virtualized, so it stays fast at tens of thousands of images. |
| **Loupe** | Single-image, full-resolution, interactive pan/zoom. Face rectangles (with the person's name) can be toggled on here. |
| **Compare** | Synchronized side-by-side comparison with candidate navigation — for picking a winner between similar shots. |
| **Survey** | A responsive multi-photo matrix for reviewing a larger set at once. |

**Culling**: flags (Pick/Reject/Unflag), 1–5 star ratings, and 6 color labels — all click-driven from the per-cell badge row or fully keyboard-driven (see [Shortcuts](#keyboard-shortcuts)). Multi-select (click + Shift/Cmd, or "Select All") applies any of these to many photos at once, with a selection-count badge.

**Filtering**: the filter bar combines flag, star rating (`>=` or `=`), color label, file type (RAW/JPEG), camera, lens, date-taken range, and a free-text search (filename, camera, lens) — all combinable at once, scoped to whatever the rail currently has selected.

**Organizing**: hierarchical keywording, manual collections, rule-based Smart Collections, virtual copies and stacking (grouping burst shots or edit variants without duplicating files), and a full EXIF (read-only) + IPTC (editable: caption, copyright, contact) metadata panel. GPS coordinates from EXIF are shown and manually editable — see [Map & geolocation](#map--geolocation-planned) for what's *not* built yet here.

**Other actions**: drag-and-drop import, "Reveal in File Manager," non-destructive "Remove from Catalog" (never touches the file on disk), batch export, and one-click batch HDR merge / panorama merge for a multi-selected bracket or shot sequence.

## Develop module

Open any photo into Develop (`D`, or double-click). Every edit is non-destructive — stored as structured instruction data, never baked into pixels — with full History/Undo, named Snapshots, and Before/After compare (`\`).

- **Global tone**: White Balance (+ eyedropper, + one-click Auto WB), Exposure, Contrast, Highlights/Shadows/Whites/Blacks, one-click Auto Tone, Vibrance/Saturation, Tone Curve, HSL/Color Mixer, Split Toning, Dehaze, Clarity/Texture, Vignette, Grain, camera/creative color profiles.
- **Local adjustments**: linear gradient, radial gradient, and adjustment brush (with auto-mask) — all composable, plus color-range and luminance-range masking.
- **Geometry**: Crop, straighten, rotate/flip, manual perspective/upright correction, profile-based lens corrections (distortion, vignette, chromatic aberration).
- **Retouching**: healing/clone brush, spot removal, red-eye removal.
- **Sharpening & noise reduction**: separate luminance/color noise reduction, sharpening.
- **Presets**: create, save, apply, import/export as files; Copy/Paste Settings and Batch Apply let you carry one photo's edits (or a chosen subset) onto another photo or a whole selection.
- **Soft proofing** against an output color profile before you export or print.
- **Panels resize** (drag the History/adjustments panel edges) and every slider supports fine step-nudge via up/down controls in addition to drag.

**Rendering**: Develop renders on the GPU (WebGPU) for interactive speed. If WebGPU isn't available on your machine, the app automatically falls back to a CPU-rendered preview — mask/crop *creation* is disabled in that mode (existing masks still render correctly), and a banner tells you which mode you're in.

## Print module

Layout templates, page setup, and printer color management, plus a direct "Export as PDF" / contact-sheet path independent of your OS print dialog.

## Export

JPEG export with an optional long-edge resize and a quality setting; batch export shows per-file progress and can reveal the destination folder in your file manager when it finishes.

**Metadata**: the Export dialog lets you choose what gets embedded in each exported JPEG. *Write EXIF* embeds camera/lens, exposure settings, and capture time; *Include GPS location* (a sub-option of EXIF) adds the photo's coordinates — untick it to strip location before sharing. *Write IPTC* embeds caption, copyright, contact, and keywords (UTF-8; fields longer than IPTC's limits are trimmed). Both EXIF and IPTC are on by default; unticking both gives a bare JPEG. If embedding fails for a file, the export still succeeds and the dialog notes that metadata wasn't written. XMP is not written.

**Export plugins**: configure external tools (Settings → Export Plugins) to run automatically after a successful export — useful for handing a finished file to another program. This is v0 of the plugin API: fire-and-forget, no shell, no sandboxing — see [ADR-0008](adr/ADR-0008-plugin-extensibility-api-v0.md) for exactly what that means and why.

## HDR & panorama merge

Select 2+ photos in Library and use the toolbar's **HDR** or **PAN** action:

- **HDR merge** combines a multi-exposure bracket into a single high-bit-depth composite with real radiometric merging and automatic alignment.
- **Panorama merge** stitches multiple overlapping shots into one image via feature-based homography, including boundary/edge correction.

Both are hand-rolled (no external computer-vision library) and the merged result lands back in your Library like any other photo, ready for Develop.

## Faces & People

Face detection, embedding, and clustering all run **fully locally** — no cloud model calls, ever ([ADR-0007](adr/ADR-0007-face-detection-and-recognition.md)).

- **Detecting faces**: say yes to the import-time prompt, or use Library's **Face** button (one photo), **Detect Faces** (multi-select), or **Detect Faces in Folder** (everything in the current rail scope that hasn't been scanned yet).
- **Per-photo tagging**: select a photo, open its **People** section in the metadata panel (same place as Keywords/IPTC) to see, tag, rename, reassign, or exclude ("not a face") each detected face. Turn on **Show Faces** in Loupe to see the rectangles (with names) directly over the photo.
- **Browsing by person**: the rail's **People** section lists everyone detected, with a cropped avatar and photo count.
  - **Single-click** a person's name to rename them inline (Enter to save, Escape to cancel).
  - **Double-click** anywhere else on their row to filter the Library grid to just their photos.
- There's currently no way to merge two people who turn out to be the same person, or to browse people across the whole catalog independent of the currently-scoped folder/collection — known gaps, not oversights (see [PROGRESS.md](../PROGRESS.md) and RFC-0005 §7 for the full accounting).

## Map & geolocation (planned)

Not built yet — [M5.5 in the roadmap](../PRD/MILESTONES.md). Once shipped: search an address or place name to set a photo's (or a batch's) GPS location, drop/drag a pin as an alternative to typing, and a world map Library view plotting every geolocated photo, click-to-filter. Today you can only view/edit GPS coordinates that already exist in a photo's EXIF, or type coordinates in manually — no map UI, no address search.

## Settings

App-wide preferences (Settings, gear icon): catalog backup (frequency, folder, integrity check), storage location for the thumbnail/preview cache, keyboard shortcut rebinding, and export plugin configuration.

## Keyboard shortcuts

All rebindable in Settings → Shortcuts; these are the defaults.

| Key | Action |
|---|---|
| `←` / `→` | Previous / next photo |
| `↑` / `↓` | Step up / down in the grid |
| `G` | Grid view |
| `E` | Loupe / single view |
| `C` | Compare view |
| `N` | Survey view |
| `D` | Develop module |
| `Space` | Toggle Grid/Loupe (or fit zoom) |
| `0`–`5` | Set star rating (0 clears it) |
| `P` | Pick flag (toggle) |
| `X` | Reject flag (toggle) |
| `U` | Unflag |
| `6` / `7` / `8` / `9` | Red / Yellow / Green / Blue color label |
| `\` | Before/After toggle (Develop) |
| `O` | Mask overlay toggle (Develop) |
| `H` | Hide/show mask pins (Develop) |

## What's not built yet

This guide only documents shipped features. For what's planned next — map/geolocation, a Develop effect quality/performance pass, and the AI-assisted selection/masking/enhancement work beyond face detection — see [PRD/MILESTONES.md](../PRD/MILESTONES.md). For permanent non-goals (cloud sync, mobile app, any video support), see [PRD/PRD.md §3](../PRD/PRD.md#3-non-goals-permanent-not-just-later).
