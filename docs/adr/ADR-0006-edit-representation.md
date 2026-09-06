# ADR-0006: Non-destructive edit representation — versioned JSON edit stack

- Status: Accepted
- Date: 2026-07-25
- Relates to: [ADR-0004](ADR-0004-rendering-and-color-management.md), [ADR-0005](ADR-0005-catalog-storage.md), [PRD §7.4](../../PRD/PRD.md#74-develop-edit)

## Context

PRD §7.4 requires a fully non-destructive, reversible edit stack per image, with history/undo and snapshots, and virtual-copy awareness (each virtual copy has its own independent edit stack). The edit representation also needs to grow forward without breaking old catalogs: M2 adds masks, M5 adds AI-generated masks, M6 adds generative operations — none of that exists at M1, but the schema chosen now has to be able to absorb it later.

## Decision

Represent each image-version's edits as a **versioned, JSON-serializable "edit stack"**: an ordered list of typed operation records (e.g., `{op: "exposure", value: 0.5}`, later `{op: "brush_mask", ...}`), stored as a JSON blob column per image-version row in the SQLite catalog ([ADR-0005](ADR-0005-catalog-storage.md)), and replayed in order against the decoded linear buffer by the rendering pipeline ([ADR-0004](ADR-0004-rendering-and-color-management.md)) to produce the current preview/output. Every stack carries a `schema_version` field.

## Rationale

- **Replay-ability is what makes this non-destructive**: the stack is instructions, never baked pixels, matching the PRD's core non-negotiable (§7.1, §7.4) and Lightroom's own foundational architecture (see lightroom-reference.md, Era 1).
- **JSON in a DB column, not a bespoke binary format**: keeps the format human-inspectable (useful for debugging and for the XMP export step in ADR-0005), and avoids building/maintaining a custom binary (de)serializer for something that isn't a performance bottleneck (the stack itself is tiny; it's the *rendering* of it that must be fast, which is ADR-0004's job, not this one's).
- **`schema_version` per stack, not per catalog**: lets old image-versions keep working after the app adds new operation types in M3/M6/M7 — new code just needs to know how to render every `op` type it encounters, and can reject/flag stacks referencing unknown future op types gracefully rather than crashing.
- History/undo and snapshots (PRD §7.4) fall out of this representation almost for free: history is just prior states of the same stack; a snapshot is a named, retained copy of a stack at a point in time.

## Consequences

- Every new Develop capability (masks in M3, AI masks in M6, generative ops in M7) must be added as a new, additive `op` type with its own schema-versioned shape — this is a real ongoing constraint on how those milestones implement features, not just a data-modeling detail.
- The renderer (ADR-0004's shader pipeline) needs to be designed from M1 as "interpret a list of typed ops," not hardcoded to a fixed small set of global sliders — this was already called out as a consequence in ADR-0004 and is reinforced here.
- Virtual copies (PRD §7.3) are implemented as distinct image-version rows each owning their own independent edit stack, sharing the same underlying source file reference.

## Update — HDR merge's output is decode-equivalent, not an `op` (2026-09-05, M5, RFC-0003)

HDR merge (RFC-0003) combines multiple *source images* into one new pixel buffer via alignment + radiometric averaging + tone mapping — fundamentally different from every `op` this ADR governs, which each transform one image-version's *own* single decoded buffer. A merge result is modeled as a **brand-new cataloged image** (its own `images`/`image_versions` rows, own empty edit stack ready for its own future Develop edits), not as a new `op` type appended to any existing stack, and not as a virtual copy of any of its source images — this is a genuinely new relationship (see the DB-schema update below), not a fit for either of the two per-image-version constructs this ADR/ADR-0005 already have.

Concretely: the merge's output buffer plays the same role in the rendering pipeline that a fresh RAW/JPEG decode does — it is what `op` replay starts *from*, never itself expressed as one of the ops being replayed. This is a real, deliberate scope boundary: it means a merge cannot later be "edited" by changing its inputs' alignment or exposure weighting after the fact (RFC-0003 §2's own named non-goal) — the merge is a one-time bake to a new source-equivalent buffer, matching how a real Lightroom-class tool's own HDR merge feature works (produces a new raw-like file, not a live composite).

**New provenance-only table**, `hdr_merge_sources` (`catalog.rs`'s `migrate()`, RFC-0003 §3.6): `(result_image_id, source_image_id, ordinal, ev_offset, dx, dy)`, one row per bracket member. This is explicitly **never consulted by the render/replay path** this ADR governs — it exists purely so the catalog can answer "what fed this merge, in what order, with what computed alignment/EV" for provenance/debugging purposes, the same way EXIF answers "what camera settings produced this file" without the renderer ever reading EXIF mid-render. Confirmed this is new modeling, not a repurposing of an existing relationship: `images.stack_id` (mentioned in ADR-0005/M1 schema notes) was checked and confirmed unused/unimplemented anywhere in the codebase; `image_versions.is_virtual_copy` means "multiple edit stacks over one file" — the opposite relationship from "one new file derived from several."

## Update — Panorama merge's output is decode-equivalent too, same shape as HDR merge (2026-09-06, M5, RFC-0004)

Panorama merge (RFC-0004) extends the exact statement the update above already makes for HDR merge to a second merge feature: it also combines multiple *source images* (here, via feature matching + homography estimation + a warped/blended composite, not radiometric averaging) into one new pixel buffer, and that result is modeled the same way — a **brand-new cataloged image** with its own empty edit stack, never expressed as an `op`, never a virtual copy of any source. The same scope boundary applies: a panorama cannot later be "restitched" with different frame ordering or blend weighting after the fact (RFC-0004 §2's own named non-goal) — it is a one-time bake to a new source-equivalent buffer, exactly like HDR merge's own one-time bake.

**New provenance-only table**, `panorama_merge_sources` (`catalog.rs`'s schema, RFC-0004 §3.6): `(result_image_id, source_image_id, ordinal, homography_json)`, one row per stitched frame — `homography_json` a JSON array of the 9 row-major matrix values mapping that frame into the reference frame's coordinate system. Same "never consulted by the render/replay path" framing as `hdr_merge_sources`; stored as an opaque JSON blob rather than separate columns because (unlike HDR's `dx`/`dy`/`ev_offset`) nothing else in the catalog ever queries into individual matrix cells.

## Alternatives considered and rejected

- **Baked/destructive edits (modify pixels directly)**: rejected outright — violates the PRD's core non-destructive requirement (§7.1, §7.4), not a real option.
- **Custom binary edit-stack format**: rejected — no performance need justifies the added complexity of a bespoke binary (de)serializer and its own versioning/migration tooling, when JSON does the job and is easier to debug/export.
- **XMP as the primary edit-stack storage** (rather than the DB): rejected here for the same reasons as ADR-0005 — XMP is kept as an export/interchange format, not the working representation the renderer reads from.
