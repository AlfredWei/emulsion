# ADR-0011: Paid store distribution: the channel flag, the store edition and compile-time legal gating

- Status: **Accepted.** The decisions below are the owner's answers of 2026-11-18 and 2026-11-23 (owner messages, paraphrased), recorded by store plan PR S1. The mechanism is built in the same PR.
- Date: 2026-11-27
- Relates to: [RFC-0038](../rfc/RFC-0038-lgpl-and-database-licence-plan.md) (B5, the paid route), [RFC-0039](../rfc/RFC-0039-lensfun-clean-room-replacement.md) (the lens clean room, deferred), [ADR-0012](ADR-0012-model-editions.md) (model editions), [the store release plan](../store-release-plan.md), the licence research memo of 2026-11-18 (held outside the repository)

## Context

The owner sells Emulsion on the Mac App Store, paid upfront. RFC-0038 B5 is that route. Some features carry licence or data terms whose fit with a paid, closed build only a lawyer could settle, and the owner has no lawyer. The owner asked that every such feature is switched off at compile time in the store build, while the default build, the open source build and development builds keep every feature (owner message 2026-11-18, paraphrased). The licence research memo of 2026-11-18 names the features (its section 9). It is research, not legal advice, and it waits for the owner to accept its stated risk.

## Decisions recorded

Each answer is owner message 2026-11-18, paraphrased, unless the row says 2026-11-23 or 2026-11-26. The plan's "Program checklist" keeps the full wording and the open items.

| Decision | Answer |
| --- | --- |
| RFC-0038 D1. Who gets installers | Option (b), public and paid. Mac App Store first. |
| D2 and D3. LibRaw licence | The memo elects CDDL-1.0 as an engineering default. The only duty is to publish LibRaw's own source and say where (S5). Waits for the owner to accept the memo's risk. |
| D4. How source is made available | LibRaw's source in a public repository, tagged per release (S5). |
| D5. Lens database | The store edition ships no lens profiles. This ADR's Cargo feature removes the crate and its database. |
| D6. Signing and "Installation Information" | Goes away for the store build, because the LGPL-3.0 crate is not in it. |
| D7 and D11. Model data and face features | The recommended defaults. Weights whose main training data has non-commercial terms stay out, and face features stay out of the first store release. |
| D8. Relicensing | Approved. The repository goes private and future versions become proprietary. The memo names the MIT history a question for a lawyer (S5). |
| D9, D13 and D19. Windows activation, route and signing | Deferred with the Microsoft Store. |
| D10 and D14. Seller and accounts | The owner sells as an individual with the Apple Developer account already held. No D-U-N-S number. |
| D12. Mac price model | Paid upfront, one price, no trial, with an on-device purchase check (M8). |
| D15. The name | Chosen privately on 2026-11-23. It stays out of this repository until the owner says it is registered. |
| D16. Channel order | Mac App Store first. |
| D17. Map and place search | Off by default in release builds, on in debug builds, a setting turns them on (S9). Not compiled out. |
| D18. Architecture and minimum macOS | Apple silicon only, macOS 13 (raised 2026-11-23 for StoreKit 2 `AppTransaction`). |
| D20. Bundle ID | Chosen at the first upload (M6), as the reverse DNS of a neutral studio domain plus a label that is not the product name. Set only in the Mac store config. |
| D21. A failed purchase check | A read-only mode, never a terminated app. A grace period of one year when the check cannot run (2026-11-26). |

## Decision: what the store build compiles out, and how

"Compiled out" uses two mechanisms. Which one a feature uses depends on where its open question sits.

