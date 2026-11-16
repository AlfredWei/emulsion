# RFC-0039: Replace the `lensfun` crate with clean-room code and ship the lens database as a data file

- **Status.** Draft for decision, written 2026-11-16 against `main` at `b134c06`. Gated on RFC-0038 D1 and D5 (section 2). Engineering
  planning, not legal advice. Every statement about what a licence requires is a question for counsel (section 9).
- **Companion documents.** [RFC-0038](RFC-0038-lgpl-and-database-licence-plan.md) (section 1 decisions D1 to D6, step B5.3, its scope
  note and contingency, section 4 questions), [RFC-0021](RFC-0021-lens-correction-direction.md) (the lens kernels and their oracle test),
  [RFC-0024](RFC-0024-develop-open-latency.md) (database load time), [PROGRESS.md](../../PROGRESS.md) (entry "Lens matching pinned by
  a golden fixture"), [fixture_coverage.py](RFC-0039-appendix/fixture_coverage.py) (regenerates every fixture count below).
- **Labels.** "Measured" means read or run in this session at `b134c06`. "Inferred" means reasoned from what was read. "Guess" is a guess.

## 1. Goal

RFC-0038 step B5.3 says to replace the LGPL-3.0 `lensfun` crate with clean-room code and to keep the CC-BY-SA lens database as a
separate data file. It says the step "gets its own RFC first". The owner asked for that RFC, so the work can be priced, decided and
later built without anyone re-deriving it. This is that RFC.

A person editing photos should notice nothing. The same EXIF gives the same profile and the same coefficients, and the
`LensProfileMatch` JSON stored in edit stacks keeps its shape, so no catalog migration is needed (inferred from section 4.3). The next
maintainer inherits a lens reader the project owns, and a database update becomes a file copy plus a deliberate fixture regeneration
instead of a crate bump.

## 2. D1 and D5 decide whether to build it

RFC-0038 B5 applies only when D1 is proprietary or paid ("only then", its heading says). This RFC follows that gate.

| Answer | What it means for this RFC |
| --- | --- |
| D1 (a), public and MIT, source on GitHub | Do not build. RFC-0038 Phase B covers the LGPL-3.0 terms for an open-source release (inferred from RFC-0038 section 3). |
| D1 (c), named testers of the MIT build | Same as (a). |
| D1 (b), proprietary or paid | Build, once D5 is answered. |
| D5, the database may ship unmodified as a separate file with attribution | Build as written. |
| D5, it may ship only under conditions on form or attribution | Step 2 changes to meet them. The rest is unchanged. |
| D5, it cannot ship beside a proprietary binary | The "B5.3 contingency" paragraph of [RFC-0038](RFC-0038-lgpl-and-database-licence-plan.md) section 3 applies (another data source). Steps 2 and 4 of this RFC are void. The matcher and interpolation may carry over if the new source has the same shape (guess). |

Step 1 (extend the oracle) is useful under every answer, because it also guards a crate upgrade. It can start before the gate.
Removing the crate also removes the crate's share of D6, the LGPL-3.0 "Installation Information" question (RFC-0038 B5.6 says so).

## 3. What the code shows today

Measured by reading the files at `b134c06`.

- **The dependency.** `lensfun = "0.7.0"` in `app/src-tauri/Cargo.toml` (line 37). `roxmltree` is already in the build through the
  crate's own dependencies (its NOTICE lists it).
- **Production callers.** Only `lens_profile.rs` imports crate types. The `lookup_lens_profile` command (`lib.rs`, around line 1370)
  runs `match_profile` on the blocking pool, and startup calls `warm_lens_db` (around line 2334). The frontend calls the command from
  `app/src/lib/api/develop.js`.
- **Test callers.** `develop_engine/tests/geometry.rs` has `profile_mapping_equals_the_upstream_lensfun_correction`, which builds
  `lensfun::Modifier` at runtime as its oracle. The golden fixture's generator (`golden_inputs`, `regenerate_the_lens_profile_golden`
  in `lens_profile.rs`) walks `lensfun::Database`. Neither can run once the crate is gone.
