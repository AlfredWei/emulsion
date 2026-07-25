# ADR-0005: Catalog storage — embedded SQLite, XMP as export-only interchange

- Status: Accepted
- Date: 2026-07-25
- Relates to: [PRD §7.1](../../PRD/PRD.md#71-catalog--library-engine-foundation-not-user-facing-on-its-own), [ADR-0006](ADR-0006-edit-representation.md)

## Context

PRD §7.1 requires: a single local catalog storing image references, metadata cache, edit history, collections, and keywords; the catalog never stores original pixel data; and the catalog must survive moved/renamed folders, missing/offline volumes, and corruption (backup/repair path). This mirrors how Lightroom itself works (see [lightroom-reference.md](../../PRD/lightroom-reference.md), Era 1): a SQLite database next to the photos, with edits also expressible as XMP sidecars.

## Decision

Use an **embedded SQLite database** (via the `rusqlite` Rust crate) as the catalog's source of truth, living as a single file the user can locate, back up, and move. **XMP sidecar files are generated on demand as an export/interchange format**, not treated as the source of truth the app reads from.

## Rationale

- SQLite is battle-tested for exactly this shape of workload (single-writer, many-reader, embedded, file-based, must survive crashes) and is what Lightroom itself uses — strong precedent for a catalog of this kind.
- Keeping the DB as sole source of truth (rather than XMP-as-truth) avoids a whole class of sync bugs where the sidecar and the DB disagree; XMP export becomes a one-directional, user-triggered "give me an interoperable copy of my edits" action, which is sufficient for interop with other tools without taking on bidirectional-sync complexity.
- `rusqlite` is a mature, well-maintained Rust binding, keeping the catalog engine inside the same Rust core as everything else in [ADR-0001](ADR-0001-application-shell.md).

## Consequences

- A relink/repair workflow is required from M1 onward for moved or offline volumes (PRD §7.1) — this is catalog-engine work, not optional polish, and should be scoped into M1, not deferred.
- Scheduled catalog backups (PRD §7.6, pulled into M2's scope per MILESTONES.md) are simple with a single-file SQLite DB — rotate timestamped copies on a schedule/on close.
- Since XMP is not the source of truth, a user who only ever looks at XMP sidecars (e.g., via another tool) will not see edits until they explicitly export/sync them — this is a deliberate scope boundary, not an oversight, and should be stated plainly in user-facing docs later.

## Alternatives considered and rejected

- **XMP-as-source-of-truth (read edits from sidecars, DB as pure cache)**: rejected — this is closer to what some competing tools do, but it reintroduces exactly the sync-conflict class of bugs SQLite-as-truth avoids, for no benefit given there's no multi-device/cloud sync in this product's scope at all (PRD §5).
- **A heavier embedded DB (e.g., an embedded document store)**: rejected — SQLite's relational model fits this data well (images, edit versions, collections, keywords are naturally relational), and adding a less common embedded DB has no clear upside here.
