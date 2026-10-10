# RFC-0028: Context menus, and multi-select in Develop with Auto Sync

- Status: Draft for review
- Date: 2026-10-09
- Relates to: [UX-DESIGN](../ux/UX-DESIGN.md) (§2 information architecture, §4 Develop), [RFC-0009](RFC-0009-page-svelte-state-design.md) (stores / actions split)

## 0. Process note

Design only. **No product code is changed.** The UX was designed first, as an interactive mock (§3.7), at the user's request. Decided with the user on 2026-10-09: the menu is an **in-app component** (not native OS menus), and Develop multi-select includes a **Lightroom-style Auto Sync toggle**, not just a Sync button.

## 1. Problem

1. There is **no context menu anywhere**. Every operation is reachable only through toolbar buttons, the native menu bar, hotkeys or a side panel, and the webview's default right-click menu (Reload, Inspect...) is what a user gets today. Photo apps are right-click driven: the commonly used operations should be one right-click away on the thing the user is pointing at, and *different things should offer different menus*.
2. Develop's filmstrip is single-select: clicking a photo opens it, and `selectedIds` is forced to just the open photo. Lightroom lets you select several photos in Develop to Sync Settings, Auto Sync an edit across them, copy / paste, rate, flag and export in one go.

## 2. Facts this design rests on (from the code)

- **Operations already exist as plain actions** (`lib/actions/*.js`): rating / flag / colour label (`metadataActions`: `handleRatingChange(versionId, n)` etc.), collections (`collectionsActions`), presets and Copy / Paste Settings (`presetActions`), remove from catalog (`libraryActions.handleRemoveConfirmed`), export (`handleExportClick`), reveal in file manager (`api/system.revealInFileManager`), history / snapshots (`developActions`), faces (`faceActions`). A menu item should call one of these, never new logic.
- **Target resolution already follows Lightroom**: `targetVersionIds(versionId)` in `selectionActions.js` acts on the whole selection when the clicked cell is part of a multi-selection, otherwise on that cell alone. The menu reuses it unchanged.
- **Selection** is `selection.selectedIds` (a `Set`, replaced immutably) plus `selection.selectedId` (the anchor). `handleSelect(versionId, event)` already implements click / Shift-range / Cmd-Ctrl-toggle for the Library.
- **Develop's filmstrip** (`+page.svelte`) passes `selectedIds = new Set([develop.versionId])` and `onSelect = openDevelop`, so it can never show more than one selected photo.
- **Settings can be applied to other photos** by `getEditStack -> applyPresetOps -> setEditStack -> regenerateThumbnail` per target (`handleApplyPresetToSelection`, `handlePasteSettingsToSelection`), non-atomic, one IPC round trip per call, with a re-sync of the open photo's in-memory stack if it was a target. Copy Settings already has *groups* (`CopySettingsDialog`), and some ops are deliberately excluded from copy / presets (per-photo ones, e.g. `ai_denoise`).
- **Edits to the open photo** are written by `develop.scheduleFlush(label)` (debounced) / `flushEditStack(label)`; history entries are per photo.

## 3. Decisions

### 3.1 One menu component, many item builders

- `ContextMenu.svelte`, mounted once in `+page.svelte`, driven by a `contextMenu` store (`open({x, y, items, returnFocusTo})`, `close()`). Items are plain data:
  `{ id, label, shortcut?, disabled?, checked?, danger?, children?: Item[], onSelect? } | { separator: true }`.
- **Builders are pure functions** `(target, state) -> Item[]` in `lib/contextMenus/*.js` (one file per surface), so what a menu offers for a given selection is unit-testable without a DOM. The component never knows what a "photo" is.
- Behaviour: opens at the pointer, **clamped to the viewport** (flips left / up near an edge), submenus fly out on hover or Right-arrow, **keyboard**: Up / Down / Home / End, Right / Left for submenus, Enter / Space to activate, Esc closes and returns focus, first letter jumps; `role="menu"` / `menuitem` / `menuitemradio` with `aria-checked`, `aria-disabled`; closes on outside click, scroll, resize, window blur, Esc, or another right-click. The keyboard route to open it is **Shift+F10 / the Menu key** on a focused cell.
- The app suppresses the webview's own menu (`contextmenu` -> `preventDefault`) everywhere **except** text inputs and text areas, where the native Cut / Copy / Paste menu is the right one. Panels with no menu of their own show nothing rather than the browser menu.
- Shortcut hints are read from `shortcuts.js` (the user can rebind them), never hard-coded in the menu.
- Disabled, not hidden, when an item makes sense for the surface but not for the current state (e.g. Paste Settings with nothing copied), with the reason in a tooltip; items that cannot apply to the surface at all are not shown. This keeps menus short *and* predictable.

