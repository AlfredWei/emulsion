# User Guide

What Emulsion does today and how to use it — written for someone using the app, not building it. For *why* it's built the way it is, see [README.md](../README.md) and the [ADRs](adr/); for what's planned next, see [PRD/MILESTONES.md](../PRD/MILESTONES.md).

This guide covers what's actually shipped. Where a feature is only partly built or still planned, it says so explicitly rather than describing the eventual goal as if it exists.

## Getting started

Launch the app (`make dev` in development, or the built app once packaged). You'll land in **Library**.

- **Import a folder…** copies (or adds in place — your choice) every supported RAW/JPEG file under a folder into the catalog, generating thumbnails in the background.
- **Import files…** lets you pick specific files instead of a whole folder.
- Duplicate files (by content, not just filename) are detected and skipped automatically.
- A "Detect faces?" prompt appears after import — face detection runs as a visible third phase of the import progress bar if you say yes (see [Faces & People](#faces--people)).

The catalog (a single SQLite file) and your originals stay on your own disk — nothing is uploaded anywhere, ever, except the narrow exceptions noted under [Map & geolocation](#map--geolocation).

## The Library rail

The left-hand rail (Library and People sections both use it) has four navigation sources, exactly one active at a time:

- **All Photos** / **Last Import** — always visible.
- **Folders** — every folder your imported photos actually live in, collapsible via its header.
- **Collections** — manual collections (drag photos in) and Smart Collections (rule-based, auto-updating), collapsible. Use the **+** / **⚡+** buttons to create one.
- **People** — every person face detection has found, collapsible, hidden entirely until at least one exists. See [Faces & People](#faces--people) for what you can do here.

Clicking a Folder or Collection scopes the grid to it; clicking **All Photos** clears any active scope. A **Map selection** entry appears while a map pin or cluster is the scope.

## Right-click menus

Right-click a photo (in the Library grid or the filmstrip, in Library or Develop) for the operations you use most: **Open in Develop / Loupe**, **Rating**, **Flag**, **Colour label**, **Add to Collection** (and **Remove from collection** when you are viewing one), **Develop Settings** (Copy / Paste Settings, Apply Preset, Reset Settings), **Export…**, **Show in Finder**, **Copy Path**, **Detect Faces** and **Remove from Catalog…**. The menu acts on the photo you clicked, or on the whole selection when that photo is part of it (its first row then says "3 photos"); right-clicking a photo outside the selection selects just that one first. Items that need a single photo, or something you have not done yet (Paste Settings before any Copy), are greyed with the reason as a tooltip. Destructive items are red and ask first. Right-click the empty part of the Library grid for Select All / Deselect All / Import. Keyboard: **Shift+F10** (or the Menu key) on a focused photo opens its menu; Up / Down, Home / End, Right / Left for submenus, Enter to choose, Esc to close, or type a letter to jump. Shortcut hints show your own key bindings. Text fields keep the system's Cut / Copy / Paste menu. Other rows have their own menus: right-click a **collection** (Show Photos, Select All Photos in It, Delete…), a **person** in the Catalog rail (Show Photos, Rename…), and in Develop's left rail a **snapshot** (Restore, Delete…), a **preset** (Apply, Export…, Delete…) or the empty part of the Presets list (Save Current as Preset…, Import…). More menus (the Develop canvas, History, masks, panel headers) and multi-select in Develop are planned (RFC-0028).

## Library module

Five view modes (toolbar; `G`/`E`/`C`/`N` for the first four):

| View | What it's for |
|---|---|
| **Grid** | The main thumbnail grid — virtualized, so it stays fast at tens of thousands of images. |
| **Loupe** | Single-image, full-resolution, interactive pan/zoom. Face rectangles (with the person's name) can be toggled on here. |
| **Compare** | Synchronized side-by-side comparison with candidate navigation — for picking a winner between similar shots. |
| **Survey** | A responsive multi-photo matrix for reviewing a larger set at once. |
| **Map** | A world map of the photos that have a location — see [Map & geolocation](#map--geolocation). |

**Culling**: flags (Pick/Reject/Unflag), 1–5 star ratings, and 6 color labels — all click-driven from the per-cell badge row or fully keyboard-driven (see [Shortcuts](#keyboard-shortcuts)). Multi-select (click + Shift/Cmd, or "Select All") applies any of these to many photos at once, with a selection-count badge.

**Filtering**: the filter bar combines flag, star rating (`>=` or `=`), color label, file type (RAW/JPEG), camera, lens, date-taken range, and a free-text search (filename, camera, lens) — all combinable at once, scoped to whatever the rail currently has selected.

**Organizing**: hierarchical keywording, manual collections, rule-based Smart Collections, virtual copies and stacking (grouping burst shots or edit variants without duplicating files), and a full EXIF (read-only) + IPTC (editable: caption, copyright, contact) metadata panel. GPS coordinates from EXIF are shown and manually editable — see [Map & geolocation](#map--geolocation) for place search, the map, and what leaves your computer.

**Other actions**: drag-and-drop import, "Reveal in File Manager," non-destructive "Remove from Catalog" (never touches the file on disk), batch export, and one-click batch HDR merge / panorama merge for a multi-selected bracket or shot sequence.

## Develop module

Open any photo into Develop (`D`, or double-click). Every edit is non-destructive — stored as structured instruction data, never baked into pixels — with full History/Undo, named Snapshots, and Before/After compare (`\`).

- **Global tone**: White Balance (+ eyedropper, + one-click Auto WB), Exposure, Contrast, Highlights/Shadows/Whites/Blacks, one-click Auto Tone, Vibrance/Saturation, Tone Curve, HSL/Color Mixer, Split Toning, Dehaze, Clarity/Texture, Vignette, Grain, camera/creative color profiles.
- **Local adjustments**: linear gradient, radial gradient, and adjustment brush (with auto-mask) — all composable, plus color-range and luminance-range masking.
- **Select Subject (click to select an object)**: the *Select Subject* tool in the tool strip selects an object with a click. **Click** the object (or **Alt/Option-click** a part that should *not* be included) and the red overlay shows the selection; each further click refines it. The model offers up to three candidates (e.g. the whole person vs. the shirt): switch with the **1 / 2 / 3** buttons or **Tab**. **Enter** (or *Done*) keeps the selection as a mask, **Esc** (or *Cancel*) throws it away. The mask's panel has **Feather** and **Grow** (move the edge outwards/inwards), and *Add shape ▾ ▸ Subject* uses the same tool to add or subtract an object from another mask. The first time you use it, Emulsion asks to download the selection model (about 155 MB, once, from the host named in the dialog; nothing about your photos is sent) or lets you pick the two model files you already have; after that selection runs entirely on this computer, in a helper process that is started on first use and exits when idle. Selections are soft-edged by design and work best on objects; for the sky use *Luminance Range* or *Color Range*. To adjust a saved selection later, open its panel and use **Refine with clicks…** (Esc then puts the saved selection back). A saved Subject mask needs no model to display or export. A download can be cancelled from the dialog.
- **AI Denoise** (Noise Reduction panel): removes noise with a neural model, for high-ISO photos where Luminance and Color Noise Reduction are not enough. It is a *job you start*, never a live slider: **Preview on a crop** denoises a rectangle in the middle of the photo in a few seconds and shows it before / after (a divider you drag; **1:1** shows real pixels, **Fit** the whole rectangle); **Denoise whole photo** runs the model over the whole photo on this computer (about 40 seconds for a 10-megapixel photo on an M1 Pro laptop, so roughly twice that for 24 megapixels; slower machines take longer), shows its progress, and can be cancelled; you can keep editing meanwhile, and open other photos (the job carries on and says so in the status line when it is done). When it finishes, **Amount** is set to 70 and the preview shows the result; **Amount** then mixes the denoised copy with the original (0 = original, 100 = fully denoised) and moving it is instant. Full strength can look waxy on some photos, so judge it by eye. The denoised copy is kept on this computer (in the cache folder chosen under Settings → Storage) until you choose *Remove denoised copy*, or until the copies of all photos together pass about 2 GB, when the oldest are removed (denoise that photo again to get one back); it is used first in the pipeline, before everything else, and combines with Luminance and Color below. Hiding the Noise Reduction panel switches it off, and Reset removes the amount. Export and Print apply the kept copy, and neither ever runs the model by itself: a photo whose Amount is above 0 but that was never denoised exports without the effect, and the Export dialog says which photos those are before you click Export, with a **Denoise these first** button that runs the model on them one after another (progress shown, **Stop** available; it needs the model installed already, which Develop's *Denoise whole photo* does the first time). The first time you use it Emulsion asks to download the model (about 117 MB, once, from the host named in the dialog; nothing about your photos is sent) or lets you pick the file you already have; a download that is interrupted continues where it stopped. Not copied by presets or *Copy Settings*, since the copy belongs to one photo. The Loupe view, soft proof, History / Snapshot / preset hover previews and Develop without WebGPU show it too (they refresh when they are rebuilt, e.g. reopening the Loupe); the Library grid's thumbnail is rebuilt with it when you leave Develop.
- **Combining shapes in one mask**: a linear, radial, brush, luminance-range or color-range mask has a **Shapes** list in its panel. *Add shape* ▾ lets you **Add**, **Subtract** or **Intersect** another shape (place it on the image, or paint it for a brush); the red overlay shows the combined selection, and the mask's Exposure/Contrast/Saturation apply to it. Click a row to edit that shape; change its combine mode or remove it from its row. Up to 16 extra shapes per image (brush shapes share the 8 brush layers with brush and spot masks). Reordering and a per-shape on/off switch are not available yet.
- **Geometry**: Crop, straighten, rotate/flip, manual perspective/upright correction, profile-based lens corrections (distortion, vignette, chromatic aberration).
- **Retouching**: healing/clone brush, spot removal, red-eye removal.
- **Sharpening & noise reduction**: separate luminance/color noise reduction, sharpening.
- **Presets**: create, save, apply, import/export as files; Copy/Paste Settings and Batch Apply let you carry one photo's edits (or a chosen subset) onto another photo or a whole selection.
- **Soft proofing** against an output color profile before you export or print.
- **Zoom & navigate**: **Fit** shows the whole photo; click the photo to jump to 100% (click again for Fit), or use the **− Fit 100% +** controls in the bottom-right corner to step through 50 / 75 / 100 / 150 / 200%. `Ctrl`/`Cmd` + scroll wheel or a trackpad pinch zooms about the cursor, clamped to 50–200%. Percentages are of the photo's native size. While zoomed in, a small **navigator** above the controls shows the whole photo with a frame around the part you're looking at; click or drag in it to move there. Drag the photo (or hold `Space`) to pan.
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

## Map & geolocation

Part of [M5.5 in the roadmap](../PRD/MILESTONES.md). **Built:** searching for a place or address and applying it as the GPS location of the selected photo(s), in one action for a whole multi-selection; the **Map** view and placing photos on it; and looking up a place name for a photo's existing coordinates (below). **Not built yet:** nothing — M5.5 is complete.

**Choosing a search service (Settings → Map):** *OpenStreetMap* is the default and needs no setup or account; it's good with addresses and limited to about one search per second. *Google* is better at landmark and business names but needs your own API key — create one in Google Cloud Console and enable the Geocoding API (Google requires a billing account, though light personal use is within its free monthly allowance). The key stays in your local catalog and is never shown again in the app.

**Using it:** select one or more photos, and in the metadata panel's Location section type a place name or address and press Search. Pick a result to apply its coordinates to every selected photo (the list says how many). Existing altitude is kept. The location is stored in your catalog and, if you tick EXIF + GPS, written into exported JPEGs. You can still type coordinates by hand, for one photo.

**The Map view:** click **Map** in the Library toolbar. Every photo with a location in the current source (All Photos, a folder, a collection…) and passing the filter bar appears as a pin, showing a thumbnail of that photo once one exists (generated on the spot if it doesn't yet, the same as opening it in Grid would); nearby pins merge into a cluster shown as a small fanned stack of up to three of its own photos, with its count badged in the corner, that splits apart as you zoom in. Click a pin or cluster to scope the grid to those photos: the rail shows a **Map selection** entry (click it, or All Photos, to clear it) and the view returns to Grid. Photos without a location are not on the map; a badge says how many were left out. Opening the map again shows the whole current source, not the last selection. `M` opens it from the keyboard (rebindable in Settings).

**Placing photos on the map:** drag a photo from the filmstrip (still visible under the Map view) onto a point on the map — or, if it's part of your current selection, drag any selected photo to bring the whole selection. A pin follows your pointer while you drag, showing exactly where it will land; letting go there saves that location immediately, for every photo you dragged. Applying writes to your catalog only, like the search-and-assign flow above, and overwrites any location those photos already had.

**Looking up a place name for a photo's coordinates:** select a single photo that already has a location and press **Look up place name** next to its coordinates in the metadata panel. This is the one action in the app that sends catalog data — that photo's own coordinates — to the search service; it only happens when you press the button, never automatically. The result is shown, not saved; press it again for another try.

**What leaves your computer:** the text you type into the search box (plus your key, if you chose Google), and only when you press Search; search results aren't saved, only the location you pick is. The Map view also downloads map tiles from OpenStreetMap's public server, only while the Map view is open; the requests reveal which part of the world you are looking at, never which photos you have or where they are (a placed pin is saved locally and is not sent anywhere). **Look up place name** is the exception described above: it sends the one photo's coordinates, only on request. The map credits © OpenStreetMap contributors, as their tile policy requires; that link opens in your browser.

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
| `M` | Map view |
| `D` | Develop module |
| `Space` | Toggle Grid/Loupe (or fit zoom) |
| `0`–`5` | Set star rating (0 clears it) |
| `P` | Pick flag (toggle) |
| `X` | Reject flag (toggle) |
| `U` | Unflag |
| `6` / `7` / `8` / `9` | Red / Yellow / Green / Blue color label |
| `\` | Before/After toggle (Develop) |
| `Z` | Zoom Fit / 100% (Develop) |
| `Cmd/Ctrl` + `+` / `-` | Zoom in / out through 50–200% (Develop) |
| `Cmd/Ctrl` + `0` / `1` | Zoom to Fit / 100% (Develop) |
| `O` | Mask overlay toggle (Develop) |
| `H` | Hide/show mask pins (Develop) |

## What's not built yet

This guide only documents shipped features. For what's planned next — map/geolocation, a Develop effect quality/performance pass, and the AI-assisted selection/masking/enhancement work beyond face detection — see [PRD/MILESTONES.md](../PRD/MILESTONES.md). For permanent non-goals (cloud sync, mobile app, any video support), see [PRD/PRD.md §3](../PRD/PRD.md#3-non-goals-permanent-not-just-later).
