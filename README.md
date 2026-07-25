# Emulsion

*A local-first Lightroom Classic replacement — working title.*

[![CI](https://github.com/AlfredWei/emulsion/actions/workflows/ci.yml/badge.svg)](https://github.com/AlfredWei/emulsion/actions/workflows/ci.yml)

Emulsion is a desktop app for photographers to import, organize, non-destructively edit, and export their photo libraries — including camera RAW — without a subscription or a cloud account. The catalog and the pixels stay on your own disk; there's no server component anywhere in the design.

**Status: M0 complete, M1 in progress (Slices 1–2 of 5 done).** See [PROGRESS.md](PROGRESS.md) for exactly what's confirmed, what's deferred, and what's next. Not yet a usable app — see [Current state](#current-state-what-youll-actually-see) below before you go looking for features.

## Quick start

Prerequisites: [Rust](https://rustup.rs/) (1.77.2+; project developed against 1.97.1), [Node.js](https://nodejs.org/) (18+; developed against v23.5.0), and the platform's native build tools (Xcode Command Line Tools on macOS; [vcpkg](https://vcpkg.io/) + `vcpkg install libraw:x64-windows-static-md` on Windows — see [Tauri's prerequisites guide](https://tauri.app/start/prerequisites/) too, and [Platform support](#platform-support) below).

```bash
make install   # npm install
make dev       # start the app (Tauri window + Vite dev server)
```

`make help` lists everything else (`test`, `build`, `check`, `spike`, `clean`). See [Makefile](Makefile).

## Current state: what you'll actually see

Running `make dev` opens the real Library module: import a folder via the native picker, real thumbnails render in a virtualized grid, and you can rate/flag/color-label images — all backed by a real, persistent SQLite catalog. Develop (editing) isn't built yet — that module currently shows a placeholder. See [PROGRESS.md](PROGRESS.md) for the exact scope cut (no folder tree, no metadata panel, no filter/sort yet — those are later slices).

- **The Rust core** (catalog schema, RAW decoding, import) — has real unit tests: `make test`.
- **The M0 WebGPU validation page** — the proof the rendering architecture works, at `app/src/routes/m0-spike/`. Run `make spike` to load it directly (auto-reverts the config change on exit).
- See [docs/PROJECT_STRUCTURE.md](docs/PROJECT_STRUCTURE.md) for what every file/folder in this repo is for, including the throwaway diagnostic routes (`m0-spike`, `m1-smoke`, `m1-slice2-smoke`) that aren't part of the real app.

## Documentation map

| Doc | What it's for |
|---|---|
| [PRD/PRD.md](PRD/PRD.md) | Product requirements: vision, scope, target user, functional/non-functional requirements. Start here for *what* this is. |
| [PRD/MILESTONES.md](PRD/MILESTONES.md) | The 8-milestone roadmap (M0–M7), each with scope / explicitly-deferred / exit criteria. Start here for *what's next*. |
| [PRD/lightroom-reference.md](PRD/lightroom-reference.md) | Research on Lightroom's actual v1→now feature timeline, used to sequence the roadmap above. |
| [docs/rfc/RFC-0001](docs/rfc/RFC-0001-architecture-and-tech-stack.md) | The system architecture, tying together all the ADRs below into one picture. Start here for *how it's built*. |
| [docs/adr/](docs/adr/) | Individual architecture decisions (app shell, frontend stack, RAW decoding, rendering/color pipeline, catalog storage, edit representation), each with context, decision, consequences, and alternatives considered — several have dated "M0 spike finding" sections where reality corrected the original plan. |
| [docs/ux/UX-DESIGN.md](docs/ux/UX-DESIGN.md) | Design principles and module layouts for Library/Develop, plus a reviewed static mockup at [docs/ux/mockups/](docs/ux/mockups/). |
| [docs/PROJECT_STRUCTURE.md](docs/PROJECT_STRUCTURE.md) | What every file and folder in this repo is for. Start here for *where things live*. |
| [PROGRESS.md](PROGRESS.md) | Running log of what's actually done vs. in progress vs. blocked — the first thing to read after time away from this project. |

## Architecture, in short

- **Shell**: [Tauri](https://tauri.app/) — Rust core + the OS's own webview, not a bundled browser engine ([ADR-0001](docs/adr/ADR-0001-application-shell.md)).
- **Frontend**: Svelte(Kit), no generic component library ([ADR-0002](docs/adr/ADR-0002-frontend-ui-stack.md)).
- **RAW decoding**: [LibRaw](https://www.libraw.org/) via Rust FFI (`rsraw`) ([ADR-0003](docs/adr/ADR-0003-raw-decoding.md)).
- **Rendering**: "decode once in Rust, edit reactively via in-webview WebGPU" — confirmed working on macOS by an M0 spike, not just assumed ([ADR-0004](docs/adr/ADR-0004-rendering-and-color-management.md)).
- **Catalog**: embedded SQLite via `rusqlite`, XMP as export-only interchange, never the source of truth ([ADR-0005](docs/adr/ADR-0005-catalog-storage.md)).
- **Edits**: a versioned, JSON-serializable, fully non-destructive edit stack ([ADR-0006](docs/adr/ADR-0006-edit-representation.md)).

Permanently out of scope: cloud sync, mobile companion app, video editing beyond basic trim. See [PRD.md §3](PRD/PRD.md#3-non-goals-permanent-not-just-later).

## Platform support

Developed on macOS; Windows is validated via CI on GitHub's own Windows runners (see [.github/workflows/ci.yml](.github/workflows/ci.yml)), not a local machine — this project doesn't have one. **Both platforms are green as of 2026-07-25**: the Rust core builds and its full test suite passes (12/12, including a real RAW decode) on both `macos-latest` and `windows-latest`. RAW decoding (`rsraw`) links a vcpkg-installed prebuilt LibRaw on Windows instead of building from source under MSVC — see [ADR-0003](docs/adr/ADR-0003-raw-decoding.md) for why and [app/src-tauri/vendor/rsraw-sys/PATCH.md](app/src-tauri/vendor/rsraw-sys/PATCH.md) for exactly what was patched.

Still genuinely untested on Windows: the in-webview WebGPU rendering path (ADR-0004) — CI only builds/tests the Rust core, it doesn't launch the full GUI app on any platform yet. See [PROGRESS.md](PROGRESS.md) for current status.

## Working practices

- `main` is the stable branch. Non-trivial work happens on a `feature/*` or `docs/*` branch, gets pushed, and opens a PR — then **stops for review**. Only after explicit approval does it get merged (merge-commit style, not squash/rebase) and `main` gets pulled before starting the next branch. See the commit history for the pattern (M0/M1 Slice 1's earliest history predates this and was merged directly).
- [PROGRESS.md](PROGRESS.md) gets updated as work lands, not just at the end of a milestone — read it first when picking this project back up.
- No cloud, no telemetry, no external dependencies beyond what's declared in `app/package.json` / `app/src-tauri/Cargo.toml`.

## License

Not yet decided — tracked as an open decision in [MILESTONES.md M7](PRD/MILESTONES.md#m7--polish-extensibility-10-launch). This is currently a private repository.