### 3.2 Right-click selects, then acts (the target rule)

On `contextmenu` over a photo: if it is **in** the selection, the menu acts on the whole selection and the selection is untouched; if it is **not**, the selection becomes just that photo first (Lightroom / Finder), then the menu acts on it. The menu header line (a disabled first row) says what it will act on when more than one photo is involved: *"3 photos"*.

### 3.3 Menus by surface

Slice 2 builds the rest after slice 1 proves the engine. **Exists** = the action is already there; **new** = needs code beyond wiring.

| Surface | Items |
|---|---|
| **Library grid cell / filmstrip cell** | Open in Develop; Open in Loupe; *Compare / Survey* (shown when 2+ selected); Rating ▸ (0-5); Flag ▸ (Pick / Reject / Unflag); Colour label ▸; Add to Collection ▸ (existing collections + *New collection...*); Remove from Collection (only when viewing a collection); **Develop Settings ▸** Copy Settings..., Paste Settings, Apply Preset ▸, Reset Settings; Export...; Show in Finder / Explorer (single photo); Copy Path (single); Detect Faces; Remove from Catalog... (danger). *Create Virtual Copy* only if/when that action exists (it does not today). |
| **Library grid, empty area** | Select All; Deselect All; Import Folder...; Import Files... |
| **Develop filmstrip cell** | The same photo menu, plus **Sync Settings...** and **Auto Sync** (when 2+ selected), minus *Open in Develop* (Open in Library instead). |
| **Develop canvas** | Before / After; Zoom ▸ (Fit, 100%); Show Clipping; Mask overlay on / off; Undo / Redo (with the history label, e.g. *"Undo Exposure"*); Copy Settings...; Paste Settings; Reset Settings. Over an active mask pin: Delete mask, Invert mask, Duplicate, Refine with clicks... |
| **History row** | Restore to this state; Create Snapshot from this state; Copy Settings from this state. |
| **Snapshot row** | Apply; Rename; Delete (danger). |
| **Preset row** | Apply to Photo / Selection; Rename; Export...; Delete (danger). |
| **Collection / catalog rail row** | Rename; Edit smart rules...; Delete (danger); Select photos; Export collection... |
| **Mask list row** | Rename; Duplicate; Invert; Show / Hide; Delete (danger). |
| **Develop panel header** | Reset this panel; Show / Hide panel (the eye); Solo this panel. |
| **People / face tile** | Rename person; Merge...; Not this person; Exclude face. |
| **Tone curve / HSL control** | Reset this control (right-click or double-click the slider already resets some; the menu makes it discoverable). |

Dangerous items always confirm (the existing `ConfirmDialog`); nothing here deletes without it.

### 3.4 Multi-select in Develop

- **Selection model.** Develop's filmstrip uses the **same `selection` store** as the Library, so the selection survives Library <-> Develop. Two concepts, as in Lightroom: the **active photo** (`develop.versionId`, the one the canvas and panels show; Lightroom's "most selected") and the **selection** (`selection.selectedIds`, which always contains the active photo).
- **Gestures** (filmstrip, `handleSelect` rules): plain click = open that photo and make it the only selection; **Cmd / Ctrl-click** toggles a photo in the selection and, if it was added, makes it the active photo; **Shift-click** selects the range from the anchor and makes the clicked photo active; arrow keys step the single selection as today; Cmd / Ctrl-A selects all filtered photos. Only the active photo is decoded and uploaded to the GPU, so selecting 100 photos costs nothing in Develop.
- **What a multi-selection enables** (nothing happens to the other photos unless the user asks): Copy / Paste Settings, Apply Preset, rate / flag / label, Export, Remove, the right-click menu acts on all, and the filmstrip drag still carries the whole selection.
- **Sync Settings...** (button in the panel footer when 2+ are selected, and a menu item): opens the Copy Settings group dialog; on confirm the chosen groups of the **active photo's** stack are applied to every other selected photo with `applyPresetOps` (the same code path as paste to selection), one history entry per target labelled *"Sync Settings"*.

### 3.5 Auto Sync

