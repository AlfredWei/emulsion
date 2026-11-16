# RFC-0038: LGPL and CC-BY-SA terms before Emulsion installers go to other people

Status: draft plan (synthesised 2026-11-14 from two independent plans; provenance at the end).

Status: engineering plan, not legal advice. Every statement about what a licence *requires* is marked as a question for
counsel or the owner; the engineering steps are written so they are useful whatever the answers turn out to be.

## 0. What the repo actually shows (checked 2026-11-14)

The grounding file is accurate. Checked against the tree, with additions that change the plan:

| Fact | Where checked | Consequence |
| --- | --- | --- |
| LibRaw 0.21.3, dual LGPL-2.1 / CDDL-1.0; macOS compiles vendored source, `cargo:rustc-link-lib=static=raw` | `vendor/rsraw-sys/LibRaw/COPYRIGHT`, `libraw/libraw_version.h`, `vendor/rsraw-sys/build.rs` | Static link on macOS. LibRaw source itself is unmodified (`PATCH.md`: only `build.rs` changed). |
| Windows links vcpkg `libraw:x64-windows-static-md`, the whole `ports/` tree checked out at vcpkg commit `cdf98aaa…` | `.github/workflows/ci.yml` (both jobs) | The Windows binary's LibRaw is built from *vcpkg's* port, which may apply its own patches and pull other static libraries. Not yet inventoried. |
| `lensfun` 0.7.0, LGPL-3.0-or-later, normal crates.io dependency | `app/src-tauri/Cargo.toml`, crate `NOTICE` | Compiled into the same Rust binary. |
| Lens DB: 60 files in `data/db/`, gzipped by lensfun's own `build.rs` and embedded with `include_bytes!` | crate `build.rs`, `src/db.rs:101` | The CC-BY-SA data is inside the executable, not a separate file. Unmodified. |
| The installer ships `THIRD_PARTY_NOTICES.md` and `THIRD_PARTY_LIBRARIES.md` | `tauri.conf.json` `bundle.resources` | Notices exist, but **no LGPL, GPL, CDDL or CC-BY-SA text is shipped**: `THIRD_PARTY_LIBRARIES.md` says full texts are "in its own source distribution". |
| About tab opens only `THIRD_PARTY_NOTICES.md` and its text mentions only AI models | `SettingsDialog.svelte` ~l.617, `src/lib.rs` `third_party_notices_path` | The library list is installed but not reachable from the UI. |
| Release build is `npm run build:release` (models + AI helper + `tauri build`), run by hand | `app/package.json`, `Makefile` `build` | **CI never builds an installer**; no release workflow, no tags. Nothing ties an installer to a commit. |
| PRD M8 lists "Licensing/distribution decision (open source? paid?)" as undecided | `PRD/MILESTONES.md` l.325 | The repo's MIT `LICENSE` and the PRD disagree. This is the decision everything else hangs on. |
| `r-efi` is listed as `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | `THIRD_PARTY_LIBRARIES.md` | A non-LGPL option exists; the generator should record which one Emulsion takes. Probably not even linked on macOS/Windows. |

Not verified, and treated as open below: whether the `emulsion-ai` helper binary also contains LibRaw or lensfun code;
which extra static libraries the vcpkg LibRaw build brings in on Windows; whether installers will be signed or
notarised.

## 1. Decisions that are not engineering (owner, then counsel)

These come first because they decide how much of section 3 is needed. Engineering should not guess them.

- **D1. Who gets installers, and on what terms (owner).** (a) public, open source under MIT, source on GitHub;
  (b) public but proprietary or paid; (c) a few named testers. The obligations below are triggered by *conveying*
  binaries, so even (c) counts. If (b), the LGPL work grows a lot (see 3.6). Record the answer by resolving PRD M8's
  open bullet and the `LICENSE` file together.
- **D2. Which LibRaw licence Emulsion takes (owner, with counsel).** LGPL-2.1 or CDDL-1.0. They have different
  conditions about source availability and about how they treat static linking; counsel should say which is less
  work for the chosen D1, and whether one notice can cover both platform builds. Engineering's reading, to be confirmed:
  CDDL is a file-level licence without a relink condition, so electing it could remove the LibRaw relinking question
  altogether, leaving only `lensfun` (LGPL-3.0, always static). If no election is made, plan for the LGPL-2.1 terms,
  which are the harder ones.
- **D3. How the "user can modify the library and relink" condition is met for a static build (counsel).** Candidate
  routes: (i) the whole application source at the exact release commit is public, plus build instructions;
  (ii) also ship object files / `.rlib`s for relinking; (iii) switch LibRaw to dynamic linking. Counsel to say
  whether (i) is enough for LGPL-2.1 (LibRaw) and LGPL-3.0 (lensfun), given that Rust has no stable ABI.
- **D4. How source is made available (counsel).** Source archive next to each installer on the same release page,
  versus a written offer, versus "it is on GitHub". Counsel to say whether a link to the GitHub repo is enough or
  whether the release must carry the exact library sources (including the vcpkg-built Windows LibRaw).
- **D5. Lens database (counsel).** Whether embedding the unmodified CC-BY-SA 3.0 database in an MIT program needs
  anything beyond attribution, the licence URI and "unmodified"; and whether gzip-embedding counts as a modification.
- **D6. Signing (owner, then counsel).** If builds are signed/notarised, whether LGPL-3.0's "Installation
  Information" clause applies to a desktop app (it is written for "User Products"), and whether documenting
  ad-hoc re-signing is wanted anyway.

## 2. Order of work

```
Phase A (now, no regret)  ->  D1..D6 answered  ->  Phase B (release plumbing)  ->  Phase C (per-release gate)
A1 inventory, A2 texts, A3 UI, A4 CI checks        B1 release CI, B2 source bundle, B3 relink docs+test
```

Phase A is useful under every answer to D1–D6 and makes the questions to counsel precise (A1 gives counsel the exact
list of what is linked, from which source, on which platform). Do not ship installers to anyone outside the owner
until Phase B is done and Phase C passes.

## 3. Engineering steps

### Phase A — independent of the decisions

**A1. Inventory what each installer really contains (1 PR, docs + generator).**
- macOS: build with `npm run build:release`, then on `Emulsion.app/Contents/MacOS/*` run `nm -gU` / `nm` and
  `otool -L`. Grep for `libraw_init` and for lensfun symbols (`lensfun::`), for both `emulsion` and `emulsion-ai`.
  Record which binary carries which library. Also note that `app/src-tauri/Cargo.toml` has
  `crate-type = ["staticlib", "cdylib", "rlib"]` (the Tauri template default), so `cargo build` also emits a static
  library of the whole app; release packaging must publish only the installer bundles, never those artefacts.
- Windows: on the CI runner, after the vcpkg step, run `vcpkg depend-info libraw:x64-windows-static-md` and
  `vcpkg list`; list `ports/libraw/*.patch` at commit `cdf98aaa…` and the LibRaw tarball URL/hash in its
  `portfile.cmake`. Any extra static libraries it brings (if any) go into the notices too.
- Extend `scripts/gen_third_party_libraries.py`: a "Native libraries" section (LibRaw version 0.21.3, how it is
  built per platform, the vcpkg commit, any transitive static libs) and an "elected licence" column for multi-licence
  entries (`r-efi`: MIT; LibRaw: per D2, "not yet elected" until then). Keep it generated, as today.
- Status: measured in `docs/installer-inventory` (appendix: installer-inventory.md). macOS: both LGPL components are in the one
  `emulsion` binary; `emulsion-ai` has neither. Windows: the vcpkg port also installs other static libraries (jasper, lcms,
  libjpeg-turbo, zlib, freeglut), now listed in a Windows-only row of the generated table and raised in question 2. Not done: a
  packaged installer was not built, so what `tauri build` copies is unknown; the "elected licence" column waits for D2.

**A2. Ship the licence texts with the installer (1 PR).**
- New repo directory `licenses/` populated by the generator, not by hand, so it cannot drift:
  `LibRaw-COPYRIGHT`, `LibRaw-LICENSE.LGPL` (2.1), `LibRaw-LICENSE.CDDL` (from `vendor/rsraw-sys/LibRaw/`);
  `lensfun-LICENSE-LGPL-3.0`, `lensfun-LICENSE-GPL-3.0`, `lensfun-NOTICE` (located through `cargo metadata`'s
  `manifest_path`, not a hard-coded `~/.cargo` path); `CC-BY-SA-3.0.txt` (legal code, committed once);
  the MPL-2.0 text for the four MPL crates already listed.
- `app/src-tauri/tauri.conf.json` `bundle.resources`: add `"../../licenses/": "licenses/"`.
- Change the generator's `TEXT` so `THIRD_PARTY_LIBRARIES.md` says the texts are in `licenses/` beside it, and states
  that the Lensfun database is included unmodified.
- Status: done in `feature/licence-texts` (the unmodified-database statement was already in the table; the generator copies
  from the vendored LibRaw and from the crates `cargo metadata` resolves; `CC-BY-SA-3.0.txt` is committed once and its SHA-256 is
  pinned in the generator; the four MPL files carry one text, which the generator checks; `selectors` ships no licence file).

**A3. Make the notices reachable from the app (1 small PR).**
- `SettingsDialog.svelte` About tab: one sentence naming LibRaw (copyright LibRaw LLC), Lensfun (copyright the
  Lensfun contributors and David Veszelovszki) and the Lensfun lens database (CC-BY-SA 3.0, the Lensfun community),
  plus a second button "Open library licences".
- Generalise `bundled_models::notices_path` into a resource lookup taking a file name (same resource-dir-then-checkout
  fallback) and add a `third_party_libraries_path` command next to `third_party_notices_path` in `src/lib.rs`.
  `capabilities/default.json` already allows `opener:allow-open-path` on `**`, so no capability change.
- Whether this UI wording satisfies any "prominent notice" condition is for counsel; it costs little either way.
- Status: done in `feature/about-library-licences`.

**A4. CI guards so this does not rot (1 PR, `.github/workflows/ci.yml`).**
- Add `--check` to the generator (regenerate to memory, diff against `THIRD_PARTY_LIBRARIES.md` and `licenses/`, exit
  non-zero on drift) and run it in a new lightweight job (needs Rust 1.99.1 for `cargo tree`/`cargo metadata` and
  Python; ubuntu is fine because it uses `--target all`).
- Copyleft allow-list in the generator: fail if any crate's licence expression contains `GPL`, `LGPL`, `AGPL`,
  `CDDL`, `EPL`, `CC-BY-SA` or `MPL` and the crate is not named in the hand-written conditions table. (Alternative:
  `cargo deny check licenses` with a `deny.toml`; see rationale.) A new copyleft dependency then cannot slip in
  silently.
- Version-sync step in both Windows jobs: assert the vcpkg-installed LibRaw version equals
  `LIBRAW_MAJOR/MINOR/PATCH_VERSION` in the vendored `libraw_version.h`. This already matters for the FFI layout; it
  also makes "the source of LibRaw is the vendored 0.21.3 plus vcpkg's port patches" a checked claim.
- Rust test beside `every_bundled_model_has_an_entry_in_the_third_party_notices` (`bundled_models.rs`): assert the
  `licenses/` files exist and that `THIRD_PARTY_LIBRARIES.md` names LibRaw, lensfun and the database.
- Status: done in `chore/licence-ci-guards` (the Rust test came with A2). The allow-list is the generator's own rule rather
  than `cargo deny`: a crate whose licence expression has no permissive alternative must be in `COPYLEFT_ALLOWED`.

### Phase B — after D1–D4

**B1. Build installers in CI from a tag (new `.github/workflows/release.yml`).** Trigger on `v*` tags; matrix
`macos-latest`, `windows-latest`; same Rust pin and vcpkg step as `ci.yml` (factor the vcpkg step into a composite
action under `.github/actions/` so the pin lives in one place); run `npm ci` and `npm run build:release`; upload
the bundles from `app/src-tauri/target/release/bundle/` to a **draft** GitHub Release. After building, assert the
bundle contains `licenses/`, `THIRD_PARTY_NOTICES.md` and `THIRD_PARTY_LIBRARIES.md` (macOS: inside
`Emulsion.app/Contents/Resources/`; Windows: unpack the NSIS/MSI with 7-Zip or `msiexec /a` and check). This is the
step that ties every installer to one commit, which every answer to D3/D4 relies on.

**B2. Source bundle per release (same workflow).** Attach `emulsion-<version>-source.tar.gz` containing `git archive`
of the tag, `cargo vendor` output for `app/src-tauri` (includes `lensfun-0.7.0` with `data/db/`), and, for Windows,
the vcpkg `ports/libraw` directory at the pinned commit plus the LibRaw tarball its portfile downloads. Add the
release notes line "Source for this release, including the LGPL libraries, is attached". If counsel answers D4 with
"a link is enough", B2 can be dropped; it is cheap to keep.

**B3. Relink instructions, and a CI job that proves them.** A section in `README.md` (or `docs/BUILDING.md`):
- lensfun: put a modified copy anywhere and add `lensfun = { path = "..." }` under the existing
  `[patch.crates-io]` in `app/src-tauri/Cargo.toml`, then `npm run build:release`.
- LibRaw on macOS: edit `vendor/rsraw-sys/LibRaw/`; on Windows: point vcpkg at a modified port (overlay port).
  Note that bindgen regenerates the FFI from the header on native builds, so header changes are picked up, but
  struct-layout changes need matching use in `vendor/rsraw`.
- If D6 = signed: how to ad-hoc re-sign (`codesign --force --deep -s - Emulsion.app`).
- Prove it by hand once: on a clean macOS account (and a clean Windows machine), follow only `docs/BUILDING.md` (new in this
  step: toolchain versions, `npm ci`, `cargo build --locked`, the Windows vcpkg step copied from `ci.yml`), change one line
  inside `vendor/rsraw-sys/LibRaw`, and see the change in the rebuilt app. If that fails, the instructions are not
  compliant. Add it to the milestone-end "not verified" backlog.
- CI: a weekly (`schedule:`) job that copies lensfun via `cargo vendor`, applies a trivial patch, adds the
  `[patch.crates-io]` entry, and runs `cargo build --release` on macOS. If it breaks, the instructions are wrong.

**B4. If D2 = CDDL for LibRaw:** generator states the election, `licenses/` keeps `LICENSE.CDDL` as the operative text,
and B2's source bundle is the "how to obtain the source" pointer CDDL asks for (counsel to confirm wording).

**B5. If D1 = proprietary/paid (only then; large; costed sequence).** Steps are in dependency order. Effort is engineer-days for one
person who knows the codebase. Counsel time, account approvals and GPU rental are not counted. Model findings are in
[RFC-0038-appendix/model-training-data.md](RFC-0038-appendix/model-training-data.md). B1 and B2 are prerequisites, costed in Phase B.

New decisions (in addition to D1 to D6):
- **D7. Model-data policy (owner, then counsel)**: which dataset-term classes a paid build accepts. Two questions for counsel: do
  non-commercial dataset terms reach weights that a third party licensed as Apache or MIT, and does an ImageNet-initialised backbone count?
- **D8. Relicensing (owner, counsel)**: future versions become proprietary. Decide whether the public MIT repository goes private and
  whether a free edition stays. MIT grants already made for published commits cannot be withdrawn; counsel to confirm the wording.
- **D9. Activation (owner)**: an offline signed licence file versus online activation. PRD section 3 (local-first) argues for offline.
- **D10. Seller and tax (owner, accountant)**: the legal entity, and a merchant of record versus selling directly.
- **D11. Face features in a paid build (owner, counsel)**: biometric-data duties, separate from the licence question.

| Step | What changes | Days | Depends on | Decide first | Start now? |
| --- | --- | --- | --- | --- | --- |
| B5.1 Evidence pack | Training-data terms for all eight models in `models/*_MODEL.md` plus a "Training data" column in the notices; A1's inventory | 1.5 to 2.5 | A1 | none | Yes |
| B5.2 Ownership check, then relicense | Details below | 1 to 2 | B5.1 | D8; counsel on AI-assisted authorship and the published MIT history | The check: yes (read-only) |
| B5.3 Replace `lensfun` (clean room) | New `src/lens_db/` behind the existing `lens_profile::match_profile`: an XML reader on `roxmltree` (already a dependency), a fuzzy camera/lens matcher, and the interpolation. Drop `lensfun` from `Cargo.toml`. Ship the 60 XML files unmodified as a separate resource (`bundle.resources`), with `licenses/CC-BY-SA-3.0.txt` and attribution; regenerate `THIRD_PARTY_LIBRARIES.md`. The implementer writes from the XML schema and golden outputs, not from the LGPL source, and keeps a clean-room log. Gets its own RFC, now [RFC-0039](RFC-0039-lensfun-clean-room-replacement.md), which finds four more ported regions and re-costs the step at 6.5 to 12 days | 4 to 8 (3 to 6, +1 harness, +0.5 to 1 notices) | Golden harness (slice 2) | D5 | Harness: yes |
| B5.4 LibRaw | (a) Elect CDDL-1.0 (0.5 to 1 day): election column in the generator, `LibRaw-LICENSE.CDDL` becomes the operative text, B2 carries LibRaw's source and the vcpkg port patches, the EULA must not restrict the CDDL source rights. (b) Ship LibRaw as a shared library (3 to 5 days): `vendor/rsraw-sys/build.rs` to `dylib`, macOS `bundle.macOS.frameworks` with `@rpath` and nested signing, the Windows `x64-windows` DLL; the EULA still needs the LGPL-2.1 section 6 terms. (a) is preferred, pending counsel | 0.5 to 5 | A1; B1 for (b) | D2; counsel on CDDL | No |
| B5.5 Model remediation | The mechanism first (1 to 2 days, slice 4), then the per-model rows below | 5.5 to 74 total | D7 | D7, D11 | Mechanism: yes |
| - LaMa (likely blocker) | (i) Fall back to the existing `inpaint_patchmatch.rs` (1 to 2 days; RFC-0032 section 9 shows a large quality loss); (ii) retrain the LaMa architecture on CC0 or CC BY photos with a non-ADE20K perceptual loss and re-run the bake-off (15 to 30 days plus GPU); (iii) a commercially licensed inpainter (not researched). Reverses ADR-0010, so record a new decision | 1 to 30 | mechanism | D7 | No |
| - SFace (likely blocker) | (i) Drop face grouping from the paid edition (1 to 2 days); (ii) the synthetic-data model `FACE_MODELS.md` names, about 92% LFW (15 to 25 days); (iii) a commercial SDK | 1 to 25 | mechanism | D7, D11 | No |
| - YuNet (likely blocker) | Swap the detector. Candidate: Google's MediaPipe face detector (its card has not been read); load in tract and check parity on the blink set. It gates face grouping and the blink check | 2 to 4 | mechanism | D7 | No |
| - Real-ESRGAN (closer look) | (i) Ask the maintainer what the checkpoint was trained on (0.5 day plus a wait); (ii) retrain SRVGGNetCompact (1.2 M parameters, BSD-3 code) on CC0 or CC BY photos (5 to 10 days); (iii) drop Upscale (1 day) | 0.5 to 10 | mechanism | D7 | Asking: yes |
| - D-FINE (closer look) | Counsel clears the ImageNet initialisation (0 days), or fall back to SAM 2 or drop the person preset (2 to 3 days) | 0 to 3 | D7 | D7 | No |
| - NAFNet, SAM 2, Face Mesh | Notices only | 0 | none | none | n/a |
| B5.6 Signing and notarisation | Details below | 4 to 6 | B1 | Entity name, budget, Windows certificate route (cost not retrieved) | Account applications (calendar time) |
| B5.7 Payments | A merchant of record (details below) or a processor plus own tax handling; product setup, checkout link, webhook to licence issuance | 2 to 4 | B5.8 design | D10 | No |
| B5.8 Licence keys | New `src/licensing.rs`: offline Ed25519-signed licence files, trial and grace period. Gating covers AI features and export size, never the catalog or the user's data. An issuing service or a provider API; Settings "Licence" tab; tests for tampering, clock rollback and expiry. Gets its own RFC | 3 to 6 | D9 | D9, D10 | No |
| B5.9 EULA and privacy policy | Counsel's texts. Installer licence page, first-run acceptance, About-tab link. The EULA keeps third-party rights intact. The privacy policy covers geocoding, tiles, model downloads, activation, updates and local face data | 1.5 to 3 | B5.4, B5.8, B5.10 | Counsel; D11 | No |
| B5.10 Updater | `tauri-plugin-updater` (not in `Cargo.toml` today) with mandatory signatures: the public key in config, the private key as a CI secret (losing it means no more updates), `createUpdaterArtifacts`, a static `latest.json` on a public host, an opt-in check | 2 to 4 | B1, B5.6, B5.11 | Key custody; opt-in | No |
| B5.11 Hosting and repo split | Release assets of a private repository are not public. The two model assets (`bundled-models.json`, `denoise_models.rs`), the installers and `latest.json` need a public host, plus CI secrets and minutes | 1 to 2 | D8 | D8 | No |
| B5.12 Public OpenStreetMap services | The tile policy gives commercial apps no special terms, says access "may be withdrawn at any point" and asks apps not to hard-code the tile URL; Nominatim allows at most 1 request per second across all users and warns commercial apps. Make the endpoints configurable (slice 5), then choose a provider | 1 to 2 | none | Provider and cost | Configurability: yes |

**B5.2 details.**
- **Ownership facts** (read-only git, 2026-11-14): one author identity, `Alfred, S.-Y., Wei`; 888 commits on `main` (911 across all branches) at `e5fcd3a26`; current
  figures and their pin are in [RFC-0038-appendix/provenance.md](RFC-0038-appendix/provenance.md).
  No other human author appears in git; contributions made on the GitHub side only would not show there.
- **AI co-authors**: 463 commits on `main` carry a `Co-Authored-By` trailer naming a Claude model. Counsel should check ownership and
  copyrightability against the tool terms in force at the time.
- **Third-party code in the tree**: vendored `rsraw` and `rsraw-sys` (MIT, patched; keep their notices), and the
  `docs/rfc/*-appendix/*.py` scripts (not shipped).
- **Relicensing edits**: `LICENSE`, the `license` fields in `app/package.json` and `Cargo.toml`, `README.md`, line 3 of
  `THIRD_PARTY_NOTICES.md`, the text in `scripts/gen_third_party_libraries.py`, and the About copy in `SettingsDialog.svelte`.

**B5.3 scope note.** The coefficient-rescaling functions in `lens_profile.rs` (`rescale_distortion`, `rescale_tca`, `rescale_vignetting`) describe themselves as "a direct port" of lensfun's own math, so LGPL-derived logic already sits in the app's own source file. A clean-room replacement must rewrite those functions from the published model definitions too, not only swap the crate; the golden fixture from slice 2 pins their output. Counsel should also be asked about them (question 1).

**B5.3 contingency.** If counsel says the CC-BY-SA database cannot ship even as a separate data file beside a proprietary binary (D5),
the lens feature needs another data source: roughly 10 to 25 days, not researched.

**B5.6 details** (Tauri's macOS and Windows signing guides).
- **macOS**: an Apple Developer Program membership (about US$99 a year per the guide; notarising is not possible on the free plan) and a
  Developer ID Application certificate, which only the Account Holder can create. Sign with the hardened runtime and entitlements for the
  app, `emulsion-ai` and any ONNX Runtime dylib; run notarytool and staple in `release.yml`.
- **Windows**: sign through Azure Artifact Signing or an OV or EV certificate via `signCommand`; since 2024 EV certificates no longer
  get instant SmartScreen reputation. Prices: not retrieved, check current ones.
- **D6**: mostly goes away once B5.3 removes `lensfun`.

**B5.7 detail.** Paddle describes itself as the seller on record that acts as a reseller and takes on VAT and sales tax. For B5.8,
Lemon Squeezy's licence API has activate, validate and deactivate; Keygen offers machine activation and offline licence files.

**Total: about 27 to 119 engineer-days** (roughly 5 to 24 working weeks for one person), plus Phase B and counsel or calendar time.
Almost all of the spread is B5.5. Low end: 1.5+1+4+0.5+5.5+4+2+3+1.5+2+1+1 = 27. High end: 2.5+2+8+5+74+6+4+6+3+4+2+2 = 118.5.
- **Fallback path, about 27 to 40 days (6 to 8 weeks)**: CDDL election, PatchMatch for Remove, drop face grouping, swap the face
  detector, ask about Real-ESRGAN, counsel clears D-FINE.
- **Retrain path, up to about 119 days plus GPU rental.**
- **Excluded**: counsel fees, GPU cost, programme and certificate fees, and the 10 to 25 day contingency above.

**Order.**
1. B5.1 and the start-now pieces: golden harness, edition mechanism, provenance report, configurable OSM endpoints.
2. Get D7 to D11 answered.
3. B5.2, B5.4, then B5.5 model by model.
4. B5.3.
5. B1 and B2.
6. B5.6, then B5.8, then B5.7.
7. B5.10 and B5.11.
8. B5.9 last, because it describes everything else.
9. Phase C, with two new gates: the edition manifest excludes every model D7 rejected, and the EULA and privacy policy are shown in the built app.

Record D7 to D11 in ADR-0011. B5.3 and B5.8 each get their own RFC first.

### Phase C — gate before any installer leaves the owner's machine

A short checklist in the release workflow summary (or `docs/RELEASING.md`):
generator `--check` green; release built by `release.yml` from a tag, not locally; bundle contains `licenses/` and both
notice files; About tab opens both files in the built app (the `verify-emulsion` skill can drive this); source bundle
attached (if B2 kept); LibRaw version-sync step green; counsel's answers to D1–D6 recorded (an ADR, e.g. ADR-0011
"Distribution licensing", so later work does not re-open them).

### What not to do

- Do not ship installers before Phase A is merged.
- Do not change the repository licence from MIT because of this.
- Do not state in the README that the project "complies"; say what is provided (texts, source, build guide).
- Do not modify the Lensfun XML files without reading the ShareAlike consequence first.

### Rough effort

Phase A: about a day across four small PRs. B1 and B2: about a day, mostly Windows iteration. B3: a day plus the
clean-machine test. B5 (only if proprietary): about 27 to 119 days, see its costed table. Phase A needs no decision and can start now.

## 4. Questions to send to counsel (with A1's inventory attached)

1. For a statically linked, MIT-licensed, publicly sourced app: is publishing the full source of each release plus
   build/relink instructions sufficient for LGPL-2.1 §6 (LibRaw) and LGPL-3.0 §4 (lensfun)? Does that change if the
   app later becomes proprietary?
2. LibRaw: LGPL-2.1 or CDDL-1.0 for our case, and does the Windows build from vcpkg's port need its own source offer?
   The vcpkg port also links other static libraries into the Windows build (jasper under the JasPer License 2.0, lcms under MIT,
   libjpeg-turbo under BSD-3-Clause and IJG, zlib under Zlib, and freeglut, whose port declares no licence; appendix: installer-inventory.md).
   Is reproducing their notices enough? Their copyright files are now in `licenses/` (`vcpkg-<port>-copyright.txt`; the pinned lcms 2.14 port has no features, so there is no GPL-labelled line at the pin).
   Their versions are pinned with the whole vcpkg `ports/` tree and CI fails if an installed version differs from the recorded one.
   Measured on the release `emulsion.exe` (appendix): only LibRaw and zlib's inflate code come from the vcpkg libraries; jasper, libjpeg-turbo and freeglut
   have no symbol in the image, and the lcms code in it is the Rust crate `lcms2-sys`'s own copy. Does a notice have to name a library that is linked
   but contributes no code? libjpeg-turbo's file also asks binary distributions to carry an IJG acknowledgement sentence; is that needed here?
   freeglut's port declares no licence; its copyright file is an MIT-style notice by Pawel W. Olszta.
3. Does a GitHub link satisfy source availability, or must each release carry the source (B2)?
4. CC-BY-SA 3.0 database embedded unmodified (gzipped) in the executable: what attribution form is enough, and does
   embedding make our binary an adaptation or a collection?
5. If signed/notarised: any Installation Information duty, and must we warn about re-signing?
6. Are the shipped notice files plus the About-tab sentence a sufficient "prominent notice"?
7. Does a model's training-data licence (non-commercial research data such as Places2, WIDER FACE, ImageNet) reach weights that
   a third party licensed as Apache-2.0 or MIT, and does an ImageNet-initialised backbone count? (Appendix: model-training-data.md.)
8. **AI-assisted authorship** (appendix: provenance.md). Git shows one human author identity. At `e5fcd3a26`, 463 of 888 commits on
   `main` carry a `Co-authored-by` trailer naming a Claude model; the appendix gives current counts, the models named with date ranges,
   and the share of added lines by area. (a) Who owns code produced with this assistance, and does copyright subsist in it, under the
   tool terms in force when each commit was made? (b) Is a commit trailer adequate evidence that assistance was or was not used, or
   should the project keep other records from now on? (c) Does the published MIT history limit a later proprietary licence for future
   versions (D8), and does the answer differ for AI-assisted commits? (d) In a relicensing, do the vendored `rsraw` and `rsraw-sys`
   code (MIT, patched) and the two committed model weight files need anything beyond their existing notices?
9. Are face embeddings (SFace) acceptable in a paid product under the biometric-data rules of the target markets?

## 5. Change plan: first slices (start now, no regret under any D1 answer)

One PR per slice, each with a `PROGRESS.md` entry in the house style and an explicit "Not verified" list. These run alongside Phase A
(A1 to A4) and none depends on D1. Slices 1 to 3 are documents or tests only.

**Slice 1: training-data terms recorded for every model** (`docs/model-training-data`).
- **Files**: each `models/*_MODEL.md` gets a "Training data and its terms" block (source URL, date read, "not retrieved" list); new
  `models/SEGMENT_MODEL.md` for SAM 2, which has no note today; `THIRD_PARTY_NOTICES.md` gets a "Training data" column;
  `bundled_models.rs` extends the notices test to require a non-empty training-data cell; ADR-0010 and `FACE_MODELS.md` get a dated
  note that the terms are now read; the appendix is the source.
- **Done when**: all eight models are covered, every claim has a source, no wording reads as a legal conclusion, the test passes.
- **Verified by**: `cargo test bundled_models`; a reviewer opens each URL, including the two the merge could not (Places2 archive,
  WIDER FACE); the diff touches only documents and that one test.

**Slice 2: golden behaviour fixture for lens matching** (`test/lens-profile-golden`).
- **Files**: `app/src-tauri/src/lens_profile.rs` (a test against `src/testdata/lens_profile_golden.json` and an `#[ignore]` regenerate
  test); the fixture holds about 300 queries (every camera and lens pair in the `test_image/` EXIF, a spread of focal length, aperture
  and distance, and naming variants: maker case, mount prefixes, zoom spellings, an unknown lens).
- **Done when**: matched profile identity is exact, coefficients agree within 1e-6, and the test runs on macOS and Windows CI.
- **Verified by**: `cargo test lens_profile`; a deliberately mutated constant must make it fail, recorded in the PR. It is the oracle
  for B5.3 and also guards crate upgrades. The fixture notes it came from a tool run, not copied source.

**Slice 3: provenance report** (`chore/provenance-report`).
- **Files**: `scripts/provenance_report.py` (read-only git: shortlog, committer split, co-author trailers, vendored directories);
  output in `docs/rfc/RFC-0038-appendix/provenance.md`; question 8 in section 4 (AI-assisted authorship) rewritten around it (question 7 is
  training data, so no new question is added).
- **Done when**: `--ref e5fcd3a26` reproduces 1 author, 888 commits and 463 trailer commits, and gives identical output on a second run.
  (`main` has moved on: the appendix is pinned to the commit named in its first lines. Status: done in `chore/provenance-report`.)
- **Verified by**: run it twice and diff; spot-check against `git shortlog -sne`.

**Slice 4: edition manifest and a "not included" state** (`feature/model-editions`).
- **Files**: `models/bundled-models.json` gains an `editions` field; `fetch-bundled-models.mjs` takes `--edition` and writes
  `bundled-models.edition.json`; the `*_models.rs` status functions report `not_in_edition` with no download offered; the panels show
  one sentence.
- **Done when**: a no-LaMa edition build shows "not included in this edition" in Remove, with no crash and no download offer; the
  default build is unchanged.
- **Verified by**: `cargo test`, Vitest, and a `verify-emulsion` run on the no-LaMa build. Also the route RFC-0029 names for
  removing a model if a licence reading is challenged. Not verified: catalogs that already hold face embeddings.
  Status: built in `feature/model-editions`; the decisions are in [ADR-0012](../adr/ADR-0012-model-editions.md) (ADR-0011 stays reserved for D7 to D11).

**Slice 5: configurable map-tile and geocoder endpoints** (`feature/osm-endpoints-configurable`).
- **Files**: `LibraryMapView.svelte` (the tile URL becomes a setting; attribution follows it); `geocode.rs` (endpoints from settings,
  User-Agent version from the package version); the settings store, the Map section of `SettingsDialog.svelte`, `docs/USER_GUIDE.md`.
- **Done when**: endpoints change without a code change, OSM attribution stays for the defaults, an invalid URL falls back to the default.
- **Verified by**: Rust and Vitest unit tests plus a manual map check (on the milestone-end "not verified" backlog). The OSM tile policy
  asks apps not to hard-code the tile URL, so this applies whatever D1 turns out to be.

Not in this list on purpose: `release.yml` and the source bundle (RFC Phase B), signing (needs accounts first), licensing code (needs
D9), and the `lensfun` rewrite itself (follows D1; its harness is slice 2).

## Provenance

Synthesised by the pstack arena from two independent plans (a Sonnet run and an Opus run; no cross-judge, because the
model pool offered no model from another family). Base: the Opus plan (more repo facts checked, six explicit decisions,
the `emulsion-ai` and vcpkg inventory questions, costs for the proprietary case). Grafted from the Sonnet plan: the CDDL
reading in D2, the `crate-type` artefact warning, the hand-run clean-machine rebuild, "what not to do", effort figures.
Spot-checked on 2026-11-14: PRD M8 still lists the licensing decision as open; the vcpkg libraw pin is in `ci.yml`; the
Lensfun database is embedded with `include_bytes!` in the crate's generated code; `emulsion-ai` is built by
`prepare-ai-helper.mjs`. Engineering planning, not legal advice.

Second arena run (2026-11-14), covering B5, the model appendix and section 5: a Sonnet run and an Opus run again; the Opus run is the base
(more primary sources opened, new findings on the D-FINE backbone, Places2 and WIDER FACE terms, OpenStreetMap policy and the updater).
Grafted from the Sonnet run: the 10 to 25 day contingency if the Lensfun database cannot ship as a data file, the SAM 2 caution about the
unread SA-1B text, and the point that earlier owner acceptances were for a free tool. Not taken: its kill-switch slice (the edition
manifest in slice 4 covers it) and its licence-texts slice (already Phase A). Corrected by a check: the Sonnet run's commit count mixed
all branches (911) with `main`; `main` has 888 commits and 463 with a Claude trailer. No cross-judge for the same reason as before.