- **The data.** The crate's `data/db/` holds 56 XML files, a DTD, an XSD, a README and `timestamp.txt` (60 entries, 5.0 MB), with
  1,041 cameras and 1,543 lenses, all `version="2"`. RFC-0038 section 0 records that the crate gzips and embeds them.
- **Load cost.** 42 ms in a release build and about 440 ms in a dev build (RFC-0024, quoted in `lens_profile.rs`; not re-measured).
- **LGPL-derived code in the app's own files.** RFC-0038's scope note names the three rescale functions. There are four more regions,
  each described by its own header as a port of lensfun:
  1. `lens_profile.rs` lines 171 to 248: `rescale_distortion`, `rescale_tca`, `rescale_vignetting` ("a direct port").
  2. `develop_engine/lens.rs` lines 36 to 126: `lens_undist_poly3` (a Newton inverse that Manual Distortion still uses),
     `lens_dist_poly3`, `lens_dist_poly5`, `lens_dist_ptlens` and `lens_tca_poly3_forward` ("a direct, hand-verified port").
  3. `develop_engine/lens.rs` lines 289 to 316: `LensNorm::for_profile` ("Port of `Modifier::new`'s own math").
  4. `app/src/lib/gpu/shaders/lens.js` lines 71 to 135: the WGSL twins of regions 2 and 3 (`lensUndistPoly3`, `lensDistPoly3`,
     `lensDistPoly5`, `lensDistPtlens`, `lensTcaPoly3Forward`, `lensNormForProfile`).
  5. RFC-0021 sections 3.1, 3.2 and Appendix A, which transcribe the forward formulas from the crate's functions.

Whether each region is a derivative work in law is for counsel (questions Q4 and Q5). For engineering, all five are treated as tainted.

## 4. Data shapes and the module boundary

### 4.1 The query (unchanged)

`match_profile(camera_make: Option<&str>, camera_model: Option<&str>, lens_model: Option<&str>, focal_length: Option<f32>,
aperture: Option<f32>) -> Option<LensProfileMatch>` keeps its signature. Its boundary rules stay as the fixture pins them. A blank camera
model, a blank lens model, or a focal length that is missing or not above zero gives `None`. A missing or non-positive aperture gives no
vignetting. Inside, the arguments are parsed once into a query with a non-empty camera model, a non-empty lens model, a positive focal
length and an optional positive aperture. Subject distance stays the app constant `DEFAULT_SUBJECT_DISTANCE_M` (1000 m).

### 4.2 The parsed database

This is a shape, not code. Field names follow the XSD.

```
LensDb      { mounts: [Mount], cameras: [Camera], lenses: [Lens] }        (file order kept, so the first match wins)
Mount       { name: Names, compat: [String] }
Camera      { maker: Names, model: Names, variant: Option<String>, mount: String, crop_factor: f32 }
Lens        { maker: Names, model: Names, mounts: [String], crop_factor: f32, aspect_ratio: f32, kind: LensKind,
	          distortion: [DistortionSample], tca: [TcaSample], vignetting: [VignettingSample] }
Names       { default: String, localized: [(lang, String)] }
DistortionSample { focal: f32, real_focal: Option<f32>, model: Poly3 { k1 } | Poly5 { k1, k2 } | Ptlens { a, b, c } }
TcaSample        { focal: f32, model: Linear { kr, kb } | Poly3 { vr, vb, cr, cb, br, bb } }
VignettingSample { focal: f32, aperture: f32, distance: f32, model: Pa { k1, k2, k3 } }
```

