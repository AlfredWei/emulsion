# RFC-0019: HSL / Color Mixer band centers at real hues (M5.6's ninth effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-30
- Companion documents: [PRD/MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [develop_engine/hsl_split.rs](../../app/src-tauri/src/develop_engine/hsl_split.rs), [gpu/shaders/gradeMath.js](../../app/src/lib/gpu/shaders/gradeMath.js), [api/develop.js](../../app/src/lib/api/develop.js), [RFC-0018](RFC-0018-parametric-tone-hue-preserving-clamp.md) (corrected in §8 below), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Self-chosen "next" pick from the "not yet researched effects" list (tone curve, HSL/color mixer, split toning, grain, lens corrections). HSL was picked because reading `hsl_split.rs` turned up a defect that is checkable from the app's own UI, not a matter of taste: the panel's "Yellow" swatch is drawn with `hsl(90°, 70%, 50%)`, which is chartreuse. Split Toning (whose math is sound) is deliberately left alone. While tracing the pipeline for this RFC, an imprecision in RFC-0018 also surfaced; it is corrected in §8 rather than left standing.

## 1. Problem

The Color Mixer's eight bands sit at evenly spaced hue-circle positions, `[0, 45, 90, 135, 180, 225, 270, 315]` (`HSL_BAND_CENTERS_DEG` in `develop.js`, `HSL_BAND_CENTERS_DEG` in `hsl_split.rs`, `HSL_BAND_CENTERS` in `gradeMath.js`). Even spacing was chosen so that at most two raised-cosine weights are nonzero at any hue and they sum to exactly 1 with no renormalization. The cost is that the band *names* no longer sit where the colors they name actually are on the HSL hue circle:

| Band | Center today | Where that color actually is | Error |
|---|---|---|---|
| Orange | 45° | ~30° | 15° |
| **Yellow** | **90°** | **60°** | **30°** (the swatch itself is chartreuse) |
| Green | 135° | 120° | 15° |
| Blue | 225° | 240° | 15° |
| Magenta | 315° | ~300° | 15° |

The user-visible consequence, computed from the existing `hsl_band_weight` (not estimated): a **pure yellow pixel (hue 60°) responds 75% to the Orange sliders and only 25% to the Yellow sliders**. Pure orange (30°) is 25% Red / 75% Orange; pure green (120°) is 25% Yellow / 75% Green; pure blue (240°) is 75% Blue / 25% Purple. A user who drags "Yellow" to fix a yellow flower mostly moves greens instead, and the eyedropper's band-jump (`nearestHslBand`) will name a chartreuse pixel "yellow" and a true yellow "orange".

## 2. Non-goals

- **The band count (8) and names** stay as they are (matching Lightroom's own eight).
- **Everything downstream of the weights is untouched**: hue/saturation/luminance delta accumulation, the `chroma_fade` near-gray suppression, the HSL-lightness-based luminance shift, and HSL→RGB reconstruction. Those are separate concerns (e.g. HSL "lightness" is not perceptual luma, so a luminance shift moves blue and yellow by very different perceived amounts — a candidate for its own future slice, not folded in).
- **Split Toning, Tone Curve, Grain, Lens Corrections** are not touched.
- **No claim to match Adobe's exact centers.** Lightroom's band centers are not published. The values chosen here are the hue positions of the named colors on the standard HSL circle (with Purple/Magenta placed where commonly observed), i.e. a reasoned choice, not a verified reproduction — same honesty class as RFC-0016's Roundness and RFC-0017's Contrast curve.

## 3. Research finding

### 3.1 New centers

```
red 0°, orange 30°, yellow 60°, green 120°, aqua 180°, blue 240°, purple 270°, magenta 300°
```

Gaps between neighbors are now unequal (30°, 30°, 60°, 60°, 60°, 30°, 30°, 60°), which is exactly what makes the old "one shared 45° half-width" weight function unusable: with unequal gaps a fixed half-width either leaves gaps with total weight < 1 or overlaps with total weight > 1.

### 3.2 New weight function: piecewise raised-cosine between *adjacent* centers

Instead of a symmetric window around each center, each band's weight rises from its previous neighbor and falls to its next neighbor, using each side's *own* gap:

```
d = wrap(hue - center_i) into (-180, 180]
if d >= 0:  gap = next_center - center_i  (cyclic);  w = d < gap ? 0.5*(1 + cos(pi * d / gap)) : 0
if d <  0:  gap = center_i - prev_center  (cyclic);  w = -d < gap ? 0.5*(1 + cos(pi * |d| / gap)) : 0
```

Properties, all by construction rather than by tuning:

1. **Partition of unity.** Between adjacent centers `a` and `b`, `w_a = 0.5(1+cos(πt))` and `w_b = 0.5(1+cos(π(1-t))) = 1 - w_a`, so the two nonzero weights sum to exactly 1 for any gap size; no other band is nonzero there. No renormalization, no gaps, no double-counting.
2. **C1-continuous** everywhere (zero slope at every center and every crossover), preserving the reason the raised cosine was chosen over a triangular ramp originally.
3. **Exact isolation at a center.** At a band's own center its weight is exactly 1 and every other band's is 0.
4. **Eyedropper agreement.** Within an interval, `w_a > 0.5` iff `t < 0.5` iff the hue is nearer (in degrees) to `a`, so the highest-weight band and the nearest-center band are always the same — `nearestHslBand` stays exact, unchanged in logic (only its doc comment, which claims even spacing, needs updating).

### 3.3 Worked example (checked numerically, not just argued)

| Hue | Old weights | New weights |
|---|---|---|
| 30° (orange) | red 0.25, orange 0.75 | orange 1.0 |
| 60° (yellow) | orange 0.75, yellow 0.25 | yellow 1.0 |
| 120° (green) | yellow 0.25, green 0.75 | green 1.0 |
| 240° (blue) | blue 0.75, purple 0.25 | blue 1.0 |
| 45° (orange/yellow midpoint) | orange 1.0 | orange 0.5, yellow 0.5 |

A numeric sweep of the new function over the full hue circle in 0.25° steps gives a maximum `|Σw − 1|` of `2.2e-16` (float rounding), and zero disagreements between argmax-weight and nearest-center.

### 3.4 A real, named consequence

Slider *names* are unchanged, but *which pixels each slider affects* moves. Any existing HSL edit stored in a catalog (and any HSL values inside saved presets) will re-render differently after this change — e.g. a "Yellow +40 saturation" that used to mostly boost chartreuse now boosts real yellow. This is the fix working as intended (the stored intent was "make yellows more saturated"), not a regression to compensate for, and no schema migration is attempted: there is no faithful way to translate a stored edit from the old hue mapping to the new one, and the app is pre-release with a local-only catalog.

## 4. Design — CPU (Rust)

`hsl_split.rs`: `HSL_BAND_CENTERS_DEG` becomes the new array; `hsl_band_weight(hue_deg, center_deg)` is replaced by `hsl_band_weight(hue_deg, band_index)` implementing §3.2 using the cyclic previous/next entries of `HSL_BAND_CENTERS_DEG`. `apply_hsl_bands` passes the band index instead of the center. Its doc comment's "centers exactly 45 degrees apart, so at most 2 bands are nonzero" is rewritten to the adjacent-interval argument in §3.2.

## 5. Design — GPU (WGSL) and JS

- `gradeMath.js`: `HSL_BAND_CENTERS` becomes the new array; `hueBandWeight(hueDeg, centerDeg)` becomes `hueBandWeight(hueDeg, i)` with the same two-sided-gap logic (indexing the constant array at `(i+1)%8` and `(i+7)%8`); the loop in `applyHslBands` passes `i`.
- `develop.js`: `HSL_BAND_CENTERS_DEG` becomes the new array (this also fixes `DevelopPanel.svelte`'s swatches, which render `hsl({center}, 70%, 50%)`, so the Yellow swatch becomes an actual yellow with no markup change). `nearestHslBand` is unchanged; its doc comment is updated.
- No new uniform, binding, pass, or bind group.

## 6. Testability

- **Exact isolation at every center**: for each of the 8 bands, `hsl_band_weight(center_i, i) == 1` and every other band's weight at that hue is 0 (within float tolerance). This replaces the old center-isolation test's implicit 45° assumption.
- **Partition of unity across the whole circle**: over a dense hue sweep, the eight weights sum to 1 within float tolerance, and at most two are nonzero.
- **The user-visible regression, stated directly**: at hue 60° the Yellow band's weight is 1 and Orange's is 0 (old: 0.25 / 0.75); at hue 120° Green is 1 (old 0.75).
- **Midpoint blends**: at hue 45° Orange and Yellow are each 0.5; at hue 90° Yellow and Green each 0.5 (unequal-gap case: `t = 0.5` of a 60° gap) — the direct check that the two-sided-gap logic is right where gaps are unequal.
- **Wraparound**: hues just below 360° and just above 0° blend Magenta and Red correctly across the 300→360 gap (the cyclic-neighbor edge case).
- **Existing tests updated, not deleted**: `hsl_single_band_at_its_own_center_isolates_that_band` and `hsl_boundary_hue_blends_both_neighboring_bands` encode the 45° spacing and get new hues/values with doc comments; `nearestHslBand`'s JS tests get the new expected band for hues that changed owner.
- **CPU/GPU parity**: the existing HSL green-band e2e scenario (`GREEN_PATCH`, hue ≈ 96°) sits between old-Yellow/old-Green and is now a Yellow/Green *blend* under the new centers — it should still agree between CPU and GPU (that is what the test checks) but exercises the two-sided weight in a genuinely blended region. Verified additionally by a direct numeric browser check of the WGSL weight function at the §3.3 hues against the Rust values.

## 7. Exit criteria (this slice)

- New centers and the adjacent-interval weight function implemented in Rust, WGSL, and JS with the tests in §6 passing, including the unequal-gap midpoint and the 360°→0° wraparound.
- Every existing HSL test that encoded the 45° spacing updated with a derived value and an explanatory comment; none deleted or loosened.
- The Yellow swatch renders as yellow (`hsl(60°…)`), confirmed in the dev-server browser.
- PROGRESS.md records the slice, including the §3.4 re-render consequence for existing HSL edits and the "not a verified match to Adobe's centers" caveat.

## 8. Correction to RFC-0018 (found while tracing the pipeline for this RFC)

RFC-0018 §1/§2 (and the doc comment added to `apply_parametric_tone`) say the pipeline "clamps to `[0,1]` exactly once, at the very final byte conversion (`pipeline.rs`)". That is imprecise. Tracing the per-pixel loop for this RFC: right after `apply_global_adjustments`, `pipeline.rs` unconditionally runs `sample_lut(&curve_lut, rgb[c])` on each channel, and `sample_lut` clamps its input to `[0,1]` (`v.clamp(0.0, 1.0)`); the WGSL twin `sampleCurveLut` does the same. So the **first per-channel hard clamp is the Tone Curve stage, immediately after Saturation** — before HSL and Split Toning — and the byte-conversion clamp is a second one. The chain from WB through Saturation is unclamped float; everything from the Tone Curve onward sees values already in `[0,1]`.

RFC-0018's *fix and its correctness are unaffected*: the asymmetry mechanism is identical (one channel truncated by the per-channel clamp while its siblings still carry the full delta), it just occurs at the Tone Curve stage rather than at the last byte conversion, and the shared-scale delta still keeps every channel inside `[0,1]` so that clamp is a no-op for Parametric Tone's own contribution. Only the *location* of the clamp was misdescribed. This RFC's implementation PR corrects RFC-0018's wording and the `apply_parametric_tone` doc comment. It is stated here, rather than silently patched, following this project's "corrected during implementation" practice.
