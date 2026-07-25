# ADR-0003: RAW decoding — LibRaw via Rust FFI bindings

- Status: Accepted
- Date: 2026-07-25
- Relates to: [PRD §5, §7.2](../../PRD/PRD.md), [PRD risk: RAW library coverage](../../PRD/PRD.md#11-key-risks), [RFC-0001](../rfc/RFC-0001-architecture-and-tech-stack.md)

## Context

The PRD (confirmed decision, §5) commits to using an existing open-source RAW decode library rather than building a proprietary demosaic engine, and states "broad RAW format support" as a Library/Import requirement (§7.2). Two real options exist in the Rust ecosystem:

1. **LibRaw** (C++ library) via Rust FFI bindings — mature, broadest camera coverage (400+ models), the de facto standard used by most non-Adobe RAW tools.
2. **`rawler`** — a pure-Rust RAW decoder, no FFI/C++ build dependency, but materially narrower camera-format coverage as of 2026.

## Decision

Use **LibRaw via Rust FFI bindings** (the `rsraw` crate, which vendors LibRaw as a build dependency and already supports macOS/Windows/Linux builds), rather than the pure-Rust `rawler`.

## Rationale

- Camera-format breadth is a direct, user-visible requirement (PRD §7.2, §11) — a photographer whose camera isn't supported has no workaround. LibRaw's coverage is the strongest available option today.
- `rsraw` already solves cross-platform build packaging for the two target OSes, which was the main cost of choosing LibRaw over a pure-Rust option.
- The FFI boundary is narrow and well-scoped (decode RAW → linear pixel buffer + embedded preview + metadata), which limits the blast radius of introducing a C++ dependency into an otherwise Rust codebase.

## Consequences

- The build pipeline must compile/link LibRaw (C++) on both macOS and Windows — real but bounded complexity, already handled by the `rsraw` crate's existing build support.
- New camera model support depends on upstream LibRaw releases, not on this project's own code — acceptable, matches how virtually every non-Adobe RAW tool (darktable, RawTherapee) sources camera support.
- This is flagged in the PRD as a standing risk (§11): even LibRaw can lag on brand-new camera models or differ from Adobe's proprietary color science. Mitigation: ship a documented "supported cameras" list per release, and treat gaps as expected/normal rather than bugs.

## Alternatives considered and rejected

- **`rawler` (pure Rust)**: rejected for now on coverage grounds — avoiding the C++ FFI dependency is appealing (simpler builds, full memory safety through the decode path) but not worth shipping with meaningfully fewer supported cameras than users expect from a Lightroom-class tool. **Revisit trigger**: if `rawler`'s camera coverage reaches parity with LibRaw for the cameras our actual user base owns, re-evaluate switching to remove the FFI dependency entirely.
- **Proprietary in-house decoder**: already rejected at the PRD level (§5) — this ADR just confirms the concrete library choice within that decision.
