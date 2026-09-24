# RFC-0013: Per-panel visibility toggle and reset (Develop panel)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-24
- Companion documents: [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md), [ADR-0006](../adr/ADR-0006-edit-representation.md), [develop_engine/pipeline.rs](../../app/src-tauri/src/develop_engine/pipeline.rs), [api/develop.js](../../app/src/lib/api/develop.js), [state/developView.svelte.js](../../app/src/lib/state/developView.svelte.js), [components/DevelopPanel.svelte](../../app/src/lib/components/DevelopPanel.svelte), [PROGRESS.md](../../PROGRESS.md)

Per [[m5-rfc-adr-practice]]: this touches both the persisted edit-stack schema (ADR-0006) and both render paths (ADR-0004), so it gets an RFC before implementation, same discipline as the M5.6 guided-filter slices — a separate, unrelated track, not a fourth M5.6 finding.

## 1. Problem

Requested directly: each Develop panel section (Basic, Tone Curve, HSL/Color, Split Toning, Texture & Clarity, Dehaze, Sharpening, Noise Reduction, Vignette, Grain, Lens Corrections, Perspective) should get an eye-icon **visibility** toggle — real Lightroom's own "panel switch," a quick before/after comparison that temporarily excludes that panel's whole contribution from the rendered image without touching its stored slider values — and a **reset** control that reverts just that panel's own values to default, leaving every other panel untouched.

Today only two blunt instruments exist: `resetEditStack` (DevelopPanel's global Reset button) clears the *entire* edit stack, and each op's own getter already falls back to an `IDENTITY_*` default when the op is *absent* — but there's no per-panel granularity for either action, and no concept of "hidden but still holding a value" at all.

Confirmed with the user: this **persists with the photo and affects export**, matching real Lightroom's panel switches (not an ephemeral preview-only toggle), and applies at the **13 top-level panel granularity** DevelopPanel.svelte already has (not finer, e.g. not splitting Texture from Clarity or Luminance NR from Color NR).

## 2. Non-goals

- **Soft Proof is excluded.** It's not an edit-stack op at all — it's view-time ICC-profile simulation state (`softProof.svelte.js`), already has its own "Enable Soft Proofing" checkbox, and has no slider values to "reset" in the same sense. That leaves **12** panels in scope, not the full 13 DOM sections.
- **Masks/local adjustments, Crop, and Lens Corrections' own existing `profile_enabled` checkbox are untouched.** Masks aren't a DevelopPanel accordion section (they live in `MaskEditorPanel`/`MaskToolStrip`) and weren't part of what was asked. Crop isn't one of the 13 sections either. `lens_correction.profile_enabled` stays exactly what it is today (whether the *profile-based* correction applies) — panel-level visibility is a new, independent layer on top: hiding the whole Lens Corrections panel bypasses it regardless of `profile_enabled`'s own value.
- **No change to any existing op's schema or default-value contract.** A hidden panel's ops are still stored, still readable, still editable while hidden (sliders stay interactive, per real Lightroom) — only what gets *rendered* changes.

## 3. Design — the new op and panel→op-group mapping

One new, minimal op, following this codebase's own "presence = the interesting state, absence = default" convention (`resetEditStack`'s own doc comment: "every section's own getter already falls back to its own identity default when its op is absent"):

```json
{ "op": "panel_hidden", "panel": "dehaze" }
```

Presence of a `panel_hidden` entry for a given `panel` key means: **treat every op belonging to that panel as absent for rendering purposes** (both preview and export), without deleting or altering those ops' own stored entries. Toggling visibility back on simply removes that one marker entry. Multiple hidden panels are multiple `panel_hidden` entries — a small, ordinary extension of the existing "bag of typed op objects" shape (ADR-0006), not a new sub-schema.

Fixed panel→op-name mapping (shared conceptually by both runtimes, each with its own small native copy — see §4/§5):

| Panel key | Op name(s) |
|---|---|
| `basic` | `exposure`, `contrast`, `saturation`, `temperature`, `tint`, `highlights`, `shadows`, `whites`, `blacks` |
| `tone_curve` | `tone_curve` |
| `hsl` | `hsl` |
| `split_toning` | `split_toning` |
| `texture_clarity` | `texture`, `clarity` |
| `dehaze` | `dehaze` |
| `sharpening` | `sharpen` |
| `noise_reduction` | `luma_nr`, `color_nr` |
| `vignette` | `vignette` |
| `grain` | `grain` |
| `lens_corrections` | `lens_correction` |
| `perspective` | `perspective` |

**Reset** is a separate, independent action: it removes a panel's own op entries from the stack entirely (falling back to each field's existing `IDENTITY_*`/fallback default, the exact same mechanism `resetEditStack` already relies on, just scoped to one panel's op names instead of clearing the whole array) — it does **not** touch that panel's `panel_hidden` marker. Toggling visibility does not reset values, and resetting does not change visibility. Two independent real Lightroom behaviors (you can hide a panel and keep tweaking it; you can reset a panel without it ever having been hidden), kept as two independent code paths here too.

## 4. Design — render-time filtering (CPU/Rust, affects preview *and* export)