A switch beside *Sync Settings...*, **off by default and never persisted** (it resets to off when the selection becomes a single photo, when the app restarts, and when the module changes). It is rendered unmistakably when on (accent-coloured switch, *"Auto Sync: 4 photos"* chip in the Develop toolbar), because its failure mode is silently editing many photos.

While it is on, an edit to the active photo is propagated to the other selected photos. The design that keeps this safe and fast:

1. **Granularity is the changed op, not the whole stack.** When the active photo's stack is flushed, diff it against the last flushed stack by op identity (`op` name, plus the panel `group`) and take the ops whose value changed (including *removed*, for Reset). Only those ops are written into each target, **replacing that op wholesale** (absolute value), leaving the target's other ops alone. So dragging Exposure on photo A sets Exposure to the same value on B and C, and B's own Contrast stays B's. (This is Lightroom's behaviour, and it avoids a relative-offset rule that the user would have to reason about.)
2. **Same exclusions as Copy Settings**: ops that are per-photo are never auto-synced, namely crop / straighten / perspective, spot removal, red eye, **masks and their local adjustments**, `ai_denoise`, and the lens profile. They can still be synced deliberately with *Sync Settings...*, whose dialog already lets the user tick them.
3. **When**: on the active photo's debounced flush (the same moment it is persisted), never per slider frame. The other photos show their result in the filmstrip thumbnails and when opened; the live preview is the active photo only. A drag of 2 s writes once per photo, not 120 times.
4. **How**: one new Rust command `apply_ops_to_versions(versionIds, ops, removedOps, label)` does the per-target read-merge-write inside **one catalog transaction** and returns the new thumbnail jobs, instead of N x 3 IPC round trips from the frontend. A failure rolls back and is reported once ("Auto Sync failed: ..."); the active photo's own edit is unaffected.
5. **History**: each target gets an entry labelled *"Auto Sync: Exposure"* in its own history (undoable by opening that photo and using its History); the active photo's undo / redo stays its own and does **not** replay on the others. This is stated in the user guide. (Making undo span the whole sync group is a later, separate question.)
6. **Thumbnails**: regenerated for the targets once, after the burst settles, through the existing `regenerateThumbnail` path (which since #226 also warms the Loupe's graded preview).
7. **Conflicts**: a target that is open in another window or is currently the active photo is impossible (one Develop). A target removed from the catalog mid-flight is skipped and counted in the notice.

### 3.6 Not in this RFC

Customising or reordering menu items; plugin-contributed items (the export-plugin hook could add a *Run plugin* entry later); touch long-press; making Auto Sync's undo group-wide; syncing masks with position adaptation (Lightroom's *Match Total Exposures* and *Sync mask* behaviours).

### 3.7 UX (designed first, in a mock)

An interactive mock, [context-menus-multiselect-mockup.html](../ux/mockups/context-menus-multiselect-mockup.html), implements the behaviour above so it can be judged by use before any app code: the target rule, the menu engine (clamping, flip-left submenus, keyboard, disabled-with-reason, danger + confirm, the "N photos" header), the Develop selection gestures with the active photo vs the selection, Sync Settings, and Auto Sync (changed op only, absolute value, excluded ops never synced). Decisions it settled or surfaced:

- **Active vs selected must read at a glance.** The active photo gets the **accent frame**, other selected photos a **neutral grey frame** (not two accent shades). The filmstrip **scrolls the active photo into view** when the active photo changes through a menu or a key, otherwise it can be off-screen while the panels show it.
- **Auto Sync has three cues, because its failure mode is silent bulk edits**: the switch itself (neutral off, accent on, bold label), a **titlebar chip "Auto Sync: N photos"** that is visible whatever panel is scrolled into view, and the switch **dimmed and inert until 2+ photos are selected**. It switches itself off when the selection drops to one photo, and says so in the status line.
- **Per-photo ops are shown as excluded, not hidden**: the panel groups them under a *Per-photo (never auto-synced)* heading in the mock (in the app this is the existing panel order plus a tooltip on the Auto Sync switch listing the exclusions), so a user dragging Crop with Auto Sync on is not surprised that the others did not move.
- **A multi-selection's menu is one header row longer, not different**: the "N photos" header names the target, items that cannot apply to many (Copy Settings, Show in Finder, Copy Path) are **disabled with a reason on hover**, never silently hidden, so the menu keeps the same shape for 1 and for 12 photos (muscle memory).
- **A photo menu is 14 rows for one photo** (incl. separators it fits a 600 px window); nothing was dropped, but *Rating*, *Flag*, *Colour label*, *Add to Collection* and *Develop Settings* are submenus so the first level stays scannable. Submenus flip to the left near the right edge.
- **Focus returns to the photo after the menu closes and after an action** (the grid re-renders on a rating change; the mock lost focus until it was restored explicitly, and the real grid's virtualisation has the same hazard: restore by `version_id`, not by DOM node).
- **A menu closes when the window loses focus** (so it never lingers over another app) and on scroll / resize.
- **Not in the mock** (checked at implementation): the real thumbnails, drag-and-drop interplay, a real native right-click on macOS, touch.

## 4. Slices

0. **UX mock** — done, see §3.7.
1. **Menu engine + photo menus.** `contextMenu` store, `ContextMenu.svelte`, the builder pattern, the app-wide webview-menu suppression, the Library grid cell / empty-area menus and the filmstrip menu, wired to existing actions. vitest for the builders and the store; a Svelte component test for keyboard navigation and clamping; an e2e that right-clicks a cell and rates / flags through the menu.
2. **The other surfaces** in §3.3 (canvas, history / snapshot / preset rows, collection rail, mask list, panel headers, people), a few per PR, each with its builder tests. **2a (done)**: collection, person, snapshot and preset rows and the Presets list -- everything whose action already exists; Rename, *Edit smart rules…*, *Export collection…*, *Merge…* and the History row wait for backend commands. **2b (done)**: Develop canvas, panel headers (+ Solo), the mask panel and added-shape rows (there is no mask *list* in the app, see PROGRESS). Still open: slider / curve *Reset this control* (needs a per-slider default), the mask-pin menu on the canvas, the History row, renames.
3. **Develop multi-select + Sync Settings (done).** Rules as §3.4; two details settled in the code: removing the active photo with Cmd / Ctrl-click hands the role to the nearest selected photo (the last one cannot be removed), and right-clicking a photo outside the selection is a plain click on it (it becomes active, as in Lightroom) so the selection always contains the active photo. Export in Develop now takes the selection. The filmstrip selection gestures, the shared selection, the footer *Sync Settings...* button, the menu entries, the user guide.
4. **Auto Sync.** The stack diff (pure, heavily unit-tested: changed / added / removed ops, excluded ops, no-ops), `apply_ops_to_versions` and its transaction tests, the switch and toolbar chip, an e2e that edits one photo and checks the others.
5. **Polish**: a keyboard-only pass, the user guide, a screenshot pass at the narrow filmstrip width.

## 5. Risks

- **Right-click on macOS WKWebView**: Ctrl-click also fires `contextmenu`, which is fine, but the filmstrip's pointer-based photo drag (`shell.handlePhotoDragPointerDown`) must ignore button 2 so a right-click never starts a drag. The WebDriver e2e can dispatch a `contextmenu` event but not a real native one; the real gesture is a manual check (backlog).
- **Auto Sync scale**: with 200 photos selected one edit means 200 writes and 200 thumbnails. Mitigation: the single transaction, thumbnails regenerated lazily (the cells show the old thumbnail with the existing "stale" treatment until done), and a confirm when the selection is larger than a threshold (proposed 50) when Auto Sync is first switched on.
- **Active-photo changes while a sync is in flight** (the user clicks another photo): the flush for the old photo is awaited before the switch (the existing `openDevelop` ordering, invariants I1-I3), and the sync is part of that awaited work.
- **Menus that lie**: a builder offering an action that does nothing for the current state is the usual failure of context menus. Hence *pure builders with a test per surface x selection shape* (none, one, many, mixed) instead of ad-hoc conditionals in components.
- **Op diffing**: ops that hold a list (tone curve points, HSL bands) must be treated as one op, or a single moved point would sync as a partial list. The stack's op granularity needs confirming against `develop.js` in slice 4's first commit.

## 6. Open questions

1. Should **Cmd / Ctrl-click in Develop make the clicked photo active** (Lightroom) or keep the current one active and only extend the selection? Proposed: Lightroom's.
2. The **Auto Sync confirm threshold** (50) and whether Auto Sync should refuse above a hard cap.
3. Whether *Reset Settings* in the photo menu on a multi-selection should confirm (proposed: yes, always, because it is destructive and wide).
4. Menu item **icons**: none in v1 (text + shortcut hints), to keep the first slice small. Agree?