`aspect-ratio` is a decimal or a `w:h` string (XSD). `focal` and `aperture` on a lens are a value or a min and max. A sample with an
unknown model name is skipped. As today, a file that does not parse means no database and no profiles (`lens_profile.rs` lines 24 to 36).
File order is part of the contract. PROGRESS records that taking the last camera match instead of the first fails the fixture.

### 4.3 The result (unchanged)

`LensProfileMatch` keeps its serde form and every field, which are `camera`, `lens`, `crop_factor`, `real_focal`, `lens_center_x`,
`lens_center_y`, and the optional `distortion`, `tca` and `vignetting` coefficients. The frontend bakes it into the `lens_correction`
op. Develop re-resolves the profile on every open and rewrites the stored one when it differs (RFC-0021, "Corrected during
implementation"). So any drift from today's output silently rewrites a person's stored edits the next time they open the photo
(inferred). That is why the bar is exact parity, not better matching. Parity includes three oddities the fixture pins (all measured). The camera string repeats the maker
("Canon Canon EOS 5D Mark III"). An unmatched camera reads "(camera unmatched)". The lens centre is always 0, because the database has
no `<center>` element.

### 4.4 The module boundary

- New `app/src-tauri/src/lens_db/` holds `model.rs` (section 4.2), `parse.rs` (`roxmltree` to the model), `matcher.rs`, `interpolate.rs`,
  `scale.rs` (database coefficients to the render's normalised space), and `mod.rs` (load once, `OnceLock`, as today).
- `lens_profile.rs` keeps `match_profile`, `warm_lens_db`, `LensProfileMatch` and the three coefficient enums with the same signatures.
  It becomes the only caller of `lens_db`.
- `develop_engine/lens.rs` and `lens.js` keep their function names and signatures. Only the bodies of regions 2 to 4 are rewritten.
- The data moves to `data/lensfun-db/` in the repo. It holds the 56 XML files, the README and `timestamp.txt`, and the XSD and DTD if
  counsel allows (Q1). The generator pins each file's SHA-256, as it does for `CC-BY-SA-3.0.txt`. `tauri.conf.json` bundles the folder, and
  the app finds it through `bundled_models::resource_path` (resource folder first, then the checkout, so `cargo test` works).
- The XML is parsed at runtime and never converted at build time, so the shipped files stay byte-identical to upstream. That is the
  simplest reading of "unmodified" (inferred; Q2 asks counsel).

## 5. Clean-room discipline

### 5.1 Roles

- **Specifier.** May have read anything. Writes this RFC, the step 1 fixtures, and the hollowing commit (section 5.4). The author of
  this RFC opened `lens_profile.rs` and `develop_engine/lens.rs` in this session and did not open the crate's `src/`, `tests/` or
  `build.rs` (measured). The owner is the only human author in git (RFC-0038 B5.2), so has worked on the in-app ports (inferred). Both are specifiers and neither may implement.
- **Implementer.** Has never opened anything on the forbidden list. In practice a fresh agent session, or a person, briefed only with
  the allowed list. A session resumed or forked from one that read a forbidden source does not qualify.
- **Reviewer.** Audits the log and the transcripts before each implementer PR merges (section 5.4, item 5). Can be the owner.

### 5.2 Allowed inputs

- **A1.** This RFC and RFC-0038.
- **A2.** The database files, meaning the XML, the XSD, the DTD and `data/db/README.md`.
- **A3.** The published LensFun manual, read for this RFC on 2026-11-16: the
  [Lens group page](https://lensfun.github.io/manual/latest/group__Lens.html) (the formulas of every distortion, TCA and vignetting
  model) and the [calibration element page](https://lensfun.github.io/manual/latest/elem_calibration.html) (`real-focal`, and that
  samples are interpolated). The manual is generated from the library's header comments, so Q3 asks counsel whether it may be used.
- **A4.** The golden fixtures, meaning `lens_profile_golden.json`, the step 1 hold-out file and the frozen geometry oracle. They are tool
  output, not source.
- **A5.** Black-box runs of the current build. The implementer may ask the specifier to run new queries through today's `match_profile`.
  The specifier returns JSON only, and both sides log the request.
- **A6.** The app's code at the hollowing commit, in which the five regions of section 3 are empty.
- **A7.** Published literature on radial distortion models, each item logged by URL. Nothing from A7 was read for this RFC.

### 5.3 Forbidden sources

- **F1.** The `lensfun` crate's `src/`, `tests/`, `build.rs`, its rustdoc, and its source view on docs.rs.
- **F2.** Upstream LensFun source, meaning `libs/`, the headers as source files, `apps/`, and any mirror of them.
- **F3.** Other ports of LensFun in any language, which may themselves be derivative (guess).
- **F4.** The five regions of section 3 as they stand before the hollowing commit, and the git history of those files (`git log -p`,
  `git show`, `git blame`).
- **F5.** Any transcript, log or summary of a session that read F1 to F4.

### 5.4 Mechanics a reviewer can check

1. **Hollowing commit.** The specifier deletes the bodies of regions 1 to 4, leaves each signature with a stub (`unimplemented!()` in
   Rust, an identity return in WGSL), and deletes RFC-0021's transcribed formulas from region 5's sections, leaving a pointer to this
   RFC. The diff contains deletions and stub lines only.
2. **Shallow clone.** The implementer works in `git clone --depth 1` of the hollowing commit. History, and with it F4, is not
   available in that checkout.
3. **Hold-out.** Step 1 commits the hold-out file and the hollowing commit deletes it, so the shallow clone does not contain it. For
   the parity check of step 4, the reviewer restores it from the step 1 commit and records its SHA-256. Passing it is evidence that the
   matcher was not fitted to the visible queries.
4. **Clean-room log.** `docs/rfc/RFC-0039-appendix/clean-room-log.md`, one section per implementer session:

   ```
   ## Session <n>: <agent session id and model, or the person's name>, <start date>
   Start commit: <hollowing commit SHA>     Transcript: <where it is kept>, SHA-256 <hash>
   Attestation: I had not opened any F1 to F5 source before this session and did not open one during it.

   | # | Date | Source (path at commit, or URL with date) | Allowed item | What was taken from it |
   | 1 | 2026-..-.. | lensfun.github.io/manual/latest/group__Lens.html | A3 | the ptlens formula |
   ```

5. **Transcript audit.** The reviewer searches each transcript for the F-list patterns (`lensfun-0.7.0/src`, `lensfun-0.7.0/tests`,
   `lensfun-0.7.0/build.rs`, `docs.rs/crate/lensfun`, `github.com/lensfun/lensfun/blob`, `git log -p`, `git blame`, `git show`) and
   records the empty result in the log. The reviewer also checks that the new code contains no coefficient literal copied from a fixture.

### 5.5 What the process cannot settle

Where the math dictates the code, as with a three-term polynomial, a clean rewrite comes out identical to the port. Whether that matters
is Q4 for counsel.

### 5.6 The three rescale functions

They must be rewritten from A3 and A4, not from region 1. The manual gives the model formulas, including the `1 - k1` and
`1 - a - b - c` terms of poly3 and ptlens (measured, A3). Neither manual page read says:

- which image length the radius is normalised to;
- how the crop factor and the calibration aspect ratio enter;
- how values between samples are interpolated (the calibration page says only that they are);
- how the real focal length is found when `real-focal` is absent. The calibration page says lensfun falls back to the nominal focal
  length.

The fixture disagrees with that last point. At a calibration sample, 188 of 190 resolved queries have a `real_focal` equal to the
`real-focal` attribute, to focal times `1 - a - b - c` (ptlens), or to focal times `1 - k1` (poly3). Only 24 samples carry the
attribute (measured with `fixture_coverage.py`). The other 2 follow no rule found (guess, because some database entries share a lens name).
So for real focal length the golden output is the spec, not the manual. The other three points are open question O1. Absent a published
definition, the implementer derives them from A4 and A5, which section 7 costs.

## 6. The golden fixture as the acceptance oracle

The implementer's work is accepted when `match_profile_still_gives_the_pinned_results_for_the_golden_queries` passes on the visible
fixture and then on the hold-out file, unchanged (strings and nulls exact, numbers within 1e-6 relative). The fixture holds 410
queries, and 248 resolve. Of those, 226 have ptlens and 22 have poly3 distortion, 180 have poly3 TCA and 81 have vignetting.
They cover 65 lenses and 37 cameras (measured).

### 6.1 How it can fail as an oracle

Each gap is measured with `fixture_coverage.py`.

- **G1.** No query resolves to poly5 distortion. The database has 5 such samples.
- **G2.** No query resolves to linear TCA. The database has 5. Today only the RFC-0021 geometry test covers G1 and G2, and it compares
  against `lensfun::Modifier` at runtime, so it disappears with the crate.
- **G3.** No query has a focal length outside the lens's calibrated range. Clamping and extrapolation are not pinned.
- **G4.** 58 queries fall between samples, one per lens, between the first two. Vignetting is pinned only at the first calibrated
  aperture and at f/8, and always at 1000 m.
- **G5.** 2 queries resolve with an unmatched camera. None has a camera model without a make. None uses an adapted lens, because the
  generator always picks a camera with the lens's own mount.
- **G6.** No query uses a localised name, although 30 of the 65 resolved lenses have one.
- **G7.** 92 lens names belong to more than one database entry. Which one wins is pinned only for the 9 that happen to resolve.
- **G8.** No query comes from the EXIF of `test_image/` (PROGRESS lists it as not verified).
- **G9.** 58 of 114 lens names taken from the database never resolve in any spelling, mostly fixed-lens compacts named "& compatibles".
  The fixture pins these misses. A replacement that resolves them fails the oracle, which is intended. Better matching is a later,
  separate change with a reviewed fixture regeneration.
- **G10.** The fixture checks coefficients, not pixels. Regions 2 to 4 are covered only by the hand-derived tests in `geometry.rs` and
  by the runtime oracle of G2.
- **G11.** The generator of the fixture needs the crate. The inputs are frozen in the JSON, so the oracle survives. The generator must
  be rewritten over `lens_db` in step 6, after parity holds.

### 6.2 What to add before starting (step 1)

1. A hold-out file, `lens_profile_holdout.json`, with expected values from today's code. It covers every poly5 and linear-TCA lens (G1, G2);
   at least 10 queries each below the minimum and above the maximum focal length (G3); midpoints across each zoom's range and apertures
   between calibrated ones (G4); unknown cameras, a missing make and adapted mounts (G5); localised names (G6); duplicate names (G7);
   and the camera and lens names read from `test_image/` (G8).
2. A frozen geometry oracle, `lens_geometry_golden.json`, holding the `Modifier` outputs the RFC-0021 test computes today (six lenses, their
   focal lengths, five pixels, distortion and TCA), so that test compares against recorded numbers after the crate goes (G2, G10).
3. A threshold in `fixture_coverage.py`, so that it fails when a G1 to G7 row is below 10, or below the database count where that is smaller.

## 7. Cost

RFC-0038 estimates B5.3 at 4 to 8 days: 3 to 6 to build, 1 for the harness, and 0.5 to 1 for notices. Checked against the code:

| Work | RFC-0038 | This RFC | Basis |
| --- | --- | --- | --- |
| Golden harness (RFC-0038 slice 2) | 1 | 0 | Merged 2026-11-14 (measured, PROGRESS) |
| Step 1, the hold-out file and the frozen geometry oracle | not counted | 1 to 1.5 | Section 6 (guess) |
| Hollowing commit, log, audit | not counted | 0.5 | Section 5 (guess) |
| `lens_db` parser, matcher, interpolation and scaling | 3 to 6 | 3 to 6 | Unchanged (guess) |
| Regions 2 to 4: kernels and `LensNorm`, Rust and WGSL | not counted | 1 to 2 | Section 3 found them (guess) |
| Data as a resource, load time | inside the 3 to 6 | 0.5 to 1 | Bundling, `resource_path`, measuring against 42 ms (guess) |
| Notices and crate removal | 0.5 to 1 | 0.5 to 1 | `COPYLEFT_ALLOWED`, `licenses/lensfun-*`, the `bundled_models` test, the About sentence in `SettingsDialog.svelte`, the generator's text |
| **Total** | **4 to 8** | **6.5 to 12** | |

RFC-0038's figure is low by 2.5 to 4 days (inferred). Its scope note names only region 1, so it misses the render kernels, the WGSL
twin, the Newton inverse and the runtime oracle test. The harness day it counts is already spent. RFC-0038's B5 total moves from
27 to 118.5 days to about 29.5 to 122.5.

Two risks sit above the range. The matcher is fuzzy, because "LUMIX G VARIO 12-35/F2.8II" resolves to "Lumix G X Vario 12-35mm f/2.8 II Asph.
Power OIS" (measured, fixture). Reproducing the same winner from examples alone may take longer than 6 days (guess). If O1 finds no
published definition, deriving the normalisation and the interpolation from fixtures adds 1 to 2 days (guess), for an upper bound near 14.
Counsel time is not counted.

## 8. Alternatives rejected

- **Keep the crate and comply with LGPL-3.0 in a proprietary build.** The crate is linked statically. Letting a user relink a modified
  `lensfun` means shipping the app's object files or rlibs for the exact toolchain each release (RFC-0038 D3, route ii), and the EULA
  terms would have to allow it (Q6). Regions 1 to 5 would also stay in proprietary source. Rejected because the cost recurs every
  release, it hands out the app's objects, and it leaves the in-app ports. It is the fallback if counsel finds both acceptable.
- **Switch to dynamic linking.** The crate exposes a Rust API only (guess, from its NOTICE, which calls it a pure-Rust port). The project would write and ship a `cdylib` wrapper over a Rust API with
  no stable ABI, then sign a macOS framework and ship a Windows DLL. RFC-0038 B5.4 (b) costs shipping LibRaw as a shared library at 3 to 5
  days, and LibRaw already has a C interface. The in-app ports stay. Rejected because it costs about as much as the clean room and solves less.
- **Drop lens profile correction.** About 1 to 2 days to remove the crate and the Profile part of Lens Corrections, keeping Manual
  (guess). It removes an M3 feature that Lightroom users expect. The edition mechanism of ADR-0012 does not help, because the crate stays
  linked unless compiled out. Rejected as the default. It is the cheapest fallback if D5 says the database cannot ship and RFC-0038's
  contingency is too costly.
- **Ask for a different licence.** The crate's NOTICE says it is "a derivative work of LensFun" and inherits its licence, so its author
  alone cannot relicense the upstream contributors' code (inferred). Rejected.

## 9. Decisions only the owner or counsel can make

**Owner.**
- D1 of RFC-0038, which decides whether this RFC is built at all.
- Who implements (a fresh agent session or a person) and where transcripts are kept.
- Whether parity includes today's misses (G9). The recommended answer is yes, with any change to matching made later in its own PR.
- Whether 6.5 to 12 days is worth it against the alternatives in section 8.

**Counsel.** These add to RFC-0038 section 4.
- **Q1.** D5 as stated. Also, the XSD and DTD sit in the CC-BY-SA folder, but no licence is stated in them. May they ship?
- **Q2.** If the unmodified XML is committed to this MIT repository and read at runtime, does it stay a separate work beside the binary?
- **Q3.** Is the published LensFun manual, generated from the LGPL headers' comments, a permissible clean-room input?
- **Q4.** Where the math dictates the code, is a rewrite that comes out identical still derived from the port?
- **Q5.** Regions 1 to 4 are in MIT-published commits. RFC-0038 question 1 asks about region 1 only. Does that history
  need anything beyond removing the regions from future versions?
- **Q6.** If the crate is kept (section 8), which EULA terms does LGPL-3.0 section 4 require?
- **Q7.** Are the roles, the forbidden list, the shallow clone, the log and the transcript audit adequate evidence of a clean room?

**Open engineering questions.**
- **O1.** A published definition of the radius normalisation, the crop and aspect-ratio scaling, and the interpolation method. Not found
  in the repo, in the database's README, XSD or DTD, or on the two manual pages read.
- **O2.** The 2 of 190 `real_focal` values that follow no rule found (section 5.6).
- **O3.** Load time of the 5.0 MB of XML with `roxmltree` against today's 42 ms (not measured).
- **O4.** How later upstream database updates arrive. The proposal is to copy the files, update the pinned hashes, regenerate the
  fixtures on purpose and review the diff.

## 10. Build order

One PR per step, each with a PROGRESS entry in the house style and a "Not verified" list. Each step names the check that closes it.

0. **Gate.** D1 = (b) and D5 are recorded in ADR-0011. The step closes when that ADR is merged. Step 1 may run before the gate.
1. **Extend the oracle** (specifier, section 6.2). It closes when `fixture_coverage.py` passes its thresholds, the hold-out test and
   the frozen geometry test pass on today's code, and a deliberate change to the poly5 scaling fails the hold-out test (recorded in the PR).
2. **Ship the data as a resource** (specifier, section 4.4; the crate still does the work). It closes when
   `gen_third_party_libraries.py --check` passes, each file's SHA-256 equals the crate's `data/db/` copy, and a release bundle lists
   `lensfun-db/`.
3. **Hollow the five regions** (specifier, section 5.4). It also deletes the hold-out file. It closes when the diff holds deletions
   and stubs only, `cargo build` succeeds, and the lens tests fail at the stubs.
4. **Build `lens_db`** (implementer, shallow clone). It closes when the visible fixture passes, the hold-out file then passes unchanged,
   the clean-room log is complete, and `warm_lens_db` in a release build is measured against 42 ms.
5. **Rewrite the kernels and `LensNorm` in Rust and WGSL** (implementer). It closes when the frozen geometry oracle agrees within
   0.05 px, the existing `geometry.rs` tests pass, and a `verify-emulsion` run opens a photo with a profile and the preview matches
   the export.
6. **Remove the crate** (either role). This covers `Cargo.toml`, the fixture generator rewritten over `lens_db`, the generator's tables
   and `COPYLEFT_ALLOWED`, `licenses/lensfun-*`, the `bundled_models` licence test and the About sentence. It closes when
   `cargo tree -i lensfun` finds no such package, `nm` on the release `emulsion` binary shows no `lensfun` symbol (the method of the
   installer inventory appendix), and `--check` and the full Rust and Vitest suites pass.
7. **Audit** (reviewer, section 5.4 item 5). It closes when the empty search result is in the log and the log has gone to counsel
   with Q7.

## Provenance

Written by one agent session (Claude Opus 5.5) on 2026-11-16. It read RFC-0038 and its appendix list, RFC-0021, RFC-0037 for format,
`lens_profile.rs`, `develop_engine/lens.rs`, the relevant parts of `lens.js`, `geometry.rs` and `lib.rs`, `Cargo.toml`, the
fixture, PROGRESS.md, the crate's `NOTICE`, `data/db/README.md` and XSD, and the two manual pages above. It did not open the crate's
`src/`, `tests/` or `build.rs`, or upstream LensFun source. The fixture and database counts come from `fixture_coverage.py`, which
reads only the fixture and the XML. Engineering planning, not legal advice.
