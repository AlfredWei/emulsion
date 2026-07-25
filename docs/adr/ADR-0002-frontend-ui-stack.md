# ADR-0002: Frontend UI stack — Svelte + a small custom design system

- Status: Accepted
- Date: 2026-07-25
- Relates to: [ADR-0001](ADR-0001-application-shell.md), [UX-DESIGN.md](../ux/UX-DESIGN.md)

## Context

Given [ADR-0001](ADR-0001-application-shell.md), the UI is a standard web frontend running inside Tauri's webview. This ADR picks the frontend framework and component approach. This decision is materially **lower-stakes and more reversible** than ADR-0001 — swapping frontend frameworks later touches only the UI layer, not the Rust core, catalog, or rendering architecture.

Candidates: React, Svelte, SolidJS, Vue; and separately, whether to adopt a generic component library (e.g., MUI, Ant Design) or build a small custom design system.

## Decision

**Svelte** (compiled, no virtual-DOM runtime overhead) as the frontend framework, with a **small custom design system** (not a generic UI kit) built specifically for this app's dark-mode-first, content-dense, photo-tool aesthetic.

## Rationale

- **Performance fit**: Svelte compiles away its framework overhead at build time rather than shipping a VDOM runtime, which matters here because the UI updates very frequently (slider drags, filmstrip scroll, grid virtualization) inside a resource-constrained webview process that's also expected to host WebGPU compute ([ADR-0004](ADR-0004-rendering-and-color-management.md)). React's VDOM diffing is unnecessary overhead for this update pattern.
- **A generic component library actively works against "modern and easy to use"**: off-the-shelf kits read as generic web-app chrome, not a purpose-built creative tool. Lightroom, darktable, Capture One all have a distinctive dense/dark visual language that a generic kit doesn't produce. A small custom system (defined in [UX-DESIGN.md](../ux/UX-DESIGN.md)) is worth the extra up-front cost.
- SolidJS was a close second (similar no-VDOM performance profile); Svelte was preferred for its more mature ecosystem and gentler learning curve for a small team, without giving up the performance property that matters.

## Consequences

- All custom components (panels, sliders, histogram, filmstrip, virtualized grid) need to be built in-house — budget real design/implementation time for this rather than assuming a component library covers it.
- Svelte's smaller ecosystem than React means some utility libraries may need thin wrappers or replacement; acceptable tradeoff given the performance/aesthetic rationale above.

## Alternatives considered and rejected

- **React**: largest ecosystem, but VDOM runtime overhead is the wrong tradeoff for a UI this update-heavy, and reaching for it usually pulls in a generic component library by default, working against the "modern" requirement.
- **Vue**: reasonable middle ground, not chosen — no strong advantage over Svelte for this use case, and Svelte's compiled-away-runtime property is a better match for the stated performance priority.
- **Generic component library (MUI/Ant/etc.) on top of any framework**: rejected — visually generic, and these kits are optimized for form-heavy business apps, not dense creative-tool panels (collapsible tool sections, histograms, mask overlays) this app actually needs.
