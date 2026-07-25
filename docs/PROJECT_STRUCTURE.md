# Project Structure

A map of what lives where in this repo. See [README.md](../README.md) for the project overview and quick start, and [PROGRESS.md](../PROGRESS.md) for current status — this doc is just "what's this file/folder for."

## Top level

```
lr_replace/
├── PRD/                  Product requirements — what to build, in what order
├── docs/                 Engineering docs — how it's built, and why
├── app/                  The actual Tauri + Svelte application
├── .github/workflows/    CI (GitHub Actions)
├── Makefile               make dev / make test / make build, see `make help`
├── README.md               Project overview + quick start
├── PROGRESS.md             Running status log — read this first after time away
└── .gitignore
```

## `PRD/` — product requirements

| File | What it's for |
|---|---|
| [PRD.md](../PRD/PRD.md) | The product requirements document: vision, scope, target user, functional/non-functional requirements. |
| [MILESTONES.md](../PRD/MILESTONES.md) | The 8-milestone roadmap (M0–M7), each with scope, explicitly-deferred items, and exit criteria. |
| [lightroom-reference.md](../PRD/lightroom-reference.md) | Research on Lightroom's actual v1→now feature timeline, used to sequence the roadmap above. |

## `docs/` — engineering docs

```
docs/
├── rfc/
│   └── RFC-0001-architecture-and-tech-stack.md   The system architecture, ties all the ADRs together
├── adr/                                           One decision per file: context, decision, consequences,
│   ├── ADR-0001-application-shell.md               alternatives considered — several have dated "M0/M1
│   ├── ADR-0002-frontend-ui-stack.md                spike finding" sections added after reality corrected
│   ├── ADR-0003-raw-decoding.md                     the original plan (that's expected, not a mistake —
│   ├── ADR-0004-rendering-and-color-management.md   see each ADR's own update log)
│   ├── ADR-0005-catalog-storage.md
│   └── ADR-0006-edit-representation.md
├── ux/
│   ├── UX-DESIGN.md                       Design principles + module layouts for Library/Develop
│   └── mockups/library-develop-mockup.html  Static, reviewed reference mockup (not app code)
└── PROJECT_STRUCTURE.md                   This file
```

Start with RFC-0001 for the big picture, then the individual ADRs for why a specific piece is built the way it is.

## `app/` — the application

Two halves: `src-tauri/` is the Rust core (catalog, RAW decode, all app logic), `src/` is the Svelte frontend (UI only — no business logic lives here, it calls into `src-tauri` via Tauri commands).

### `app/src-tauri/` — Rust core

```
src-tauri/
├── src/
│   ├── main.rs          Entry point, just calls into lib.rs
│   ├── lib.rs            Tauri command definitions + app setup (catalog opened here)
│   ├── catalog.rs         SQLite catalog: schema, Catalog struct, all DB methods
│   ├── import.rs          Import pipeline: scan a folder, hash, dedupe, thumbnail
│   └── raw_decode.rs       LibRaw (via `rsraw`) wrapper for decoding RAW files
├── capabilities/
│   └── default.json      Which Tauri APIs the webview is allowed to call
├── icons/                 App icons for bundling
├── Cargo.toml              Rust dependencies
└── tauri.conf.json          App config: window, identifier, asset protocol, bundle targets
```

Every file in `src/` has real unit tests colocated in a `#[cfg(test)] mod tests` block at the bottom — run them with `make test` (`cargo test --lib`). Tests that need a real RAW file are gated behind an `EMULSION_TEST_RAW_SAMPLE` env var rather than a committed fixture (see ADR-0003) — they skip cleanly without it, both locally and in CI (where the CI workflow fetches a sample first).

### `app/src/` — Svelte frontend

```
src/
├── routes/
│   ├── +layout.js          SPA-mode config (Tauri has no server, so no SSR)
│   ├── +page.svelte         THE REAL APP — Library module (grid, import, culling)
│   ├── m0-spike/            Throwaway: M0's WebGPU-in-webview validation
│   ├── m1-smoke/             Throwaway: M1 Slice 1's import backend smoke test
│   └── m1-slice2-smoke/       Throwaway: M1 Slice 2's asset-protocol smoke test
├── lib/
│   ├── api/catalog.js        Thin wrapper around invoke() calls — the only place
│   │                          that knows the Tauri command names/argument shapes
│   ├── components/
│   │   ├── LibraryGrid.svelte   Hand-rolled virtualized grid (no library — see
│   │   │                         UX-DESIGN.md §5 and the component's own comment)
│   │   └── GridCell.svelte       One thumbnail: image + hover rating/flag/color controls
│   └── styles/tokens.css     Dark-theme design tokens, ported from the reviewed mockup
├── app.html                 SvelteKit's HTML shell
└── static/                  Static assets (icons, etc.)
```

**The `m*-spike`/`m*-smoke` routes are not part of the app** — they're throwaway diagnostic pages used to verify something empirically (in-webview WebGPU, a real RAW decode, the asset protocol) when there's no tool available in this environment to screenshot or drive the native window directly. Each one is self-contained, reports its result by invoking a `report_spike_result` Tauri command that prints to the Rust process's stdout, and is left in the repo as a re-runnable check rather than deleted — but none of them are linked from the real UI. If one of these routes is ever loaded by the actual app (via `tauri.conf.json`'s window `url`), that's a leftover from manual testing that should have been reverted — check `git diff app/src-tauri/tauri.conf.json` if `make dev` ever opens one of these instead of the real Library view.

## `.github/workflows/ci.yml`

Two jobs on every push/PR to `main`: a `macos-latest`/`windows-latest` matrix that builds and tests the Rust core (fetching a real sample RAW file so the gated tests actually run, not just skip), and a `frontend-check` job (`npm run check`). The Windows job is currently failing **on purpose** — see ADR-0003 and PROGRESS.md, it's a real, confirmed, tracked gap, not a broken pipeline.
