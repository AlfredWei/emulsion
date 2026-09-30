# RFC-0020: Grain's integer hash — bit-identical CPU/GPU noise (M5.6's tenth effect)

- Status: Draft — for review in PR (flips to Accepted once merged)
- Date: 2026-09-30
- Companion documents: [PRD/MILESTONES §M5.6](../../PRD/MILESTONES.md#m56--develop-effects-quality--performance-research), [develop_engine/effects.rs](../../app/src-tauri/src/develop_engine/effects.rs), [gpu/shaders/gradeUniforms.js](../../app/src/lib/gpu/shaders/gradeUniforms.js), [e2e/specs/develop-cpu-gpu-parity.e2e.js](../../app/e2e/specs/develop-cpu-gpu-parity.e2e.js), [PROGRESS.md](../../PROGRESS.md)

## 0. Process note

Self-chosen "next" pick from the "not yet researched effects" list. Tone Curve was read first and set aside: it is a Fritsch–Carlson monotone cubic with a shared 256-entry LUT on both sides, with nothing to fix. Grain turned up a defect that is **measured, not argued**: the CPU and GPU implementations do not produce the same grain, despite doc comments in both files saying they agree to within the parity bar.

## 1. Problem

Grain is a procedural noise overlay. Both `develop_engine/effects.rs` (`grain_hash`) and `gradeUniforms.js` (`grainHash`) compute its per-lattice-cell random value as the classic shader hash:

```
fract( sin(dot(p, (12.9898, 78.233))) * 43758.5453123 )
```

The doc comments on both sides justify using it with "not bit-exact across Rust's `f32::sin` and WGSL's own `sin`... close enough for this module's established ±2/255 parity bar." That justification is wrong, because of the `* 43758.5453123` step: it multiplies whatever error `sin` has by ~44,000 before `fract` discards the integer part. A sin error of just `2×10⁻⁵` becomes a hash error of ~1.0 — i.e. an entirely different random number.

**Measured on this machine's WebGPU adapter (Apple, Metal 3), evaluating the exact WGSL `grainHash` expression on random integer lattice coordinates and comparing against an f32-accurate CPU reference:**

| Max coordinate | GPU `sin()` abs. error, median / p99 | Implied hash error at p99 (`error × 43758`) | Median CPU↔GPU hash difference |
|---|---|---|---|
| ≤ 2 | 1.8×10⁻⁶ / 5.2×10⁻⁶ | 0.23 | — |
| ≤ 16 | 1.2×10⁻⁵ / 8.1×10⁻⁵ | 3.5 | **0.23** |
| ≤ 256 | 1.9×10⁻⁴ / 1.3×10⁻³ | 58 | **0.25** |
| ≤ 4096 | 3.0×10⁻³ / 2.1×10⁻² | 917 | **0.25** |

A median difference of **0.25 is exactly what two independent uniform random numbers give** (mean wrapped |a−b| = 0.25). So from coordinates as small as 16 pixels, the CPU and GPU grain fields are effectively **uncorrelated**. A second, independent cause compounds this at real image sizes: the hash argument itself (`x*12.9898 + y*78.233`) reaches ~3×10⁵ at 4096 px, where an f32 has a spacing of ~0.03 rad, and the CPU (separate multiply and add) and GPU (typically fused multiply-add) round it differently — the measured maximum argument difference at coordinates ≤ 4096 was `0.031`. Even a perfect `sin` could not save the hash from that, since `d(hash)/d(arg) ≈ 43758·cos(arg)`.

WGSL does not promise better: the specification permits `sin` absolute error of `2⁻¹¹` on `[-π, π]` and leaves larger arguments implementation-defined. So this is not one GPU's quirk; the result also **varies between GPUs, drivers, and operating systems**.

**User-visible consequences:**

1. **Preview ≠ export.** The interactive preview (GPU) and export / thumbnails / the GPU-fallback preview (CPU) draw *different* grain patterns from the same Amount/Size/Roughness. At Amount=100 the grain amplitude is `±0.12` (≈ ±30/255), so two independent fields differ by roughly ±10/255 pixel-by-pixel.
2. **Not reproducible across machines.** The same photo and settings show a different grain pattern on different GPUs.
3. **The parity net cannot see it.** `develop-cpu-gpu-parity.e2e.js` has scenarios for Exposure, Contrast, White Balance, HSL, Tone Curve, and Split Toning — none for Grain — so nothing has ever compared the two grain implementations.

## 2. Non-goals

- **Amount/Size/Roughness semantics and the `GRAIN_STRENGTH`/`GRAIN_MAX_CELL_PX` constants are untouched**, as are the value-noise interpolation, the smooth↔blocky Roughness blend, and the "additive luminance delta added equally to all three channels" shape.
- **Named but not fixed: Roughness also changes amplitude.** The smooth end (Roughness 0) is bilinear-smoothstep-interpolated value noise, whose standard deviation is `0.2195` for a uniform lattice — 24% lower than the raw blocky end's `0.2887`. So sliding Roughness up also makes the grain visibly stronger. Normalizing that is a tuning question independent of this RFC's correctness fix, and is a candidate for its own slice.
- **Named but not fixed: additive grain at pure black/white.** A uniform additive delta is one-sidedly clipped at 0/1, slightly lifting pure blacks and darkening pure whites; real grain is also weaker at the tonal extremes. Separate concern, not folded in.
- **Resolution scaling is unchanged**: `size` stays a fixed absolute pixel scale (the "named limitation" Dehaze/Texture/Clarity already accept), so preview and full-resolution export still differ in *scale*; this RFC only makes the pattern *at a given pixel grid* identical.
- **Grain remains unseeded** — same pattern for every image, as today.

## 3. Research finding: an integer hash is exact by construction

The hash's input is already integer-valued — `grain_value_noise` evaluates it only at `floor()`ed lattice coordinates. There is no need for a floating-point transcendental at all. Use an integer avalanche hash on `u32`, whose arithmetic (wrapping multiply, xor, shift) is **defined bit-for-bit identically** by Rust (`wrapping_mul`) and WGSL (`u32` arithmetic wraps by specification):

```
lowbias32(x):            // Chris Wellons' well-tested 32-bit avalanche
  x ^= x >> 16;  x *= 0x7feb352d
  x ^= x >> 15;  x *= 0x846ca68b
  x ^= x >> 16

grain_hash(ix, iy) = ( lowbias32( u32(ix) ^ lowbias32( u32(iy) ^ 0x9E3779B9 ) ) >> 8 ) / 2²⁴
```

The `>> 8` keeps 24 bits and `/ 2²⁴` maps them to `[0, 1)`; a 24-bit integer is exactly representable in f32, so the returned value is **bit-identical on CPU and GPU**. The rest of the noise pipeline (bilinear/smoothstep interpolation, Roughness blend) uses only f32 `+ − ×`, which agree to within 1 ulp (~10⁻⁷ × 0.12 amplitude ≈ 10⁻⁸ — far below one 8-bit step) even if the GPU contracts to FMA.

**Quality**, computed over a 512×512 lattice (not assumed): mean `0.5006`, standard deviation `0.2885` (ideal uniform: `0.5`, `0.2887`), and lag-1 correlation between horizontally/vertically adjacent cells `−0.0009` / `−0.0002` — i.e. uniform and uncorrelated, no visible lattice structure. Golden values (computed independently in Python, to be asserted in both Rust and WGSL):

| `(ix, iy)` | `u32` hash | `grain_hash` |
|---|---|---|
| (0, 0) | `0xae6f80f1` | 0.6813888549804688 |
| (1, 0) | `0xa07c7a97` | 0.6268993616104126 |
| (0, 1) | `0x8e374fe0` | 0.5555314421653748 |
| (1, 1) | `0xa290702b` | 0.6350164413452148 |
| (17, 42) | `0xf50c661e` | 0.9572204351425171 |
| (4095, 3071) | `0x30ed22ee` | 0.19111835956573486 |
| (123456, 654321) | `0x4cdf1783` | 0.30027908086776733 |

**A real, named consequence:** the *specific* random pattern for any given Amount/Size/Roughness changes (any different hash gives a different field). Grain is procedural and unseeded, so no saved edit depends on a particular pattern — the stored parameters mean exactly what they did, and only the (arbitrary) noise realization changes. Statistically the grain looks the same.

## 4. Design — CPU (Rust)

`effects.rs`: `grain_hash(x: f32, y: f32) -> f32` keeps its signature (callers pass already-`floor()`ed lattice coordinates as f32) and its body becomes the integer hash above, converting via `(x as i32) as u32` (a bit-reinterpretation, matching WGSL's `u32(i32)`). A private `lowbias32(u32) -> u32` helper uses `wrapping_mul`. `grain_value_noise`, `grain_delta`, and everything else are unchanged. The doc comment on `grain_hash` is rewritten: it currently explains why a `sin` hash is acceptable despite not being bit-exact; it will say why an integer hash is exact instead.

## 5. Design — GPU (WGSL)

`gradeUniforms.js`: `grainHash(p: vec2<f32>)` keeps its signature and becomes the same integer hash, with `lowbias32(x: u32) -> u32` alongside it and `u32(i32(p.x))` conversions. `grainValueNoise`/`grainDelta` are unchanged. The comment claiming the Rust twin is "not bit-exact" is replaced with the exactness argument. No new uniform, binding, or pass.

## 6. Testability

- **Golden vectors on both sides**: the seven `(ix, iy)` rows above asserted in a Rust unit test, **and** executed on a real GPU in the browser against the actual `grainHash` WGSL — required to match the Rust values exactly (equal, not "within tolerance"), the direct proof of the "bit-identical by construction" claim.
- **Distribution**: over a fixed lattice, mean within `0.5 ± 0.01`, standard deviation within `0.2887 ± 0.01`, and |lag-1 correlation| < 0.01 in both axes — replaces the old `grain_hash_at_origin_is_exactly_zero` test, which asserted a property of the `sin` hash (`0 × 43758 = 0`) that the new hash deliberately does not have (`(0,0)` now hashes to a normal value, removing an unnatural fixed point at the image corner).
- **Existing behavior preserved**: `amount == 0` is still an exact passthrough; `grain_delta` is still deterministic for a repeated coordinate; Size still groups pixels into matching blocky cells at Roughness 100; Roughness 0 and 100 still generally differ — all existing grain tests must pass unchanged apart from the origin-hash one above.
- **End-to-end CPU↔GPU parity, the test that never existed**: a new `develop-cpu-gpu-parity.e2e.js` scenario (Grain Amount 60, Size 25, Roughness 50, at the existing flat `ROAD_PATCH`), asserting CPU and GPU agree within the file's standard tolerance. Before this RFC such a scenario would fail by roughly the grain amplitude; after it, only 8-bit quantization can separate them.
- **A direct numeric browser check of the whole `grainDelta`** (not just the hash) at several coordinates and parameter settings against the Rust value, since the hash being exact does not by itself prove the interpolation port.

## 7. Exit criteria (this slice)

- Integer hash implemented in Rust and WGSL; golden vectors asserted in Rust and reproduced **exactly** on a real GPU.
- Distribution/uncorrelation tests pass; all other existing grain tests pass unchanged.
- New Grain CPU/GPU parity e2e scenario added (run by CI).
- PROGRESS.md records the measurement table from §1 (so the finding stands on numbers), the corrected doc comments, the changed-noise-realization consequence, and the two named-but-not-fixed items (Roughness amplitude coupling; additive grain at black/white).
