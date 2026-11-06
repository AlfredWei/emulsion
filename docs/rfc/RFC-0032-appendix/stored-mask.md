# Proposal: store each removal's mask (RFC-0032 §12.2, ADR-0010 §5)

Status: **Proposed, 2026-11-07. Needs the owner's decision before any code** (it changes the catalog format).
Part of RFC-0032 slice E. Follows the RFC-before / ADR-update-after practice: if accepted, ADR-0010 §5 is updated in the same PR as the code.

## 1. What is missing, and why it matters

A finished removal is `{patch, engine}` in the `remove` op plus one row in `removal_patches` (the result, as a PNG, ADR-0010 §4). **The mask the user marked is not kept.** Three things the RFC promised need it:

| Promise | Where | Needs the mask because |
|---|---|---|
| **Heal instead** on a result that is wrong | RFC §3 / §4, mock | the Spot mask is built from the marked area; E4 can only do it for marks not yet removed |
| **Regenerate** a removal (try again; a newer model or engine) | ADR-0010 §5, RFC §10 | re-running LaMa needs the hole |
| **"Needs regenerating"** when a patch cannot be used | ADR-0010 §5, RFC §12.1 | same |

## 2. A finding that changes the third row

ADR-0010 §4 put the patch **inside the catalog database** so that backups, restores and catalog copies carry it. That was the right call, and it makes the *automatic* "needs regenerating" state nearly unreachable:

- A catalog copy or backup carries every row. The case the study (c2-patch-study.md, task 4) imagines, "copy the catalog to another computer and the patch file is missing", cannot happen with a blob.
- The one reachable case is a patch the pipeline **skips** because the source file was replaced with one of another aspect ratio (RFC §12.1). Then the stored mask is wrong for the new file too, so regenerating from it would be a guess. The honest state there is "this removal no longer fits the photograph", with *Delete* as the action.
- A missing row would need the unwired sweep (`delete_removal_patches_except`) or a damaged file.

So the value of storing the mask is **user-initiated**: *Try again* and *Heal instead* on a removal that is already in the picture. I propose to **re-scope slice E accordingly** (§5) and keep an automatic "needs regenerating" state only for the cases above.

## 3. What to store

**The rasterised hole of each separate thing, as a 1-bit PNG in the same window as the patch**, in new columns of the same row:

- `removal_patches.mask_png BLOB` (nullable): the mask `remove_object` fed to LaMa for this patch, cropped to the patch's `x, y, w, h` in the full decode. It is exactly what the engine consumed, so a re-run is the same job.
- Nullable so every removal made before this change keeps working as today (no *Try again* / *Heal instead* for those; the buttons are absent, not broken).
- Added by `ALTER TABLE ... ADD COLUMN` through the existing `add_column_if_missing` list, so existing catalogs migrate on open and a backup is still one SQLite file.
- The `remove` op is unchanged: ids, never pixels (§12.2). The earlier plan to give the *item* a `mask` field is dropped: the stack is rewritten on every slider change and copied into every history row, and a click-selected object's cut is tens of KB.

**Why the raster hole and not the user's marks (strokes, outlines, cuts):**

| | Raster hole | Marks |
|---|---|---|
| Re-run LaMa | direct | must be re-rasterised (same function, same result) |
| Heal instead | sample a grid of the hole into dabs: works for brush, lasso **and a selected object** (E4 refuses the last) | strokes and outlines only |
| Size | a few KB to tens of KB per patch at 1 bit (**estimate, not measured**) | strokes and outlines tiny; a selected object's cut larger |
| Edit the marks afterwards | no (not asked for) | yes |

Editing a finished removal's marks is not on any slice; if it ever is, the marks can be added next to the hole without touching this.

## 4. What it enables, in order

1. **Catalog + job (Rust):** `mask_png` column; `remove_object` stores each cluster's hole with its patch; a command `regenerate_removal(patch_id)` re-runs the engine on the stored hole and **adds a new patch** (the old row stays: History can bring the old result back, §12.2 *Undo*), swapping the item in the op for one edit "Redo removal".
2. **Try again (UI):** a per-removal action on the chips from E3. Says plainly that the result may differ (RFC §10: LaMa is deterministic on one machine and provider, so on the same machine it will be identical; the button matters after a model or provider change, so it should offer to run only when the engine id of the item differs from the installed one, and otherwise say "same result").
3. **Heal instead for a finished removal:** the hole sampled into dabs (`healFromMarks.js` gains a grid-from-mask planner, which also lets E4 accept a selected object); the removal is taken out of the op and the Spot mask added, one edit "Heal instead".
4. **"Needs regenerating" state:** only for an item whose row is missing. Shown on the chip with *Regenerate* if the row had a mask (it cannot, once the row is gone: so in practice this shows *Delete*). Kept for the sweep, if ever wired.

## 5. Decisions for the owner

1. **Accept the raster-hole design (§3)** over storing marks, or ask for marks too? *Recommended: raster hole only.*
2. **Re-scope "needs regenerating" (§2)** to user-initiated *Try again* and *Heal instead*, with the automatic state kept only for a missing row? *Recommended: yes.*
3. **Run the C2 study with people first?** Its task 4 assumes a missing file; after §2 it would test a case that cannot arise. *Recommended: rewrite task 4 as "the result is wrong: what do you do?" and run it after step 3 has a UI, or skip it.*
4. Old removals without a stored hole: **no Try again / Heal instead for them** (recommended), or a one-time prompt that re-marks? *Recommended: none; there are no users with such removals yet.*

## 6. Not verified

- The size of a stored hole (needs a measurement over the real removals in a catalog).
- That a re-run from the stored hole reproduces the stored patch bit for bit on this machine (RFC §10 measured the engine, not this path). Step 1 should assert it in a test with the real model.
- How the hole samples into dabs on a real selected object (E4's grid planner is tested on polygons and strokes only).