Both `apply_edit_stack` and its three siblings (`apply_lens_correction`, `apply_perspective`, `apply_crop`) are always called together, in the same order, from exactly three call sites: `preview_cache.rs::ensure_graded_preview_for_hash` (interactive preview + hover-preview), `export.rs` (real export), and `import.rs::regenerate_edited_thumbnail` (grid thumbnails). None of the four functions themselves need to change — a single new helper, called once at each of those three sites *before* the existing four calls, does the filtering centrally:

```rust
/// Strips every op belonging to a hidden panel (per `panel_hidden` markers),
/// and the markers themselves, before any apply_* function ever sees the
/// stack -- so apply_edit_stack/apply_lens_correction/apply_perspective/
/// apply_crop stay completely unaware panel visibility exists; a hidden
/// panel's ops are, to them, simply absent, the same as if the user had
/// never touched that panel at all.
pub(crate) fn effective_stack_for_render(stack: &EditStack) -> EditStack { ... }
```

`export.rs`/`preview_cache.rs`/`import.rs` each gain one line (`let stack = &effective_stack_for_render(stack);`) ahead of their existing four `apply_*` calls — everything downstream is untouched, including every existing Rust unit test that constructs an `EditStack` directly and calls `apply_edit_stack` on it (those bypass this helper entirely, matching their own scope: they test the *op*, not panel visibility).

## 5. Design — render-time filtering (GPU/WGSL, preview only — export is CPU-only already)

No shader, pipeline, or uniform-buffer changes at all. `developView.svelte.js`'s per-field `$derived`s (`exposure`, `vignette`, `lumaNr`, ...) are what `DevelopCanvas.svelte`'s uniform-buffer builders ultimately read, and every one of them is already shaped `getterFn(this.develop.editStack, ...)`. Adding one more `$derived` — `effectiveEditStack`, wrapping `develop.editStack` through the JS twin of §4's Rust helper — and repointing those existing field derivations from `this.develop.editStack` to `this.effectiveEditStack` (a mechanical one-line change per field, ~13 lines) makes every GPU input panel-visibility-aware for free, since "hidden" and "absent" already produce identical output through every one of these getters' own existing fallback logic. `develop.editStack` itself (the thing sliders write to) is untouched — only what feeds the renderer's *read* side changes.

## 6. Design — UI (DevelopPanel.svelte)

Each of the 12 `<summary>` headers gains two small icon buttons in a `header-actions`-style flex row (the same pattern the Basic panel's own eyedropper/auto-WB buttons already establish): an eye/eye-slash toggle (`onclick` → a new `handleTogglePanelVisibility(panelKey)` action) and a reset icon (`onclick` → `handleResetPanel(panelKey)`, guarded behind a small confirm only if that's this project's existing convention for destructive-ish actions — check `historyActions.js`'s own Reset confirm dialog for precedent). A hidden panel's own body gets a dimmed/reduced-opacity treatment (CSS only, e.g. `opacity: 0.5` on `.sub-body`) so it's visually obvious without disabling the controls themselves (they stay interactive, per §3). Both actions go through the same `upsertOp`-family + `develop.editStack = ...` + `scheduleFlush(label)` path every other mutation in this codebase already uses, for undo/redo compatibility — no new mutation idiom introduced.

## 7. Testability

- **Rust unit tests**: `effective_stack_for_render` — a stack with a `panel_hidden` marker for `dehaze` renders identically to the same stack with the Dehaze op removed entirely and no marker; a stack with no markers is unchanged; hiding one panel leaves every other panel's ops untouched (a multi-panel stack, assert only the hidden one's contribution disappears).
- **JS unit tests**: the JS twin of the same filter, and `developView`'s derived fields reading through it — hiding `vignette` makes `developView.vignette` return `IDENTITY_VIGNETTE` even though `develop.editStack` still holds the real values (round-trip: un-hide, the real values reappear unchanged).
- **Reset**: each panel's reset action removes exactly that panel's own op names and nothing else, verified against the panel→op-name table in §3 (one test per panel, or one parameterized test iterating the table — avoids 12 near-duplicate tests).
- **Visual**: in the browser (dev server), toggle a panel with a visible effect (e.g. Vignette) off — the canvas reverts to as-if-absent instantly, the slider values are unchanged and still draggable, and toggling back on restores the effect at the exact prior values with no re-render glitch.
- **Export**: a hidden panel's effect is confirmed absent from an actual exported file, not just the preview — this is the one behavior that's easy to get right in preview and wrong in export if `effective_stack_for_render` isn't wired into `export.rs` too, so it gets its own explicit check.

## 8. Exit criteria

- `panel_hidden` op, `effective_stack_for_render` (Rust) and its JS twin, both wired into every render entry point (§4/§5) — preview, hover-preview, thumbnails, and export all agree.
- All 12 panels have a working visibility toggle and reset in the UI, matching the §3 table exactly.
- `cargo test`/`npm run check`/vitest/`vite build` all green; new tests per §7 passing.
- Docs: `docs/PROJECT_STRUCTURE.md` notes the new op if it documents the ops schema at that level of detail (check current convention before adding); ADR-0006 gets a dated "Update" section (per [[m5-rfc-adr-practice]]) describing the new op and its "presence = hidden, absence = visible" convention; `PROGRESS.md` gets an entry.