1. **Code and data inside the binary use a Cargo feature.** `lens-profiles` (on by default) holds the LGPL-3.0 `lensfun` crate, the CC-BY-SA 3.0 database it embeds through `Database::load_bundled()`, and the profile render path. Evidence, read on `main` at `cb2592f`: `lens_profile.rs` says it is the only file that touches the crate's types (its header, lines 1 to 14); production reaches it only through `lookup_lens_profile` and `warm_lens_db` in `lib.rs`. The render path in `develop_engine/lens.rs` and `gpu/shaders/lens.js` works from coefficients baked into the edit stack, but its profile kernels are described as ports of lensfun math (RFC-0039 section 3, regions 2 to 4), and whether that is LGPL-3.0 code is a lawyer's question (memo, section 10, question 3). So the profile half moved to `develop_engine/lens/profile.rs`, compiled only with the feature. Without it `lens/profile_absent.rs` gives `LensProfileData` no values, so a stored profile op renders without its profile part and stays in the catalog. The shader takes the same split through `VITE_LENS_PROFILES=off`, and Vite then drops the profile functions from the bundle.
2. **Model-driven features use the edition of ADR-0012.** The store edition includes `denoise` (NAFNet) and `segment` (SAM 2) only. It leaves out `inpaint` (LaMa, and S2 gives Remove its PatchMatch path), `detect` (D-FINE), `upscale` (Real-ESRGAN), `face` (YuNet and SFace) and `blink` (Face Mesh, which `blink_commands.rs` refuses without `face`, lines 40 to 57). ADR-0012 rejected a Cargo feature per family because it multiplies CI cells and cannot express a family with no file. That still holds, and the open questions attach to the weights and their training data, not to Emulsion's own code (RFC-0038 B5.5). The fetch script leaves the weights out of the bundle and no download path exists in the store build (below), so an excluded family cannot be reached.

**The channel.** `channel.rs` holds `Channel` (`Direct` or `MacAppStore`, chosen by the `channel-mas` feature) and `ChannelCaps`, one row per channel: `export_command_hook`, `model_downloads`, `custom_cache_dir`. `build.rs` sets one `cfg` per capability from the same table, and the code behind each is `#[cfg]`-gated, so the store binary holds no code that starts a user program (Apple guideline 2.5.2), fetches a model (2.4.5(iv)) or moves the cache. A test checks that the `cfg`s and `Channel::caps` agree. A `channel-mas` build always resolves to the `store` edition, whatever `EMULSION_EDITION` says, and a `compile_error!` stops a `channel-mas` build that keeps the default features.

**The frontend learns all of this from `model_edition`.** `EditionInfo` gains `channel`, `caps` and `compiled_out` (`["lens_profiles"]` without the feature). No command is added. `lookup_lens_profile` stays registered without the feature and returns `None`. The Lens Corrections panel shows "Lens profile correction is not included in this edition of Emulsion." in place of the Profile part, Settings hides Export Plugins and the cache move, and Settings, About names every left-out feature.

**The Manual distortion kernel.** Manual distortion is not a profile feature, but its inverse kernel (`lens_undist_poly3` and the WGSL `lensUndistPoly3`) was one of the ported regions. It was rewritten in a fresh agent session that read only a written spec of the model `Rd = Ru*(1 + k1*Ru^2)` and its contract, and no implementation. The rewrite replaces the old kernel in every build. See "Deviations" for the agreement check.

## Deviations from the store plan

- **The 0.05 px agreement rule failed as written, and the rewrite was adopted anyway.** Old against new on a 6000 x 4000 frame, 121 x 81 points, every 5th slider step: 168 of 401,841 points differed by more than 0.05 px (worst 0.19 px at slider -100), and 24 points converged in one kernel and not the other. Every difference is near the fold of the k1 < 0 model at the frame's corners, where the old kernel stopped at a loose tolerance after at most six Newton steps. Measured against the model itself, the old kernel's worst residual was 0.0126 px and the new one's 0.0002 px. The differences are the old kernel's error, so dropping the Manual distortion slider would have removed a working feature to keep a less accurate one. The unchanged `geometry.rs` tests pass.
- **The model manifest is not embedded whole.** `build.rs` writes only its `editions` object for `editions.rs`, so the store binary does not carry the download URLs of the files list.

## Consequences

- One codebase builds both channels: `cargo build` for the default build, `cargo build --no-default-features --features channel-mas` for the store build. The CI job `store-edition` runs the store tests and fails if `lensfun` enters the store dependency tree.
- `scripts/lanes/build-sandboxed.sh --channel mas` builds the store bundle with the store edition's models and `VITE_LENS_PROFILES=off`.
- A model that is not bundled reads `not_in_edition` in a channel without downloads, so the existing "not included" sentence covers it and no download dialog opens.
- Settings, About still credits Lensfun in the store build. The notices for each channel are S5's.

## Not verified

The store build on a Mac with an existing catalog that holds profile ops; the emulsion-ai helper built with store features (it does not use the library crate, measured by reading `src/bin/emulsion-ai/main.rs`); Windows; the agreement of the new WGSL kernel with the Rust one beyond naga validation and the live lanes.
