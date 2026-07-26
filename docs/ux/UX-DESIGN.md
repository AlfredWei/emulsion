# UX Design — Library & Develop

- Status: Draft — for review alongside the [visual mockup](mockups/library-develop-mockup.html)
- Date: 2026-07-25
- Relates to: [PRD §7.3, §7.4](../../PRD/PRD.md), [ADR-0002](../adr/ADR-0002-frontend-ui-stack.md), [RFC-0001](../rfc/RFC-0001-architecture-and-tech-stack.md)

## 1. Design principles

- **Dark-mode-first, not dark-mode-optional.** Photography tools default to neutral/dark chrome so the surrounding UI doesn't bias the eye's color and brightness judgment of the image itself. This is the primary theme, not an afterthought toggle.
- **Content-first, minimal chrome.** The photo is the largest thing on screen in every module. Panels are narrow, collapsible, and use a restrained type scale — the UI should recede.
- **Perceived performance is a design requirement, not just an engineering one.** Every view that can show *something* instantly (an embedded JPEG preview, a skeleton grid cell, a cached thumbnail) does so before the "real" full-quality version is ready, per the decode-once pipeline in [RFC-0001 §4](../rfc/RFC-0001-architecture-and-tech-stack.md#4-core-data-flow-the-develop-loop-decode-once-edit-reactively).
- **Keyboard-first for power users, discoverable for everyone else.** Every frequent action (flag, rate, flip module, nudge a slider) has a keyboard shortcut, but the mouse-driven UI is never a second-class fallback — both are first-class.

## 2. Information architecture

Two modules for M1 scope ([MILESTONES.md M1](../../PRD/MILESTONES.md#m1--mvp-import--library--basic-develop--export)): **Library** and **Develop**, reached via a persistent module switcher (top-left, always visible). A persistent filmstrip along the bottom of both modules shows the current filtered image set, so context is never lost switching between culling and editing.

```
┌─────────────────────────────────────────────────────────────┐
│  [Library] [Develop]              search / filter bar         │
├───────────┬─────────────────────────────────┬─────────────────┤
│  left      │                                 │  right           │
│  rail      │        main content area         │  panel           │
│            │                                 │                  │
├───────────┴─────────────────────────────────┴─────────────────┤
│  filmstrip (persistent across modules)                          │
└─────────────────────────────────────────────────────────────┘
```

## 3. Library module

- **Left rail**: folder tree (source of truth on disk) above a Collections/Smart Collections tree — mirrors [PRD §7.3](../../PRD/PRD.md).
- **Main area**: virtualized grid, adjustable cell size (slider or `+`/`-`), flag/rating/color-label affordances visible on hover or always-on at larger cell sizes. Virtualization keeps DOM node count bounded regardless of catalog size — a 50k-image catalog costs the same DOM weight as a 500-image one.
- **Right panel**: metadata (EXIF read-only, IPTC editable), histogram, keywording — collapsible sections so the panel doesn't force a fixed height budget.
- **Top bar**: search, filter chips (rating/flag/label/camera/lens/date), sort control, grid/loupe view toggle.
- **Loupe (single-image) view**: replaces the grid with one large image plus a lightweight filmstrip strip; same right panel persists.

## 4. Develop module

- **Left rail**: presets, snapshots, history — a scrollable list, most-recent history entry highlighted.
- **Main area**: the canvas (the in-webview WebGPU render target from [RFC-0001 §4](../rfc/RFC-0001-architecture-and-tech-stack.md)), with zoom/pan and a before/after toggle (split or full-swap).
- **Right panel**: collapsible, tool-grouped adjustment sections matching [PRD §7.4](../../PRD/PRD.md) — Basic (WB, tone), Tone Curve, HSL/Color, Detail (sharpen/NR), Effects (dehaze, grain, vignette), Lens Corrections. Sections default to a sensible collapsed/expanded state (Basic open, others closed) so the panel isn't overwhelming on first open — mirrors how experienced Lightroom users actually work.
- **Bottom**: filmstrip (shared component with Library) plus a contextual tool strip above it for crop/mask/heal tools when active.
- **Local-adjustment tools** (M3+, not M1/M2): the tool strip and mask-overlay affordances are designed into this layout now even though the underlying capability ships later, per [RFC-0001's](../rfc/RFC-0001-architecture-and-tech-stack.md) note that the render pipeline must already be architected for masks/layers from the start.

## 5. Performance/memory-visible UI rules

These are UI-level rules that exist specifically because of the PRD's performance/memory requirements ([PRD §7.6, §9](../../PRD/PRD.md)) and the render architecture in [ADR-0004](../adr/ADR-0004-rendering-and-color-management.md):

- Bound the number of concurrent full-resolution decoded images resident in GPU memory (e.g., current + immediate filmstrip neighbors); everything else uses cached thumbnails until scrolled/selected into range.
- Evict off-screen textures proactively rather than relying on OS memory pressure signals.
- Coalesce/debounce slider and pointer-drag events into the shader frame loop — the UI should never dispatch a GPU pass per raw input event.
- Expose a visible **quality/performance mode** (Draft / Standard / High) the user can toggle, mirroring Lightroom's own tradeoff control — Draft uses lower-resolution working buffers for faster feedback on large catalogs or modest hardware.

## 6. Visual language (see mockup for the concrete rendering)

- Neutral dark background (near-black, not pure black, to keep contrast comfortable), single accent color used sparingly (selection states, active tool, primary actions only).
- 4/8px spacing grid; a small, restrained type scale (UI chrome should never compete with the image for attention).
- Icon-first tool strip with text labels on hover/focus, not permanent labels, to keep the tool strip compact.
- Respect native OS window chrome (macOS traffic-light controls, Windows title bar) rather than a custom-drawn title bar — keeps the app feeling native despite the webview-based UI ([ADR-0001](../adr/ADR-0001-application-shell.md)).

## 7. Baseline accessibility (full pass is M8, this is the floor for M1+)

- Full keyboard navigability and visible focus states from the first UI built, not retrofitted later.
- WCAG AA contrast for all text/icon-on-panel-background combinations in the dark theme.
- HiDPI/OS-display-scaling awareness — the UI must render crisply at both 1x and 2x+ scaling on both target OSes.

## 8. Visual mockup

A static, non-functional HTML mockup of the Library and Develop screens implementing the principles above is at [mockups/library-develop-mockup.html](mockups/library-develop-mockup.html), also published for inline review. It is a design reference only — it is not application code, and implementation still begins at M0 per [MILESTONES.md](../../PRD/MILESTONES.md).
