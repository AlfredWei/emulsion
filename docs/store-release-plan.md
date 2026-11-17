# Emulsion store release plan

The owner wants to sell Emulsion on the Mac App Store and the Microsoft Store, and asked for the todo list for the whole project.
That is RFC-0038 decision D1 option (b), public and paid. RFC-0038 section B5 (12 steps, 27 to 119 engineer-days) is now the real path, and this plan builds on it.
The rule the program enforces is that no build reaches a store until the RFC-0038 Phase C gate and this plan's store gate pass on that exact build.
The decisions only the owner or counsel can make come first, in **Arm the program**. Each names its recommended default and the PRs it gates.
PR ids in dependency order. Shared S0 to S11, Mac M1 to M7, Windows W1 to W6, release G1 to G3.
Total cost is about 50 to 168 engineer-days (guess), plus counsel, account approval and App Review time.
Decide D14 (entity and accounts), D7 with D11 (model policy) and D15 (the name) first. They gate the most work or the longest calendar time.

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a screenshot, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs the `pstack` plugin's `skills/poteto-mode/playbooks/autopilot-stack.md`. Agents build and verify each PR and stop at verified. The owner lands every PR. No agent merges. Autopilot-stack fits better than `playbooks/autopilot-full.md` because 17 of the 28 PRs wait on an owner or counsel answer, and the owner asked to review before anything lands (inferred from the brief). It fits better than `playbooks/orchestrate.md` because orchestrate's PR owners merge their own PRs (sourced, that playbook). Run it in waves. A wave is the set of PRs whose gates are answered, listed under **Spawn owners**.

Three sets matter for scheduling.

- Owner-or-counsel-gated. S1, S2, S3, S4, S5, S6, S9, S10, M6, M7, W3, W4, W5, W6, G1, G2, G3. A gated PR does not start until its decision is recorded in ADR-0011.
- Review-gated, because they change an interaction or need a human at the screen. S2, S3, S4, S9, S10, M1, M2, M3, M4, M5, M6, M7, W4, W5, W6, G2, G3.
- Need accounts or secrets that no agent holds. M6, M7, W3, W5, W6, G2, G3. The owner puts each secret into GitHub Actions secrets. Agents dispatch workflows that use the secrets and never read them.

Costs are engineer-days for one person who knows the codebase. Every cost is a guess unless a box says otherwise. RFC-0038 B5 figures are reused where a row matches. RFC-0039 moved B5.3 from 4 to 8 days to 6.5 to 12 (sourced, RFC-0039 section 7). The total is higher than B5's 27 to 119 because B5 has no sandbox, store packaging or submission work. It is lower than a plain sum would be because B5.1, the edition mechanism and the lens golden harness are already merged (measured, PROGRESS.md), and the Mac store needs no Developer ID notarisation. The Mac App Store alone, without W1 to W6 and G3, is about 38 to 142 engineer-days (guess).

Critical path. With the fallback answers recommended below, the path is the Mac sandbox chain S0, S1, M2, M4, M6, G1, G2, about 14 to 28 engineer-days (guess). With retrain answers to D7 and D11, model remediation leads instead. S0, S1, S3 (an SFace replacement up to 25 days), S9, S10, G1, G2 is up to about 45 engineer-days plus GPU time (guess). Counsel's answers and Apple enrolment run on calendar time beside both and are likely the real critical path (guess).

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

## Program checklist

### Arm the program

- [ ] State the protocol and this plan to the operator, then stop. Start execution only on the operator's explicit go.
- [ ] Get the owner's or counsel's answer to each decision below. Evidence is the owner's message in chat or counsel's letter, quoted into ADR-0011 by S1. A decision answered later is appended to ADR-0011 by the PR it gates.

| Decision | Who | Recommended default | What would change it | Gates |
| --- | --- | --- | --- | --- |
| D14. Entity and store accounts (extends D10) | Owner, accountant | One legal entity. Request its D-U-N-S number now. Enrol it as an Apple organisation and open a Microsoft company account. Apple needs a legal entity, a D-U-N-S number, a domain email and a public website for an organisation (sourced, Apple enrol page). Microsoft requires a company account for anyone selling in their trade (sourced, Store Policy 10.14). | No entity. Then enrol Apple as an individual (the seller name is the owner's legal name) and verify the Microsoft company account as an independent developer with documents (sourced, Partner Center account page) | M6, W3, W5, G2, G3 |
| D7. Model-data policy | Owner, counsel | Reject weights whose main training data has non-commercial terms. LaMa falls back to PatchMatch (S2). YuNet is swapped (S3). SFace is dropped (S3). Ask the Real-ESRGAN maintainer, and drop Upscale if the answer is unknown (S4). Counsel clears or rejects D-FINE's ImageNet backbone (S4). | Counsel says dataset terms do not reach the weights. Then S2, S3 and S4 shrink to notices. | S1, S2, S3, S4 |
| D11. Face features in a paid build | Owner, counsel | Leave face grouping out of the first store release. Keep the eye check with the swapped detector. | Counsel clears biometric duties in the target markets and SFace is replaced | S1, S3, S9 |
| D15. The name "Emulsion" | Owner, counsel | Keep it if the trademark searches are clear. A Mac photo catalog app named Emulsion by The Escapers sold on the Mac App Store in 2015 and had a 1.4.1 release in December 2016 (sourced, PetaPixel and Softpedia, secondary). No current listing was found (sourced, research). | A live mark in software classes, or Apple or Microsoft refusing the name. Rename before G2, because every listing asset carries it. | G2, G3 |
| D16. Channel order | Owner | Mac App Store first, Microsoft Store second. Apple collects the money and the tax, so the Mac build needs no payment or licence system (sourced, guideline 3.1.1). The sandbox spike already runs (measured). The owner's machine is a Mac, so the human-at-screen checks can run. | Apple enrolment stalls on D-U-N-S, or Microsoft confirms that it collects payment for a Paid MSI listing (then Windows is cheaper) | Wave order |
| D12. Mac price model | Owner | Paid upfront, one price tier, no In-App Purchase. M7 is skipped. | The owner wants a free trial. Then M7 builds the Price Tier 0 "XX-day Trial" item plus a paid unlock (sourced, guideline 3.1.1) | M7, G2 |
| D13. Windows route | Owner | Route A. The same signed MSI or EXE that direct download would use, listed in Partner Center, with the app's own payment and licence keys. Tauri builds MSI and EXE officially (sourced, Tauri Microsoft Store guide). Third-party commerce carries no Store fee (sourced, App Developer Agreement v8.11). | Microsoft states in writing that a Paid MSI or EXE listing has Microsoft collect the money. Then W4 and W5 shrink to a Store licence check. The listing page says a one-time price is "collected through in-app purchase" and names no fee or money flow, so reading that as the app's own payment is an inference. | W4, W5, W6, G3 |
| D2 and D3. LibRaw licence | Owner, counsel | Elect CDDL-1.0 (RFC-0038 B5.4 (a), 0.5 to 1 day) | Counsel prefers LGPL-2.1 with dynamic linking (B5.4 (b), 3 to 5 days) | S5 |
| D5. Lens database | Counsel | Ship the unmodified XML as a separate resource beside the binary (RFC-0039) | Counsel says it cannot ship beside a proprietary binary. Then the RFC-0038 B5.3 contingency applies (10 to 25 days, not researched) | S6 |
| D8. Relicensing | Owner, counsel | Future versions become proprietary. The repository goes private before the first store upload. | The owner keeps the source public. Selling a paid build of public source is not forbidden by either store's rules as read (inferred), but RFC-0038 B5 assumes a proprietary product. | S5, W6 |
| D9. Activation | Owner | Offline signed licence file (RFC-0038 B5.8, PRD local-first) | Only needed under D13 route A | W4 |
| D17. Map and place-search services | Owner | Online map and place search stay off until the user turns them on. Keep the OpenStreetMap defaults with attribution and the existing 1.1 s search throttle. Contract a paid provider later. | OpenStreetMap refuses commercial use, or review asks for a provider's retention terms (Apple privacy labels depend on them, sourced) | S9 |
| D18. Mac minimum OS and architecture | Owner | Apple silicon only, `minimumSystemVersion` at least 12.0 (sourced, Tauri App Store guide), raised to the oldest macOS the owner tests | Intel users matter. Universal adds an x86_64 helper and LibRaw build, 1 to 3 days (guess, audit S7) | M6 |
| D19. Windows code-signing route | Owner | Azure Artifact Signing if the entity qualifies, else an OV certificate. Prices were not retrieved (sourced, RFC-0038 B5.6). | Cost or eligibility | W3 |

- [ ] The owner starts the calendar-time actions now. No agent can do these.
  - [ ] Request the D-U-N-S number. Evidence is the D&B confirmation, as a screenshot. Apple says allow up to 5 business days for D&B and up to 2 more for Apple (sourced).
  - [ ] Enrol in the Apple Developer Program (99 USD a year, sourced). Evidence is a screenshot of the membership page with the Team ID.
  - [ ] Open the Microsoft Partner Center company account through storedeveloper.microsoft.com (no fee, sourced). Evidence is a screenshot of the verified account.
  - [ ] Reserve "Emulsion" in Partner Center (three months, sourced) and create the App Store Connect app record for bundle ID `dev.alfredwei.emulsion`. Evidence is one screenshot of each.
  - [ ] Run the USPTO, WIPO Global Brand Database and EUIPO TMview searches for "Emulsion" in software classes. Evidence is counsel's note or the saved result pages.
  - [ ] Send counsel RFC-0038 section 4 questions 1 to 9, RFC-0039 questions Q1 to Q7, and the store questions in Appendix C. Evidence is the sent letter.
  - [ ] Ask Microsoft Partner Center support whether a Paid MSI or EXE listing has Microsoft collect the payment. Evidence is the reply. It settles the inference under D13.
  - [ ] Ask the Real-ESRGAN maintainer what `realesr-general-x4v3` was trained on (RFC-0038 B5.5). Evidence is the issue link.
- [ ] Read these from trunk at program start. Re-read them at every tick.
  - [ ] `git show origin/main:docs/store-release-plan.md`
  - [ ] `git show origin/main:.claude/skills/verify-emulsion/SKILL.md`
  - [ ] `git show origin/main:docs/rfc/RFC-0038-lgpl-and-database-licence-plan.md`
  - [ ] `git show origin/main:docs/rfc/RFC-0039-lensfun-clean-room-replacement.md`
  - [ ] `git show origin/main:docs/adr/ADR-0012-model-editions.md`
  - [ ] The pstack copies of `skills/poteto-mode/playbooks/autopilot-stack.md`, `playbooks/autopilot-full.md`, `playbooks/opening-a-pr.md`, `playbooks/shipping.md` and `skills/swarm/SKILL.md`. pstack is a plugin, not part of this repository, so read them from the installed plugin version.
- [ ] On the operator's go, arm the audit tick as `/loop 1h` with the tick prompt below. Never leave the cadence to memory.
- [ ] Use this tick prompt, verbatim. "Re-read the execution playbook from trunk. Audit the operation against it and fix drift in this tick. Probe every active lane and judge progress by side effects only. Stand down a stuck lane and dispatch its replacement now. Then post a short status message to the operator in chat only when the audit found a tracked change that no earlier status message reported, such as a PR opened, a code-ready head, a round launched or closed, a verdict, a merge, a stuck agent and the action taken, a blocker added or cleared, or a decision only the operator can make. Name every such change and nothing else. Do not repeat a table, the merged list, or an unchanged blocker. If the audit found none, end the turn with no reply text. Either way, log this tick's row in your decision trail. The row names the items reported, or none."
- [ ] On the operator's hold or stand-down, send every owner a zero-writes order at once.

### Spawn owners

- [ ] Spawn one owner per PR with the full lifecycle the execution playbook names. Owners write code on `sonnet medium` (the `feature, refactoring` line of the pstack model rule). S6 and M2 go to `opus high` (the `hardest tasks` line).
- [ ] Follow this dependency graph. Start dependent work only after its parent merges, or base it on the parent branch when the execution playbook stacks.
  - [ ] Wave 0, no decision needed. S0, S7, S8, M1 and W1 are independent and first. All branch from `main`. RFC-0039 step 1 may also start (RFC-0039 section 10).
  - [ ] Wave 1, after D7, D11, D12 and D13 are answered. S1 after S0.
  - [ ] Wave 2. S2, S3 and S4 after S1. S11 after S1. M2 after S1 and M1. M5 after S1.
  - [ ] Wave 3. M3 and M4 after M2. S9 after S3 and D17. S5 after D2 and D8. S6 after RFC-0039 steps 1 to 5 and D5.
  - [ ] Wave 4. M6 after S11, M2, D14 and D18. W2 after S11 and W1. S10 after S5, S6, S9 and counsel's texts.
  - [ ] Wave 5. M7 after M6, only if D12 is the trial. W3 after W2, D14 and D19. W4 after S1, D9 and D13.
  - [ ] Wave 6. W5 after W4. W6 after W3, W4 and D8. G1 after S2, S3, S4, S10, M6 and W3.
  - [ ] Wave 7. G2 after G1, D12, D14 and D15. G3 after G1, W5, W6 and D15.
- [ ] Hold the file boundaries. Two PRs in the same wave may run in parallel only on disjoint files.
  - [ ] S0 touches only `.github/workflows/lanes.yml`, `scripts/lanes/**`, `app/e2e/wdio.sandbox.conf.js`, `app/e2e/specs/{perf-probe,sandbox-smoke}.e2e.js` and `app/src-tauri/entitlements/lane-*.entitlements`.
  - [ ] Only S1 creates `app/src-tauri/src/channel.rs` and the `store` edition. S3 and S4 change `models/bundled-models.json` after S1 merges, one at a time, S3 first.
  - [ ] M2 owns `app/src-tauri/src/sandbox_access/**`, `catalog/folder_grants.rs` and the setup block of `lib.rs`. W1 owns the data-directory helpers of `lib.rs` and `storage.rs`. They touch different functions of `lib.rs`, so each rebases on the other's merge.
  - [ ] S5 and S6 both edit `scripts/gen_third_party_libraries.py`. S5 lands first.
  - [ ] S11 owns `.github/workflows/release.yml` as a reusable workflow. M6 adds only `release-mas.yml`. W3 adds only `release-msstore.yml`. Separate files keep the two store pipelines from serializing on one file.
- [ ] Hold the review gate. S2, S3, S4, S9, S10, M1, M2, M3, M4, M5, M6, M7, W4, W5, W6, G2 and G3 wait for the operator's review in chat with screenshots and a video before merge.

### PR mechanics, for every PR

- [ ] Resolve the forge once. Default to `gh`; if `command -v origin` succeeds and Origin can resolve the repository, use `origin pr` for every PR operation. Record any fallback to `gh`. Never require `gt`.
- [ ] Open the PR ready, never draft, per **Opening a PR**. Use the run's built-in PR tool when it has one, else `origin pr create --status open --base <base-branch>` or `gh pr create --base <base-branch>` according to the resolved forge. A stack child targets its parent branch.
- [ ] Run the repo's lint and typecheck once before the PR-facing push (`npm run check`, `npx vitest run`, `cargo test` in `app/src-tauri`). Push with hooks on.
- [ ] Run `/deslop` before each commit and `/no-comments` before review.
- [ ] Triage every Bugbot and security-reviewer comment per the pstack `skills/poteto-mode/references/bugbot-triage.md`.
- [ ] Add a `PROGRESS.md` entry in the house style with a "Not verified" list, and update `docs/PROJECT_STRUCTURE.md` when a module moves (owner's standing practice).
- [ ] Rebase onto current trunk before the code-ready report and babysit. Keep that merge base in fix rounds. Rebase again only at merge prep, on a `git merge-tree` conflict with trunk, on a CI failure that comes from a change on trunk, or when a known concurrent change to the same files lands.

### Verdict and merge, for every PR

- [ ] A full swarm runs once, at the code-ready head. Another full swarm runs only for new behavior, a redesign, or a large new diff. A rebase, a merge-conflict resolution, or a minor fix gets a single verifier at the new head, and a wording-only change gets none. Run each full swarm per the pstack `skills/swarm/SKILL.md`. One gates lane. The ten live lanes from the PR's **Verify, live** block. The perf lane from its **Verify, perf** block. Two or more audit lanes, each with its own focus, that read the diff and the receipts and distrust the PR body. The root audits the receipts in the merge-ready report before the verdict.
- [ ] Judge every finding by the execution playbook's blocking rule. The verdict is clean when none blocks. A failure on trunk behavior in a lane marked `Regression lane against trunk.` is recorded against trunk and does not block the PR. A new head gets a fresh verdict, except for results that stay valid under the patch-id rule in `playbooks/shipping.md`.
- [ ] Stop at verified. The root lists the PR with its verdict for the operator. The operator lands the stack bottom-up. A restack that leaves a PR's `git patch-id` unchanged keeps its verdict (the patch-id rule in `playbooks/shipping.md`). Nobody pushes after the handoff without the operator's go.

### Boot recipe, for every live lane

No cloud VM in this program can run the macOS app, so lanes run on GitHub Actions runners or on the owner's Mac. S0 builds the tooling below. Until S0 merges, S0's own lanes run its branch's scripts.

- [ ] `git fetch origin <head-branch> && git checkout <head SHA>`.
- [ ] Pick the surface the lane names.
  - [ ] **Debug build.** `.claude/skills/verify-emulsion/build.sh`, then `.claude/skills/verify-emulsion/drive.sh <spec>`. Unsandboxed, direct channel, a throwaway `HOME`. Ready when the spec reporter prints its first passing line.
  - [ ] **Sandbox build.** `scripts/lanes/build-sandboxed.sh --channel <direct|mas> --bundle-id dev.alfredwei.emulsion.lane<n>`, then `npx wdio run app/e2e/wdio.sandbox.conf.js --spec <spec>`. Each lane gets its own bundle ID, so each gets its own container. The embedded WebDriver listens on one fixed port, so lanes on one Mac run one at a time.
  - [ ] **Runner lane.** `gh workflow run lanes.yml -f ref=<head SHA> -f os=<macos-latest|windows-latest> -f spec=<spec> -f channel=<channel>`. Each dispatch is its own fresh runner. Windows lanes always use this.
  - [ ] **Release artifact.** The bundle that S11's `release.yml` built at the head SHA, downloaded with `gh run download`.
- [ ] Deliver input only through the WebDriver specs of `verify-emulsion`, the commands `lanes.yml` runs, or shell commands on a downloaded artifact. Native file pickers cannot be driven (sourced, `verify-emulsion` SKILL.md). Read-only diagnostics are `codesign -d`, `nm`, `pkgutil`, `xattr`, `ls`, `sqlite3` on a copy of the catalog, `/usr/bin/log show` and `Get-AuthenticodeSignature`.
- [ ] Save every screenshot or log to `/tmp/swarm-<pr-id>/worker-<n>/<slug>.<png|txt>` and return the paths with the report. Runner lanes upload them as artifacts first.

## Add the lane harness and perf probe (S0, shared)

**Depends on.** None. No owner gate. Cost 1.5 to 2.5 engineer-days (guess). It is the lever every later lane uses.

**Files.**

- [ ] Create `.github/workflows/lanes.yml` with `workflow_dispatch` inputs `ref`, `os`, `spec`, `channel` and `bundle_id`.
- [ ] Create `scripts/lanes/build-sandboxed.sh`, `scripts/lanes/perf-interleave.sh` and `app/src-tauri/entitlements/lane-drivable.entitlements` (the spike's `main-drivable.entitlements`).
- [ ] Create `app/e2e/wdio.sandbox.conf.js` and `app/e2e/specs/sandbox-smoke.e2e.js`, ported from `spike/mas-sandbox` at `b8cd7c9`.
- [ ] Create `app/e2e/specs/perf-probe.e2e.js`.

**Build.**

- [ ] `build-sandboxed.sh` builds the drivable bundle the way the spike did (`tauri build --features wdio-webdriver --bundles app` with the e2e and bundle configs), sets a per-lane `identifier` through a merged `--config`, signs `emulsion-ai` with app sandbox plus inherit and then the app. It signs ad hoc unless `LANE_SIGN_ID` names an identity.
- [ ] `sandbox-smoke.e2e.js` asserts the spike's measured facts. The data directory is inside the container. The helper reaches `running:true`. An import from outside the container fails.
- [ ] `perf-probe.e2e.js` writes `perf.json` with `import_ms` for the 8 files in `test_image/`, `first_grid_ms` from launch to the first grid cell, and `export_ms` for one JPEG export.
- [ ] `perf-interleave.sh <trunk-sha> <head-sha> <metric>` runs five trunk and head pairs in alternating order and prints both medians and the trunk spread.

**You see.**

- [ ] A `lanes.yml` run of `sandbox-smoke` on `macos-latest` ends green and uploads `sandbox-smoke.png` and `entitlements.txt`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/lanes/perf-interleave.sh --self-test` checks the median and spread on fixed inputs. Run `shellcheck scripts/lanes/*.sh && scripts/lanes/perf-interleave.sh --self-test`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `golden-path` on the debug build at trunk and at head. Trunk has no lane harness, so record that and gate that `golden-path` also passes as a runner lane at head. Save `golden-path.png`. Pass when all three runs pass and show the same Library grid.
- [ ] Lane 2. Runner lane, `macos-latest`, `sandbox-smoke`, ad hoc signing. Save `smoke-adhoc.png`. Pass when the container folder exists and all three spike facts hold.
- [ ] Lane 3. Sandbox build on the owner's Mac with `LANE_SIGN_ID` set to the Apple Development identity, `sandbox-smoke`. Save `smoke-devid.png`. Pass when the results equal lane 2.
- [ ] Lane 4. Two sandbox builds with bundle IDs `lane4a` and `lane4b`, each importing a different fixture into its container. Save `two-containers.txt`. Pass when two container folders exist and each catalog holds only its own import.
- [ ] Lane 5. `codesign -d --entitlements :-` on the lane app and its helper. Save `entitlements.txt`. Pass when the app has exactly app sandbox, user-selected read-write, network client and network server, and the helper has exactly app sandbox and inherit.
- [ ] Lane 6. Runner lane, `perf-probe` at trunk three times. Save `perf-spread.txt`. Pass when `import_ms` varies by less than 15 percent across the runs.
- [ ] Lane 7. `perf-interleave.sh` with trunk and head set to the same SHA. Save `perf-null.txt`. Pass when the head to trunk ratio is between 0.9 and 1.1.
- [ ] Lane 8. Runner lane, `windows-latest`, `golden-path`. Save `win-golden.png`. Pass when the spec passes and the screenshot artifact is uploaded.
- [ ] Lane 9. Runner lane of a throwaway spec with a failing assertion. Save `red-run.txt`. Pass when the run ends red and still uploads its screenshot.
- [ ] Lane 10. `gh run download` of lanes 2, 6 and 8 into `/tmp/swarm-S0/worker-10/`. Save `artifact-list.txt`. Pass when every file the boot recipe names is present.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the debug build. S0 changes no app code, so the metric must not move. Diff-added work is the `lanes.yml` wall time for `sandbox-smoke`.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` on `macos-latest`. Trunk lacks the probe spec, so the head's spec file is copied onto the trunk checkout for the trunk runs.
- [ ] Baseline. Record the trunk `first_grid_ms` median first.
- [ ] Rule. Fail if the head median exceeds the trunk median by more than 10 percent. Fail if a `sandbox-smoke` runner lane takes longer than 30 minutes.

**Review gate.** None. S0 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Add the channel flag and the store edition, and record the decisions (S1, shared)

**Depends on.** S0. Owner-gated on D7, D11, D12 and D13. Cost 1.5 to 2.5 engineer-days (guess). One codebase makes the direct, Mac store and Windows store builds.

**Files.**

- [ ] Create `app/src-tauri/src/channel.rs` and add features `channel-mas` and `channel-msstore` to `app/src-tauri/Cargo.toml`.
- [ ] Edit `app/src-tauri/src/export_plugin.rs`, the model download commands in `lib.rs` and the `*_models.rs` files, and `storage.rs` `move_cache_dir`.
- [ ] Edit `models/bundled-models.json` (new `store` edition) and `app/src/lib/api/system.js` plus the Settings rows that read the channel.
- [ ] Create `docs/adr/ADR-0011-paid-store-distribution.md` with every answered decision of RFC-0038 and of this plan.

**Build.**

- [ ] `Channel` is `Direct`, `MacAppStore` or `MicrosoftStore`, chosen by Cargo feature. `ChannelCaps` is one table row per channel with `export_command_hook`, `model_downloads`, `custom_cache_dir`, `own_licence`, `self_update` and `launch_licence_screen`. `compile_error!` fires when both store features are on.
- [ ] The Mac store binary compiles out `export_plugin` and every model download command, so it holds no code that starts a user program or fetches a model (Apple guidelines 2.5.2 and 2.4.5(iv), sourced). A Cargo feature is used here, unlike ADR-0012, because the code must be absent from the binary.
- [ ] The `store` edition lists only the families ADR-0011 allows. Store builds bundle every model of the edition and download none (audit S1).
- [ ] New `channel_info` command. Settings hides Plugins, download buttons and "Move cache" when the cap is off.

**You see.**

- [ ] In the Mac store sandbox build, Settings has no Plugins section, and `nm` on `emulsion` finds no `export_plugin` symbol.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `channel.rs` tests each row of the caps table. The `editions` test asserts `store` lists exactly the ADR-0011 families. Run `cargo test && cargo test --features channel-mas && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `golden-path` on the debug build, default channel, at trunk and head. Save `golden.png`. Pass when both pass with the same grid.
- [ ] Lane 2. Sandbox build, channel `mas`, open Settings. Save `mas-settings.png`. Pass when Plugins and "Move cache" are absent.
- [ ] Lane 3. `nm -gU` and `strings` on the `mas` binary for `export_plugin`, `huggingface.co` and `media.githubusercontent.com`. Save `mas-symbols.txt`. Pass when none is found.
- [ ] Lane 4. Sandbox build, `mas`, `segment-model` spec. Save `mas-segment.png`. Pass when the status is `ready` from `Resources/models` and no download control exists.
- [ ] Lane 5. Sandbox build, `mas`, open a feature whose family `store` excludes. Save `mas-excluded.png`. Pass when it shows "{Feature} is not included in this edition of Emulsion." and no dialog.
- [ ] Lane 6. Runner lane, `windows-latest`, channel `msstore`, Settings. Save `ms-settings.png`. Pass when Plugins is present and download buttons are absent.
- [ ] Lane 7. Debug build, default channel, Settings. Save `direct-settings.png`. Pass when Plugins, downloads and "Move cache" are present as on trunk.
- [ ] Lane 8. `cargo build --features channel-mas,channel-msstore`. Save `both-features.txt`. Pass when the build fails with the `compile_error!` text.
- [ ] Lane 9. Sandbox build, `mas`, call `set_cache_dir` from the spec. Save `mas-cache-refused.txt`. Pass when the command returns a refusal and the cache root is unchanged.
- [ ] Lane 10. Sandbox build, `mas`, Settings, About. Save `mas-about.png`. Pass when the edition line names exactly the excluded features.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the default channel at trunk and head. Diff-added work is the size of the `mas` `emulsion` binary.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` on `macos-latest`, and `stat -f %z` on both binaries.
- [ ] Baseline. Record the trunk `first_grid_ms` median and the default binary size first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent. Fail if the `mas` binary is larger than the default binary.

**Review gate.** None. S1 is not review-gated. The default channel is unchanged, and store channels reuse the existing "not included" sentence.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Make Remove work without LaMa in the store edition (S2, shared)

**Depends on.** S1. Owner-gated on D7. Cost 1 to 30 engineer-days (RFC-0038 B5.5 LaMa row, guess). Route (i) PatchMatch is 1 to 2 days. Route (ii) retraining is 15 to 30 days plus GPU. Skip this PR if counsel clears LaMa.

**Files.**

- [ ] Edit `app/src-tauri/src/removal_commands.rs` and the Remove status in the frontend.
- [ ] Create `docs/adr/ADR-0013-remove-without-lama.md`, which records the reversal of ADR-0010.
- [ ] Edit `THIRD_PARTY_NOTICES.md` only on route (ii), for the retrained model's training data.

**Build.**

- [ ] When `inpaint` is not in the edition, Remove runs `inpaint_patchmatch.rs` and reports `removal_model_status` as a new `patchmatch` state instead of `not_in_edition`.
- [ ] On route (ii), the retrained weights join `inpaint` with their training-data row, and the bake-off from RFC-0032 section 9 is re-run.

**You see.**

- [ ] In the `store` edition, Remove fills a brushed area, and no "not included" sentence appears.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A `removal_commands` test asserts the engine choice per edition. Run `cargo test removal`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `develop-remove-tool-real-model` on the default edition at trunk and head. Save `lama.png`. Pass when both pass and LaMa ran.
- [ ] Lane 2. Sandbox build, `store` edition, `develop-remove-tool` on `Field-corn-Liechtenstein-landscape.jpg`. Save `store-remove.png`. Pass when the patch is applied and no "not included" text is in the DOM.
- [ ] Lane 3. Same, then export one JPEG. Save `store-remove-export.png`. Pass when the exported file shows the patch.
- [ ] Lane 4. Open, in the `store` build, a catalog copy holding a LaMa patch made by the default build. Save `old-patch.png`. Pass when the stored patch still renders (ADR-0012 rule 4).
- [ ] Lane 5. `store` build, remove, then undo and redo. Save `undo-redo.png`. Pass when History lists one Remove entry and redo restores it.
- [ ] Lane 6. `store` build, `develop-removal-no-longer-fits`. Save `no-longer-fits.png`. Pass when the spec passes.
- [ ] Lane 7. Runner lane, `windows-latest`, `store` edition, `develop-remove-tool`. Save `win-remove.png`. Pass when the patch is applied.
- [ ] Lane 8. `ls Contents/Resources/models` of the `store` release artifact. Save `store-models.txt`. Pass when no LaMa file is present on route (i).
- [ ] Lane 9. `store` build, a selection over 25 percent of the frame. Save `large-remove.png`. Pass when the run finishes and progress is shown.
- [ ] Lane 10. LaMa and PatchMatch results side by side on the same mask. Save `compare.png`. Pass when both images exist for the owner's review.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Remove run time in ms on one fixed mask. Trunk runs LaMa only, so the PatchMatch path is diff-added work.
- [ ] Probe. A `perf-probe` case that runs Remove on the fixed mask, through `perf-interleave.sh`, default edition at trunk and head, then `store` at head only.
- [ ] Baseline. Record the trunk LaMa time first.
- [ ] Rule. Fail if the default-edition time exceeds trunk by more than 10 percent. Fail if the `store` PatchMatch run exceeds 5 s at preview size (absolute budget, guess).

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2 and lane 10 screenshots into `/tmp/store-review/S2-review-remove.png` and `/tmp/store-review/S2-review-compare.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S2-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Swap the face detector and leave face grouping out (S3, shared)

**Depends on.** S1. Owner-gated on D7 and D11. Cost 3 to 29 engineer-days (RFC-0038 B5.5, YuNet 2 to 4 plus SFace 1 to 25, guess). The low end drops SFace. The high end trains a replacement.

**Files.**

- [ ] Edit `app/src-tauri/src/face_models.rs`, the face detection call sites and `blink_pass.rs`.
- [ ] Edit `models/bundled-models.json`. Split the `face` family into `face-detect` (the new bundled detector) and `face-group` (SFace).
- [ ] Edit `models/FACE_MODELS.md` and `THIRD_PARTY_NOTICES.md`.

**Build.**

- [ ] First read the MediaPipe face detector's model card (RFC-0038 B5.5). If its terms fail D7, stop and report.
- [ ] Load the new detector in `tract` in place of YuNet. The eye check and face grouping both call it.
- [ ] The `store` edition includes `face-detect` and leaves out `face-group`, per D11.

**You see.**

- [ ] In the `store` edition the Cull eye check flags closed eyes, and face grouping shows its "not included" sentence.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A parity test on the blink fixture set compares the new detector's hits with YuNet's. Run `cargo test face`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `similar-eyes-check` at trunk (YuNet) and head (new detector). Save `eyes.png`. Pass when the same photos are flagged.
- [ ] Lane 2. Sandbox build, `store`, eye check on `Smiling-woman-pink-shirt-portrait.jpg`. Save `store-eyes.png`. Pass when one face is found and the result shows.
- [ ] Lane 3. Same on `Szechenyi-Chain-Bridge-Budapest-night.jpg`. Save `night.png`. Pass when no face is reported.
- [ ] Lane 4. `store` build, open face grouping. Save `group-excluded.png`. Pass when it shows the "not included" sentence and no download.
- [ ] Lane 5. `store` build on a catalog copy that holds face rows from a default build. Save `old-faces.png`. Pass when the rows stay readable and no new scan runs.
- [ ] Lane 6. `/usr/bin/log show` and the spec's network log during lanes 2 to 4. Save `no-download.txt`. Pass when no request reaches `media.githubusercontent.com`.
- [ ] Lane 7. Runner lane, `windows-latest`, `store`, eye check. Save `win-eyes.png`. Pass when lane 2's result repeats.
- [ ] Lane 8. Default edition, face grouping on the portrait fixture. Save `default-group.png`. Pass when grouping still works with the new detector.
- [ ] Lane 9. `Red-eye-flash.jpeg` eye check. Save `red-eye.png`. Pass when the face is found.
- [ ] Lane 10. `ls Contents/Resources/models` of the `store` artifact. Save `store-face-models.txt`. Pass when the detector is present and SFace and YuNet are absent.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Eye-check time per image over the 8 fixtures, at trunk (YuNet) and head (new detector).
- [ ] Probe. A `perf-probe` case for the eye check, through `perf-interleave.sh`.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 20 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2 and lane 4 screenshots into `/tmp/store-review/S3-review-eyes.png` and `/tmp/store-review/S3-review-group.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S3-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Settle Upscale and person detection for the store edition (S4, shared)

**Depends on.** S1, then S3 for `models/bundled-models.json`. Owner-gated on D7 and on the Real-ESRGAN maintainer's answer. Cost 0.5 to 13 engineer-days (RFC-0038 B5.5, Real-ESRGAN 0.5 to 10 plus D-FINE 0 to 3, guess). Skip this PR if counsel clears both models.

**Files.**

- [ ] Edit `models/bundled-models.json` (`store` edition) and `models/UPSCALE_MODEL.md`.
- [ ] Edit the person preset code that calls D-FINE, only if counsel rejects the ImageNet backbone.

**Build.**

- [ ] Real-ESRGAN. If the training set stays unknown, the `store` edition leaves out `upscale`. If the owner funds a retrain, SRVGGNetCompact is retrained on CC0 or CC BY photos and joins `upscale`.
- [ ] D-FINE. If counsel rejects the backbone, the person preset falls back to SAM 2 or leaves the `store` edition.

**You see.**

- [ ] In the `store` edition, Enlarge and the person preset each either work or show the "not included" sentence, as ADR-0011 says.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The `editions` test asserts the `store` families. Run `cargo test editions && npx vitest run scripts`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `develop-adaptive-person-real-model` on the default edition at trunk and head. Save `person.png`. Pass when both produce the same mask.
- [ ] Lane 2. Sandbox build, `store`, Enlarge on the landscape fixture. Save `store-enlarge.png`. Pass when it matches ADR-0011, a result or the sentence.
- [ ] Lane 3. Sandbox build, `store`, person preset on the portrait fixture. Save `store-person.png`. Pass when it matches ADR-0011.
- [ ] Lane 4. `store` build, a catalog copy holding an Enlarge result from a default build. Save `old-enlarge.png`. Pass when the stored file opens.
- [ ] Lane 5. Runner lane, `windows-latest`, `store`, lanes 2 and 3. Save `win-s4.png`. Pass when results equal the Mac lanes.
- [ ] Lane 6. Default edition, Enlarge. Save `default-enlarge.png`. Pass when it works as on trunk.
- [ ] Lane 7. `store` build, Settings, About. Save `store-about.png`. Pass when the edition line lists exactly what is left out.
- [ ] Lane 8. `ls Contents/Resources/models` of the `store` artifact. Save `store-models.txt`. Pass when only allowed files are present.
- [ ] Lane 9. On a retrain, the bake-off images of old and new weights. Save `bakeoff.png`. Pass when both exist. Otherwise record that no retrain ran.
- [ ] Lane 10. `library-adaptive-batch` on the `store` build. Save `batch.png`. Pass when the batch completes or skips excluded steps with the sentence.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Person-mask time and Enlarge time on one fixture, default edition, trunk and head.
- [ ] Probe. `perf-probe` cases for both, through `perf-interleave.sh`.
- [ ] Baseline. Record both trunk medians first.
- [ ] Rule. Fail if either head median exceeds trunk by more than 10 percent. A replacement model gets an absolute budget of twice the trunk time (guess).

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2 and lane 3 screenshots into `/tmp/store-review/S4-review-enlarge.png` and `/tmp/store-review/S4-review-person.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S4-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Change the licence terms for the paid build (S5, shared)

**Depends on.** None in code. Owner-gated on D8, D2 and counsel's answers to RFC-0038 questions 2 and 8. Cost 1.5 to 3 engineer-days (RFC-0038 B5.2 1 to 2 plus B5.4 (a) 0.5 to 1, guess). Route (b) adds 3 to 5. B5.1 is done (measured, PROGRESS slice 1 and A1).

**Files.**

- [ ] Edit `LICENSE`, the `license` fields in `app/package.json` and `app/src-tauri/Cargo.toml`, `README.md` and line 3 of `THIRD_PARTY_NOTICES.md`.
- [ ] Edit `scripts/gen_third_party_libraries.py`, `THIRD_PARTY_LIBRARIES.md` and `licenses/README.txt`.
- [ ] Edit the About copy in `app/src/lib/components/SettingsDialog.svelte`.

**Build.**

- [ ] Apply counsel's relicensing text to the files RFC-0038 B5.2 lists.
- [ ] Add the "elected licence" column. LibRaw's operative text becomes `LibRaw-LICENSE.CDDL` (RFC-0038 B5.4 (a)).

**You see.**

- [ ] `THIRD_PARTY_LIBRARIES.md` names CDDL-1.0 as LibRaw's elected licence, and About states the new product licence.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The generator self-test gains an election case. Run `python3 scripts/gen_third_party_libraries.py --self-test && python3 scripts/gen_third_party_libraries.py --check && cargo test --lib bundled_models`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `settings-about-open` on the debug build at trunk and head. Save `about.png`. Pass when both pass and head shows the new licence sentence.
- [ ] Lane 2. Release artifact, `ls Contents/Resources/licenses`. Save `licenses.txt`. Pass when `LibRaw-LICENSE.CDDL` is present and `README.txt` names it operative.
- [ ] Lane 3. Debug build, "Open library licences". Save `libraries-open.png`. Pass when no error line appears.
- [ ] Lane 4. Runner lane, `windows-latest`, `--check`. Save `win-check.txt`. Pass when it exits 0.
- [ ] Lane 5. `grep -rn "MIT"` over the files B5.2 lists. Save `mit-left.txt`. Pass when only vendored `rsraw` notices and third-party texts still say MIT.
- [ ] Lane 6. Release artifact, open `THIRD_PARTY_LIBRARIES.md`. Save `libraries-md.txt`. Pass when the election column is filled for LibRaw and `r-efi`.
- [ ] Lane 7. Sandbox build, `mas`, About. Save `mas-about.png`. Pass when the same sentence shows.
- [ ] Lane 8. Runner lane, `windows-latest`, About. Save `win-about.png`. Pass when the same sentence shows.
- [ ] Lane 9. Throwaway edit of `licenses/LibRaw-LICENSE.CDDL`, then `--check`. Save `drift.txt`. Pass when `--check` exits 1 and names the file.
- [ ] Lane 10. `cat app/package.json | grep license` and `cargo metadata` for the app crate. Save `license-fields.txt`. Pass when both carry counsel's identifier.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms`, the one metric a text change could move if the About tab loaded eagerly. Bundle size of `licenses/`.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` and `du -k` of `licenses/`.
- [ ] Baseline. Record the trunk values first.
- [ ] Rule. Fail if `first_grid_ms` exceeds trunk by more than 10 percent, or `licenses/` grows by more than 100 KB.

**Review gate.** None. S5 is not review-gated. It changes text, not an interaction.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Remove the lensfun crate per RFC-0039 (S6, shared)

**Depends on.** RFC-0039 steps 1 to 5, each its own PR under RFC-0039's build order, then S5 for the generator. Counsel-gated on D5 and RFC-0039 Q1 to Q7. Cost 6.5 to 12 engineer-days for all of RFC-0039, up to about 14 (sourced, RFC-0039 section 7). This section is RFC-0039 step 6.

**Files.**

- [ ] Edit `app/src-tauri/Cargo.toml` (drop `lensfun`) and the fixture generator, rewritten over `lens_db`.
- [ ] Edit `scripts/gen_third_party_libraries.py` (tables and `COPYLEFT_ALLOWED`) and the `bundled_models` licence test.
- [ ] Delete `licenses/lensfun-*`.
- [ ] Edit the About sentence in `SettingsDialog.svelte`.

**Build.**

- [ ] Remove the crate after `lens_db` and the rewritten kernels pass RFC-0039's oracles.

**You see.**

- [ ] `cargo tree -i lensfun` reports no such package, and lens profile corrections look the same.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The golden fixture, the hold-out fixture and `geometry.rs` tests pass. Run `cargo test lens && python3 scripts/gen_third_party_libraries.py --check`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Open `exif-shot-info.jpg` with its lens profile on the debug build at trunk (crate) and head (`lens_db`). Save `profile.png`. Pass when the corrected previews match within 0.05 px on the frozen geometry oracle.
- [ ] Lane 2. Same photo, export one JPEG at head. Save `profile-export.png`. Pass when the export matches the preview.
- [ ] Lane 3. `nm` on the release `emulsion` binary. Save `nm-lensfun.txt`. Pass when no `lensfun` symbol is found.
- [ ] Lane 4. Release artifact, `ls Contents/Resources/lensfun-db` and `shasum -a 256`. Save `db-hashes.txt`. Pass when every hash equals the pinned list.
- [ ] Lane 5. Debug build, About. Save `about.png`. Pass when Lensfun the library is no longer named and the database attribution remains.
- [ ] Lane 6. Runner lane, `windows-latest`, lane 1 at head. Save `win-profile.png`. Pass when the preview matches the Mac result.
- [ ] Lane 7. Debug build, Manual lens corrections on the landscape fixture. Save `manual.png`. Pass when they behave as on trunk.
- [ ] Lane 8. Sandbox build, `mas`, lane 1 at head. Save `mas-profile.png`. Pass when the profile loads from the bundle inside the sandbox.
- [ ] Lane 9. Debug build, a camera with no profile. Save `no-profile.png`. Pass when the UI says no profile was found, as on trunk.
- [ ] Lane 10. `cargo tree -i lensfun`. Save `tree.txt`. Pass when it reports no such package.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `warm_lens_db` load time in a release build, and lens-corrected preview render time.
- [ ] Probe. The RFC-0039 step 4 measurement for load time, and a `perf-probe` case for the preview, through `perf-interleave.sh`.
- [ ] Baseline. Record trunk's 42 ms load time (sourced, RFC-0039 O3) and the trunk render median.
- [ ] Rule. Fail if load time exceeds 100 ms (absolute budget, guess) or the render median exceeds trunk by more than 10 percent.

**Review gate.** None. S6 is not review-gated. The output must match trunk, and lane 1 proves it.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Read the bundled models in place (S7, shared)

**Depends on.** None. No owner gate. Cost 1 to 2 engineer-days (audit S2, guess). Today the app copies about 569 MB of shipped models into the cache root at startup (measured, audit S2), which doubles disk use inside the Mac container.

**Files.**

- [ ] Edit `app/src-tauri/src/bundled_models.rs` (`seed_from_app`) and the ready-path functions of the `*_models.rs` files.
- [ ] Edit the startup call in `lib.rs`.

**Build.**

- [ ] Shipped models resolve to `resource_dir()/models` and are never copied. Only downloaded or imported models live in the cache root. Copies left by earlier builds stay on disk (ADR-0012 rule 3).

**You see.**

- [ ] A fresh install's data folder has no copy of any shipped model file.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `bundled_models` tests assert the resolution order and that excluded families still refuse. Run `cargo test bundled_models editions`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `develop-ai-denoise-real-model` at trunk and head. Save `denoise.png`. Pass when both pass.
- [ ] Lane 2. Release artifact launched with a throwaway `HOME`, then `du -sk` of the data folder. Save `data-size.txt`. Pass when no shipped model file is in it.
- [ ] Lane 3. Same at head, `develop-select-subject-real-model`. Save `segment.png`. Pass when the helper loads the model from `Resources/models`.
- [ ] Lane 4. Same, `develop-remove-tool-real-model`. Save `remove.png`. Pass when it passes.
- [ ] Lane 5. A data folder seeded by a trunk build, then the head build. Save `old-copies.png`. Pass when features work and the old copies are untouched.
- [ ] Lane 6. `test-no-lama` edition at head. Save `edition.png`. Pass when Remove shows the "not included" sentence.
- [ ] Lane 7. Sandbox build, `mas`, Denoise. Save `mas-denoise.png`. Pass when the helper reads the model inside the bundle under the sandbox.
- [ ] Lane 8. Runner lane, `windows-latest`, Denoise. Save `win-denoise.png`. Pass when it passes.
- [ ] Lane 9. Debug build, import a model file through the import command. Save `import-model.png`. Pass when the import is copied to the cache root and used.
- [ ] Lane 10. Release artifact, delete one shipped model from the bundle copy. Save `missing-model.png`. Pass when the feature reports `missing` and nothing crashes.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. First-launch `first_grid_ms` with an empty data folder, and the data folder size after first launch.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` with a fresh `HOME` per run, plus `du -sk`.
- [ ] Baseline. Record the trunk first-launch median and folder size first.
- [ ] Rule. Fail if the head first-launch median is slower than trunk. Fail if the head folder is not at least 500 MB smaller.

**Review gate.** None. S7 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Harden the release bundle (S8, shared)

**Depends on.** None. No owner gate. Cost 1.5 to 2.5 engineer-days (audit M1, M2, M4 and M5, guess).

**Files.**

- [ ] Edit `app/src-tauri/tauri.conf.json` (`csp`, `bundle.category`, `publisher`, `copyright`, `shortDescription`).
- [ ] Edit `app/src-tauri/capabilities/default.json` and `app/src/lib/api/system.js`.
- [ ] Delete `app/src/routes/m0-spike/` and `app/src/routes/m1-slice3-smoke/`.

**Build.**

- [ ] A CSP that allows self, the asset protocol, IPC and HTTPS images, so custom tile servers keep working.
- [ ] Move reveal and open into Rust commands, as `open_third_party_notices` already does, and remove `opener:allow-open-path` with `**` from the capability.
- [ ] `bundle.category` is Photography. The built `Info.plist` gains `LSApplicationCategoryType`.

**You see.**

- [ ] The map, Develop and Export work with no CSP violation in the console.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Vitest cases for the new reveal and open calls. Run `npx vitest run && npm run check`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `golden-path` at trunk and head. Save `golden.png`. Pass when both pass.
- [ ] Lane 2. Debug build, open the map with default tiles. Save `map.png`. Pass when tiles draw and the console has no CSP violation.
- [ ] Lane 3. Debug build, a custom HTTPS tile server set in Settings. Save `custom-tiles.png`. Pass when tiles draw.
- [ ] Lane 4. Debug build, `develop-cpu-gpu-parity`. Save `gpu.png`. Pass when the spec passes under the CSP.
- [ ] Lane 5. Debug build, Export, then reveal the destination. Save `reveal.png`. Pass when no error line appears.
- [ ] Lane 6. `settings-about-open`. Save `about.png`. Pass when both buttons open without error.
- [ ] Lane 7. Navigate the webview to `/m0-spike`. Save `route-gone.png`. Pass when the route is not found.
- [ ] Lane 8. `plutil -p` on the release `Info.plist`. Save `plist.txt`. Pass when `LSApplicationCategoryType` is the photography category.
- [ ] Lane 9. Runner lane, `windows-latest`, lanes 2 and 5. Save `win-hardening.png`. Pass when both work.
- [ ] Lane 10. Debug build, Print from Library. Save `print.png`. Pass when the print dialog opens as on trunk.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms`, which a CSP could slow if it blocks and retries a resource.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms`.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent.

**Review gate.** None. S8 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Ask before face scans and online map use (S9, shared)

**Depends on.** S3. Owner-gated on D11 and D17. Cost 2 to 4 engineer-days (audit S4 and S5, guess). Both stores require a privacy policy, and Microsoft requires opt-in before personal data goes to a third party (sourced, policies 5.1.1, 10.5.1 and 10.5.2).

**Files.**

- [ ] Edit `app/src-tauri/src/catalog/faces.rs` (delete all face data) and the face scan entry points.
- [ ] Edit `app/src-tauri/src/geocode.rs`, `app/src/lib/mapTiles.js` and `LibraryMapView.svelte`.
- [ ] Edit the Privacy section of `SettingsDialog.svelte` and `docs/USER_GUIDE.md`.

**Build.**

- [ ] The first face scan asks once and runs only after a yes. "Delete all face data" removes every `faces` and `people` row.
- [ ] New setting `online_map_enabled`, false by default. The map shows a card to turn it on. `geocode::search` and `reverse` refuse while it is off.

**You see.**

- [ ] A fresh install opens the map with the card and makes no network request until the user turns it on.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A catalog test for delete-all and a `geocode` test for the refusal. Vitest for the card state. Run `cargo test faces geocode && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Open the map on a fresh catalog at trunk and head. Save `map-first.png`. Pass when trunk draws tiles at once and head shows the card.
- [ ] Lane 2. Head, turn the map on. Save `map-on.png`. Pass when tiles draw.
- [ ] Lane 3. Head, place search while off, through the command. Save `search-off.txt`. Pass when it returns the refusal and the request log is empty.
- [ ] Lane 4. Head, start a face scan in a default-edition build. Save `face-ask.png`. Pass when the question shows before any scan.
- [ ] Lane 5. Head, decline. Save `face-declined.txt`. Pass when `faces` has 0 rows.
- [ ] Lane 6. Head, accept, scan, then "Delete all face data". Save `face-deleted.txt`. Pass when `faces` and `people` have 0 rows.
- [ ] Lane 7. Head, quit and relaunch after turning the map on. Save `map-persist.png`. Pass when the map stays on.
- [ ] Lane 8. Sandbox build, `mas`, `/usr/bin/log show` and `lsof -i` for the app during its first minute. Save `no-network.txt`. Pass when no outbound connection appears.
- [ ] Lane 9. Runner lane, `windows-latest`, lanes 1 and 2. Save `win-map.png`. Pass when both match the Mac lanes.
- [ ] Lane 10. `store` edition without `face-group`, Settings, Privacy. Save `store-privacy.png`. Pass when face controls state that grouping is not included.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms`. The consent checks add a settings read at startup.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms`.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 1, lane 4 and lane 6 screenshots into `/tmp/store-review/S9-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S9-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Ship the EULA and privacy policy in the app (S10, shared)

**Depends on.** S5, S6 and S9. Counsel-gated on the texts. Cost 1.5 to 3 engineer-days (RFC-0038 B5.9, guess). RFC-0038 B5.9 says "first-run acceptance". The Mac store forbids licence screens at launch (sourced, guideline 2.4.5(vi)), so the Mac store build shows none.

**Files.**

- [ ] Create `legal/EULA.md` and `legal/PRIVACY.md` from counsel's texts, unchanged.
- [ ] Edit `app/src-tauri/tauri.conf.json` (`bundle.resources` and the NSIS and WiX licence file) and the About tab.

**Build.**

- [ ] About gains "Licence agreement" and "Privacy policy" buttons that open the bundled files through Rust commands.
- [ ] The Windows installer shows the EULA page. The Mac store build shows no licence screen (`launch_licence_screen` cap from S1).

**You see.**

- [ ] About opens both texts. The privacy text covers geocoding, map tiles, local face data and the absence of model downloads in store builds.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A Rust test asserts both files are bundled and match counsel's SHA-256 recorded in the test. Run `cargo test --lib bundled_models`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Open About at trunk and head. Save `about.png`. Pass when trunk has two buttons and head has four that all open without error.
- [ ] Lane 2. Sandbox build, `mas`, cold launch. Save `mas-launch.png`. Pass when the Library shows with no licence dialog.
- [ ] Lane 3. Release artifact, `ls Contents/Resources/legal`. Save `legal.txt`. Pass when both files are present with the recorded hashes.
- [ ] Lane 4. Runner lane, `windows-latest`, NSIS installer started without `/S`, screenshot of the second page. Save `win-eula-page.png`. Pass when the EULA text shows.
- [ ] Lane 5. Runner lane, `windows-latest`, `/S` install. Save `win-silent.txt`. Pass when it exits 0 with no page shown.
- [ ] Lane 6. `grep` the privacy text for geocoding, tiles, faces and downloads. Save `privacy-terms.txt`. Pass when each topic is named.
- [ ] Lane 7. Sandbox build, `mas`, About, Privacy policy. Save `mas-privacy.png`. Pass when it opens inside the sandbox.
- [ ] Lane 8. Runner lane, `windows-latest`, About, both buttons. Save `win-about.png`. Pass when both open.
- [ ] Lane 9. Debug build, default channel, cold launch. Save `direct-launch.png`. Pass when it behaves as the owner chose for direct builds.
- [ ] Lane 10. The privacy policy URL that G2 and G3 will enter, fetched with `curl -sI`. Save `privacy-url.txt`. Pass when it returns 200 and the page text equals `legal/PRIVACY.md`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` and installer size.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` and the release artifact size.
- [ ] Baseline. Record the trunk values first.
- [ ] Rule. Fail if `first_grid_ms` exceeds trunk by more than 10 percent or the installer grows by more than 1 MB.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 1, lane 2 and lane 4 screenshots into `/tmp/store-review/S10-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S10-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Build every channel from a tag in CI (S11, shared)

**Depends on.** S1. No owner gate. Cost 1.5 to 3 engineer-days (RFC-0038 B1 and B2, plus build numbers, guess). CI never builds an installer today (measured, RFC-0038 section 0).

**Files.**

- [ ] Create `.github/workflows/release.yml` as a reusable workflow with inputs `channel`, `edition` and `os`, plus a `push` trigger on `v*` tags.
- [ ] Create `.github/actions/vcpkg-libraw/action.yml` and use it from `ci.yml` and `release.yml`.
- [ ] Create `scripts/release/build-number.sh` and `docs/RELEASING.md`.

**Build.**

- [ ] Each run builds unsigned bundles for one channel and edition, asserts `licenses/` and both notice files are in the bundle, and uploads them as artifacts.
- [ ] `build-number.sh` sets `CFBundleVersion` and the Windows version from the run number, so every upload has a higher number (audit M6).
- [ ] The source bundle of RFC-0038 B2 is attached only if counsel's D4 answer needs it.

**You see.**

- [ ] `gh workflow run release.yml -f channel=mas -f edition=store` produces an `.app` artifact with the `store` edition file.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `build-number.sh --self-test`. Run `scripts/release/build-number.sh --self-test && actionlint`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no release workflow, so record that and gate that `ci.yml` stays green at head with the shared vcpkg action. Save `ci-green.txt`. Pass when every `ci.yml` job passes.
- [ ] Lane 2. `release.yml` with `channel=mas`. Save `mas-artifact.txt`. Pass when the `.app` has `Resources/models/bundled-models.edition.json` naming `store`.
- [ ] Lane 3. `release.yml` with `channel=msstore`. Save `ms-artifact.txt`. Pass when an `.msi` and an `.exe` are uploaded.
- [ ] Lane 4. Two runs in a row. Save `build-numbers.txt`. Pass when the second build number is higher.
- [ ] Lane 5. A throwaway branch that removes `licenses/` from `bundle.resources`. Save `assert-red.txt`. Pass when the bundle assertion fails the run.
- [ ] Lane 6. `msiexec /a` on the MSI in a runner lane. Save `msi-contents.txt`. Pass when `licenses/` and both notice files are present.
- [ ] Lane 7. A test tag `v0.0.0-lane` on a fork. Save `tag-release.txt`. Pass when a draft release holds both platforms' bundles.
- [ ] Lane 8. Downloaded `mas` artifact launched as a sandbox build. Save `mas-launch.png`. Pass when it shows the Library.
- [ ] Lane 9. `git rev-parse` recorded in the workflow summary against the input ref. Save `commit-map.txt`. Pass when they match.
- [ ] Lane 10. `release.yml` with `edition=test-no-lama`. Save `edition-artifact.txt`. Pass when the bundle lacks the LaMa file.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Workflow wall time per channel. The app is not changed, so `first_grid_ms` must not move.
- [ ] Probe. `gh run view --json` timings of lanes 2 and 3, and `perf-interleave.sh` on `first_grid_ms`.
- [ ] Baseline. Record the trunk `first_grid_ms` median first. Trunk has no release workflow, so wall time has an absolute budget.
- [ ] Rule. Fail if a channel build takes more than 60 minutes (guess) or `first_grid_ms` moves by more than 10 percent.

**Review gate.** None. S11 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Report folders the import cannot read (M1, Mac)

**Depends on.** None. No owner gate. Cost 0.5 to 1 engineer-day (guess). `collect_candidate_files` drops the `read_dir` error at `import.rs:53`, so a denied folder imports nothing and reports success (measured, spike). The fix helps every channel.

**Files.**

- [ ] Edit `app/src-tauri/src/import.rs` and the import result type.
- [ ] Edit the import result display in the Library frontend and `app/src/lib/appEvents.js`.

**Build.**

- [ ] `collect_candidate_files` returns the folders it could not read. The import result gains `unreadable_folders`.
- [ ] The Library shows "Emulsion could not read {folder}." for each one.

**You see.**

- [ ] Importing a folder the app cannot read shows that sentence instead of an empty import.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] An `import` test with a folder made unreadable by `chmod 000`. Run `cargo test import`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Import a `chmod 000` folder on the debug build at trunk and head. Save `unreadable.png`. Pass when trunk reports 0 imported and 0 failed, and head shows the sentence.
- [ ] Lane 2. Sandbox build, `direct`, import a folder outside the container. Save `sandbox-denied.png`. Pass when the sentence names the folder.
- [ ] Lane 3. Debug build, import `test_image/`. Save `readable.png`. Pass when 8 files import as on trunk.
- [ ] Lane 4. A readable folder with one unreadable subfolder. Save `partial.png`. Pass when the readable files import and the subfolder is named.
- [ ] Lane 5. Runner lane, `windows-latest`, a folder denied with `icacls /deny`. Save `win-denied.png`. Pass when the sentence shows.
- [ ] Lane 6. Call the command the drop handler calls, with an unreadable folder. Save `drop.png`. Pass when the sentence shows.
- [ ] Lane 7. Fix the permissions and import again. Save `retry.png`. Pass when the files import.
- [ ] Lane 8. An empty readable folder. Save `empty.png`. Pass when the message differs from the unreadable one.
- [ ] Lane 9. A folder deleted between pick and import. Save `vanished.png`. Pass when the sentence shows and nothing crashes.
- [ ] Lane 10. `import-progress` spec. Save `progress.png`. Pass when it passes unchanged.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `import_ms` for the 8 fixture files.
- [ ] Probe. `perf-interleave.sh <trunk> <head> import_ms`.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 1 and lane 2 screenshots into `/tmp/store-review/M1-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/M1-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Keep folder access across launches with security-scoped bookmarks (M2, Mac)

**Depends on.** S1 and M1. No owner gate on code. Cost 4 to 7 engineer-days (guess). The audit sized the whole bookmark layer at 6 to 10 (audit B1), and M3 takes its UI part. The sandbox denies every stored path after a relaunch (measured for paths outside the container, spike).

**Files.**

- [ ] Create `app/src-tauri/src/sandbox_access/mod.rs`, macOS code over `objc2` behind `cfg(feature = "channel-mas")`, a no-op elsewhere.
- [ ] Create `app/src-tauri/src/catalog/folder_grants.rs` and a migration in `catalog/schema.rs`.
- [ ] Edit the setup block of `lib.rs`, `import.rs` and `supervisor.rs`.

**Build.**

- [ ] Answer audit question 3 first. A ten-line spike resolves a bookmark in the app after the helper started and writes from the helper. Record the result in the PR.
- [ ] Table `folder_grants(root TEXT PRIMARY KEY, bookmark BLOB NOT NULL, stale INTEGER NOT NULL DEFAULT 0)`. One row per import root, so the number of active grants stays small.
- [ ] At setup, resolve every grant and start access before the background passes and before the helper spawns. Hold each `AccessScope` for the app's lifetime. Mark a grant `stale` when resolution fails.
- [ ] Import records a grant for each root folder it was given.

**You see.**

- [ ] In the sandbox build, photos imported in one launch show thumbnails and open in Develop after a relaunch.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `folder_grants` tests for insert, lookup of the covering root and `stale`. Run `cargo test folder_grants && cargo test --features channel-mas folder_grants`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `sandbox-smoke` at trunk and head. Trunk lacks bookmarks, so record that and gate the relaunch scenario of lane 2. Save `smoke.png`. Pass when both runs pass.
- [ ] Lane 2. Sandbox build signed with a lane-only temporary-exception entitlement for `/private/tmp/lane-photos/`, import that folder, quit, re-sign without the exception, relaunch. Save `relaunch.png`. Pass when thumbnails and a Develop preview render. If resolution fails under this method, record BLOCKED, and the review gate's human check carries the scenario.
- [ ] Lane 3. Same as lane 2, then export one image. Save `export-source.png`. Pass when the original decodes for export.
- [ ] Lane 4. Same as lane 2, run Select Subject. Save `helper.png`. Pass when the helper reads its inputs after the relaunch.
- [ ] Lane 5. Same as lane 2, then rename the folder before relaunch. Save `stale.txt`. Pass when the grant is marked `stale` and nothing crashes.
- [ ] Lane 6. Debug build, default channel, `golden-path`. Save `direct.png`. Pass when it passes and `folder_grants` stays empty.
- [ ] Lane 7. Runner lane, `windows-latest`, `golden-path`. Save `win.png`. Pass when it passes.
- [ ] Lane 8. `sqlite3` on a catalog copy after lane 2. Save `grants.txt`. Pass when one row exists for the imported root.
- [ ] Lane 9. `codesign -d --entitlements :-` on the lane app. Save `entitlements.txt`. Pass when `files.bookmarks.app-scope` is present.
- [ ] Lane 10. Twenty imported roots, relaunch. Save `many-roots.txt`. Pass when all twenty resolve.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the default channel at trunk and head. Diff-added work is bookmark resolution at launch in the `mas` build.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms`, and a timing log line around resolution in the `mas` sandbox build with twenty roots.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the default-channel median exceeds trunk by more than 10 percent. Fail if resolving twenty roots takes more than 100 ms (absolute budget, guess).

**Review gate.** The operator reviews before merge. The open-panel grant cannot be driven headlessly (measured, spike), so a human at the screen runs the real path.

- [ ] Copy lane 2 screenshots into `/tmp/store-review/M2-review-relaunch.png`.
- [ ] The operator signs a sandbox build with the Apple Development identity, imports a folder through the real open panel, quits, relaunches and opens a photo. Record that as a 30 to 60 second video saved as `/tmp/store-review/M2-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Grant whole folders at import and reconnect lost folders (M3, Mac)

**Depends on.** M2. No owner gate. Cost 2 to 3 engineer-days (guess). A per-file grant does not cover RAW plus XMP sidecars, HDR brackets or panorama frames beside the file (inferred, audit B4 and spike).

**Files.**

- [ ] Edit the Library import entry points in the frontend and `app/src/lib/appEvents.js`.
- [ ] Create a Reconnect banner component in the Library sidebar.
- [ ] Edit `catalog/folder_grants.rs` to rewrite paths when a folder is relocated.

**Build.**

- [ ] In the `mas` build, Import Files and a drop of single files ask for the folder that holds them, with the open panel starting there.
- [ ] A stale or missing root shows "Photos in {folder} cannot be reached. Reconnect." Reconnect opens the folder panel, stores a new grant and rewrites paths if the folder moved.

**You see.**

- [ ] After the folder is renamed, the banner appears, and Reconnect brings the thumbnails back.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A `folder_grants` relocate test and Vitest cases for the banner state. Run `cargo test folder_grants && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `golden-path` on the default channel at trunk and head. Save `golden.png`. Pass when both pass with no banner.
- [ ] Lane 2. Sandbox build with a stale grant from M2 lane 5. Save `banner.png`. Pass when the banner names the folder.
- [ ] Lane 3. Call the reconnect command with the new path, as the panel would. Save `reconnected.png`. Pass when thumbnails return and the banner is gone.
- [ ] Lane 4. `sqlite3` after lane 3. Save `paths.txt`. Pass when image paths point at the new folder.
- [ ] Lane 5. `hdr-merge` spec on the sandbox build with a folder grant. Save `hdr.png`. Pass when the bracket frames are read.
- [ ] Lane 6. Panorama frames `a` and `b` on the sandbox build. Save `pano.png`. Pass when both frames are read.
- [ ] Lane 7. A folder that holds no catalog photos chosen at reconnect. Save `wrong-folder.png`. Pass when the app says no matching photos and keeps the old grant.
- [ ] Lane 8. Debug build, default channel, Import Files. Save `direct-import.png`. Pass when no folder question appears.
- [ ] Lane 9. Runner lane, `windows-latest`, `golden-path`. Save `win.png`. Pass when no banner appears.
- [ ] Lane 10. Sandbox build, two roots, one stale. Save `two-roots.png`. Pass when only the stale one shows the banner.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` and `import_ms` on the default channel.
- [ ] Probe. `perf-interleave.sh` for each metric.
- [ ] Baseline. Record both trunk medians first.
- [ ] Rule. Fail if either head median exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2 and lane 3 screenshots into `/tmp/store-review/M3-review-<slug>.png`.
- [ ] The operator runs Import Files and Reconnect through the real open panel on an Apple Development signed build. Record a 30 to 60 second video saved as `/tmp/store-review/M3-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Write exports, merges, enlargements and backups outside the container (M4, Mac)

**Depends on.** M2. No owner gate. Cost 2 to 4 engineer-days (audit B4 2 to 3 plus the backup half of S3, guess). Export outside the container fails with `Operation not permitted` today (measured, spike). The custom cache location is off in the `mas` build (S1), so it needs no grant.

**Files.**

- [ ] Edit `app/src-tauri/src/export.rs` and `ExportDialog.svelte`.
- [ ] Edit the HDR and panorama output code in `lib.rs`, `upscale_commands.rs` and `upscale_job.rs`.
- [ ] Edit `app/src-tauri/src/catalog/backup.rs` and `BackupPromptDialog.svelte`.

**Build.**

- [ ] The export destination comes from the panel in the same session or from a stored folder grant for "same folder as last time".
- [ ] Merges write into the reference photo's folder through its grant, or ask with a save panel when no grant covers it.
- [ ] The helper writes Enlarge output into the container scratch folder, and the app moves it beside the original.
- [ ] The backup folder is stored as a folder grant.

**You see.**

- [ ] In the sandbox build, an export to a granted folder writes the file, and a scheduled backup still writes after a relaunch.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Tests for the destination choice and the Enlarge move. Run `cargo test export upscale backup`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Export to a folder outside the container on the sandbox build at trunk and head. Save `export-outside.png`. Pass when trunk fails with `Operation not permitted` and head writes the file.
- [ ] Lane 2. Head, relaunch, export again to "same folder as last time". Save `export-again.png`. Pass when the file is written.
- [ ] Lane 3. Head, `hdr-merge` with the result saved beside the brackets. Save `hdr-out.png`. Pass when the merged file is written there.
- [ ] Lane 4. Head, Enlarge on a granted photo. Save `enlarge-out.png`. Pass when `<stem>-Enlarged-4x.jpg` is beside the original.
- [ ] Lane 5. Head, set a backup folder, relaunch, run a backup. Save `backup.png`. Pass when the backup file exists.
- [ ] Lane 6. Head, merge on a photo with no covering grant. Save `save-panel.png`. Pass when the app asks for a destination instead of failing.
- [ ] Lane 7. Debug build, default channel, `golden-path` export. Save `direct-export.png`. Pass when it behaves as on trunk.
- [ ] Lane 8. Runner lane, `windows-latest`, export and Enlarge. Save `win-export.png`. Pass when both write beside the original as on trunk.
- [ ] Lane 9. Head, Export with "Denoise these first". Save `denoise-export.png`. Pass when both files are written.
- [ ] Lane 10. Head, `ls` of the container scratch folder after lane 4. Save `scratch.txt`. Pass when no leftover Enlarge file remains.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `export_ms` for one JPEG on the default channel. Diff-added work is the Enlarge move in the `mas` build.
- [ ] Probe. `perf-interleave.sh <trunk> <head> export_ms`, and a timing log line around the move.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent. Fail if the move takes more than 200 ms for a 4x result (absolute budget, guess).

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 1 and lane 6 screenshots into `/tmp/store-review/M4-review-<slug>.png`.
- [ ] The operator exports through the real panel on an Apple Development signed build and records a 30 to 60 second video saved as `/tmp/store-review/M4-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Open exported files in an app the user picks (M5, Mac)

**Depends on.** S1. No owner gate. Cost 1 to 2 engineer-days (audit B2, guess). The `mas` build has no export-plugin hook (S1). A user's own program is denied `process-exec` under the sandbox (measured, spike). Skip this PR if the owner wants the hook simply absent on the Mac store.

**Files.**

- [ ] Create `app/src-tauri/src/open_with.rs`, macOS only, over `NSWorkspace`.
- [ ] Edit `ExportDialog.svelte` and the export settings store.

**Build.**

- [ ] "After export, open in {app}" stores an app chosen in the open panel and opens the exported files with it through LaunchServices. No arguments and no command line.

**You see.**

- [ ] After an export, the picked app opens with the exported files.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A test of the stored setting round trip. Run `cargo test open_with && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Export with the plugin hook set to `/usr/bin/touch` on the debug build at trunk and head. Save `hook.png`. Pass when the hook still runs in the default channel.
- [ ] Lane 2. Sandbox build, `mas`, set Preview as the app, export. Save `preview-opened.png`. Pass when Preview becomes the front app with the file.
- [ ] Lane 3. Same, export three files. Save `three-files.png`. Pass when all three open.
- [ ] Lane 4. Same, after a relaunch. Save `after-relaunch.png`. Pass when the setting persists and works.
- [ ] Lane 5. The picked app deleted before export. Save `app-gone.png`. Pass when export succeeds and a sentence says the app is missing.
- [ ] Lane 6. Setting cleared. Save `cleared.png`. Pass when export opens nothing.
- [ ] Lane 7. `/usr/bin/log show` for sandbox denials during lane 2. Save `denials.txt`. Pass when none mentions the open.
- [ ] Lane 8. Runner lane, `windows-latest`, Export dialog. Save `win-dialog.png`. Pass when the hook UI is unchanged and the new row is absent.
- [ ] Lane 9. Debug build, default channel, Export dialog. Save `direct-dialog.png`. Pass when the new row is absent.
- [ ] Lane 10. Sandbox build, export to a folder outside the container, then open in Preview. Save `outside-open.png`. Pass when Preview shows the file.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `export_ms` on the default channel. Diff-added work is the time from export done to the open call in the `mas` build.
- [ ] Probe. `perf-interleave.sh <trunk> <head> export_ms` and a timing log line around the open call.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent, or the open call takes more than 500 ms (absolute budget, guess).

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2 screenshots into `/tmp/store-review/M5-review-preview.png`.
- [ ] The operator picks an app through the real open panel and records a 30 to 60 second video saved as `/tmp/store-review/M5-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Sign, package and upload the Mac store build (M6, Mac)

**Depends on.** S11 and M2. Owner-gated on D14 and D18, and on the secrets below. Cost 3 to 5 engineer-days, plus 1 to 3 for a universal build (audit B3 and S7, guess), plus account calendar time.

**Files.**

- [ ] Create `app/src-tauri/tauri.appstore.conf.json` with `bundle.macOS.entitlements`, `minimumSystemVersion`, the `embedded.provisionprofile` mapping and `ITSAppUsesNonExemptEncryption` (sourced, Tauri App Store guide).
- [ ] Create `app/src-tauri/entitlements/mas-app.entitlements` and `mas-helper.entitlements`.
- [ ] Create `scripts/release/mas-sign-and-package.sh` and `.github/workflows/release-mas.yml`.

**Build.**

- [ ] The owner creates the Apple Distribution and Mac Installer Distribution certificates, the App ID, the Mac App Store provisioning profile and an App Store Connect API key, then stores them as GitHub secrets. Only the Account Holder or an Admin can create distribution certificates (sourced). Evidence is `gh secret list` showing the names.
- [ ] Answer audit question 2 first. Run `codesign -d --entitlements :-` on `Contents/MacOS/emulsion-ai` in Tauri's output to see whether the bundler applies its own entitlements to `externalBin`. Record it in the PR.
- [ ] `mas-app.entitlements` holds app sandbox, user-selected read-write, bookmarks app-scope, network client and print, plus the application and team identifiers. `mas-helper.entitlements` holds exactly app sandbox and inherit (sourced, Apple helper-tool page).
- [ ] The script signs the helper, then the app, clears extended attributes, and runs `productbuild --sign`. The workflow runs `altool --validate-app`, and uploads only on a manual approval step.

**You see.**

- [ ] A build appears in App Store Connect and TestFlight. Evidence is the owner's screenshot.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The script's `--dry-run` prints the signing order for a fixture bundle. Run `scripts/release/mas-sign-and-package.sh --dry-run && actionlint`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no Mac store package, so record that and gate that the `release-mas.yml` artifact validates. Save `validate.txt`. Pass when `altool --validate-app` reports no errors.
- [ ] Lane 2. Expand the `.pkg` and run `codesign -d --entitlements :-` on the app. Save `app-ent.txt`. Pass when the set equals `mas-app.entitlements` and has no network server.
- [ ] Lane 3. Same on `emulsion-ai`. Save `helper-ent.txt`. Pass when the set is exactly app sandbox and inherit.
- [ ] Lane 4. `pkgutil --check-signature` on the `.pkg`. Save `pkg-sig.txt`. Pass when it names the Mac Installer Distribution identity.
- [ ] Lane 5. `xattr -lr` on the expanded app. Save `xattr.txt`. Pass when no `com.apple.quarantine` is found (sourced rule since 2025-02-18).
- [ ] Lane 6. `plutil -p` on the app's `Info.plist`. Save `plist.txt`. Pass when `ITSAppUsesNonExemptEncryption`, the category and `LSMinimumSystemVersion` match D18.
- [ ] Lane 7. `lipo -info` on both executables. Save `arch.txt`. Pass when the architectures match D18.
- [ ] Lane 8. The same bundle re-signed with the Apple Development identity on the owner's Mac, `sandbox-smoke`. Save `dev-signed.png`. Pass when the spec passes.
- [ ] Lane 9. `nm` on the packaged `emulsion`. Save `nm.txt`. Pass when no `export_plugin` symbol and no download host string are found.
- [ ] Lane 10. `ls Contents/Resources` of the packaged app. Save `resources.txt`. Pass when `licenses/`, both notice files, `legal/` once S10 lands, and the `store` edition file are present.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Packaged `.pkg` size and the cold launch `first_grid_ms` of the development-signed copy. Trunk has no package, so both are absolute budgets.
- [ ] Probe. `stat` of the `.pkg`, and `perf-probe` run five times on the lane 8 build.
- [ ] Baseline. Record the default-channel release `first_grid_ms` from trunk first.
- [ ] Rule. Fail if the `.pkg` exceeds the sum of the edition's model files plus 150 MB (guess). Fail if the sandboxed median exceeds the trunk release median by more than 15 percent.

**Review gate.** The operator reviews before merge. A distribution-signed build runs only after TestFlight installs it (inferred), so a human installs it.

- [ ] Copy lane 2 and lane 8 results into `/tmp/store-review/M6-review-<slug>.png`.
- [ ] The operator installs the TestFlight build, imports a folder through the real panel, quits, relaunches and exports. Record a 30 to 60 second video saved as `/tmp/store-review/M6-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Offer a trial through In-App Purchase on the Mac (M7, Mac)

**Depends on.** M6. Owner-gated on D12. Skip this PR when D12 is paid upfront, the recommended default. Cost 3 to 6 engineer-days (guess). Accounts needed are the In-App Purchase products and a sandbox tester in App Store Connect.

**Files.**

- [ ] Create `app/src-tauri/src/store_purchase.rs` over a StoreKit 2 bridge. `tauri-plugin-iap` needs macOS 13 (sourced, docs.rs), so D18 must allow it.
- [ ] Create the Purchase section of Settings and the trial notice.

**Build.**

- [ ] A Price Tier 0 non-consumable named "14-day Trial" and a paid unlock. The app states the duration, what stops working and the full price before the trial starts (sourced, guideline 3.1.1). A Restore Purchases button.
- [ ] Gating follows RFC-0038 B5.8. It covers AI features and export size, never the catalog or the user's data.

**You see.**

- [ ] The trial notice shows the days left, and Restore Purchases unlocks a bought copy.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Tests over a mocked `StoreBackend` for not started, active, expired, purchased and restored. Run `cargo test --features channel-mas store_purchase`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no purchase code, so record that and gate that the default channel shows no Purchase section at head. Save `direct.png`. Pass when it is absent.
- [ ] Lane 2. Sandbox build with the mock backend in state not started. Save `not-started.png`. Pass when the notice states duration, limits and price.
- [ ] Lane 3. Mock state active with 3 days left. Save `active.png`. Pass when AI features work and the notice says 3 days.
- [ ] Lane 4. Mock state expired. Save `expired.png`. Pass when AI features say the trial ended and the catalog still opens and exports.
- [ ] Lane 5. Mock state purchased. Save `purchased.png`. Pass when no notice shows.
- [ ] Lane 6. Mock restore. Save `restored.png`. Pass when purchased state follows.
- [ ] Lane 7. Mock clock moved back two days. Save `clock.png`. Pass when the days left do not grow.
- [ ] Lane 8. `nm` on the `mas` binary. Save `nm.txt`. Pass when no licence-key code from W4 is present.
- [ ] Lane 9. Runner lane, `windows-latest`. Save `win.png`. Pass when no Purchase section shows.
- [ ] Lane 10. Mock backend unreachable. Save `offline.png`. Pass when the last known state holds and nothing crashes.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the `mas` build. The entitlement check runs at launch.
- [ ] Probe. `perf-interleave.sh` on two sandbox builds, head without and with the purchase check.
- [ ] Baseline. Record the build without the check first.
- [ ] Rule. Fail if the check adds more than 50 ms to the median (guess).

**Review gate.** The operator reviews before merge. StoreKit purchases need a human signed in with the sandbox tester (inferred).

- [ ] Copy lanes 2 and 4 screenshots into `/tmp/store-review/M7-review-<slug>.png`.
- [ ] The operator buys and restores on the TestFlight build with the sandbox tester. Record a 30 to 60 second video saved as `/tmp/store-review/M7-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Keep Windows data in the local profile (W1, Windows)

**Depends on.** None. No owner gate. Cost 1 to 2 engineer-days (audit S6, guess). All state goes to `app_data_dir()`, which is the roaming profile on Windows (inferred, audit S6), and multi-GB caches live under it (measured, audit S6).

**Files.**

- [ ] Edit the data-directory helpers in `app/src-tauri/src/lib.rs` and `storage.rs` `resolve_cache_root`.

**Build.**

- [ ] On Windows, the catalog goes under `app_local_data_dir()` and caches under `app_cache_dir()`. macOS paths do not change. No migration, because no Windows build has reached a user (inferred, RFC-0038 section 3).

**You see.**

- [ ] On Windows, `catalog.sqlite` is under `%LOCALAPPDATA%`, and `%APPDATA%` holds nothing of Emulsion's.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A `storage` test for the path choice per OS. Run `cargo test storage`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Runner lane, `windows-latest`, `golden-path` at trunk and head, then `dir` of both profiles. Save `win-paths.txt`. Pass when trunk writes under `%APPDATA%` and head under `%LOCALAPPDATA%`.
- [ ] Lane 2. Runner lane, `windows-latest`, `thumbnail-backfill`. Save `win-thumbs.png`. Pass when thumbnails are under the cache folder.
- [ ] Lane 3. Runner lane, `windows-latest`, `storage-settings`. Save `win-storage.png`. Pass when the spec passes.
- [ ] Lane 4. Runner lane, `windows-latest`, quit and relaunch. Save `win-relaunch.png`. Pass when the catalog reopens.
- [ ] Lane 5. Runner lane, `windows-latest`, `develop-ai-denoise`. Save `win-denoise.png`. Pass when the helper works with the new paths.
- [ ] Lane 6. Debug build on macOS, `golden-path`. Save `mac.png`. Pass when the Mac data folder is unchanged.
- [ ] Lane 7. Sandbox build, `direct`. Save `sandbox.png`. Pass when the container path is unchanged.
- [ ] Lane 8. Runner lane, `windows-latest`, a backup. Save `win-backup.png`. Pass when it reads the catalog from the new place.
- [ ] Lane 9. Runner lane, `windows-latest`, a path with non-ASCII characters in the user name. Save `win-unicode.txt`. Pass when the catalog opens (audit question 7 is about photo paths, this is the profile path).
- [ ] Lane 10. Runner lane, `windows-latest`, `Get-ChildItem $env:APPDATA`. Save `win-roaming.txt`. Pass when no Emulsion folder exists.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` on `windows-latest`.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` in a Windows runner lane.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent.

**Review gate.** None. W1 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Build and stage the Windows helper and models (W2, Windows)

**Depends on.** S11 and W1. No owner gate. Cost 1 to 2 engineer-days (audit B6, guess). Only an `aarch64-apple-darwin` helper exists in `app/src-tauri/binaries/` (measured, audit B6).

**Files.**

- [ ] Edit `app/scripts/prepare-ai-helper.mjs` for `x86_64-pc-windows-msvc`.
- [ ] Edit the Windows job of `.github/workflows/release.yml` and `tauri.bundle-helper.conf.json`.

**Build.**

- [ ] Answer audit question 4 first. List `target/release` after `prepare-ai-helper.mjs` on the Windows runner to see whether `onnxruntime.dll` sits beside the helper. Bundle it if so.
- [ ] The Windows release bundle carries `emulsion-ai.exe` and every model of the edition.

**You see.**

- [ ] The installed Windows app runs Select Subject through the helper with no download.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `prepare-ai-helper.mjs` gains a target test. Run `npx vitest run scripts`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk's Windows bundle has no helper, so record that and gate lane 2. Save `trunk-bundle.txt`. Pass when the listing is recorded.
- [ ] Lane 2. `msiexec /a` on the head MSI. Save `msi-list.txt`. Pass when `emulsion-ai.exe` and the model files are present.
- [ ] Lane 3. Runner lane, `windows-latest`, `develop-select-subject-real-model`. Save `win-segment.png`. Pass when the spec passes.
- [ ] Lane 4. Same, `develop-ai-denoise-real-model`. Save `win-denoise.png`. Pass when it passes.
- [ ] Lane 5. Same, `develop-remove-tool-real-model`. Save `win-remove.png`. Pass when it passes or shows the edition sentence.
- [ ] Lane 6. Installed app, Task Manager list through `Get-Process`. Save `win-proc.txt`. Pass when `emulsion-ai` runs under the app with no console window.
- [ ] Lane 7. `dumpbin /dependents` on `emulsion-ai.exe`. Save `deps.txt`. Pass when every DLL it needs is in the bundle or in Windows.
- [ ] Lane 8. Release artifact for macOS. Save `mac-bundle.txt`. Pass when the Mac helper is unchanged.
- [ ] Lane 9. Runner lane with networking blocked by a firewall rule, Select Subject. Save `win-offline.png`. Pass when it still works.
- [ ] Lane 10. Edition `test-no-lama` on Windows. Save `win-edition.txt`. Pass when the LaMa file is absent.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Helper start to first segment on `windows-latest`. Trunk has no Windows helper in the bundle, so this is an absolute budget. Installer size.
- [ ] Probe. A `perf-probe` case for segment, five runs in a Windows runner lane.
- [ ] Baseline. Record the macOS trunk helper time first as a reference.
- [ ] Rule. Fail if the Windows median exceeds 3 times the macOS trunk median (guess), or the installer exceeds the models plus 250 MB.

**Review gate.** None. W2 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Sign an offline Windows installer that installs silently (W3, Windows)

**Depends on.** W2. Owner-gated on D14 and D19, and on the signing secret. Cost 2 to 4 engineer-days (audit B6, guess), plus certificate calendar time. The installer must be a signed standalone offline `.msi` or `.exe` that installs silently (sourced, Store Policy 10.2.9).

**Files.**

- [ ] Create `app/src-tauri/tauri.microsoftstore.conf.json` with `webviewInstallMode` `offlineInstaller`, a `publisher` that differs from the product name, `signCommand` and the targets (sourced, Tauri Microsoft Store guide).
- [ ] Create `.github/workflows/release-msstore.yml`.

**Build.**

- [ ] The owner obtains the certificate or the Azure Artifact Signing account per D19 and stores the secret. Evidence is `gh secret list`.
- [ ] The workflow signs the installer and every PE file inside it, since every PE file must chain to the Microsoft Trusted Root Program (sourced, research table 2.1).

**You see.**

- [ ] `msiexec /i Emulsion.msi /quiet` installs a signed app that starts.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A config test asserts the merged config has the offline WebView2 mode and a publisher unlike the product name. Run `npx vitest run scripts && actionlint`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no store installer, so record that and gate lane 2. Save `trunk.txt`. Pass when the absence is recorded.
- [ ] Lane 2. Runner lane, `msiexec /i /quiet`. Save `install.txt`. Pass when the exit code is 0 and the app folder exists.
- [ ] Lane 3. `Get-AuthenticodeSignature` on the installer, `emulsion.exe` and `emulsion-ai.exe`. Save `signatures.txt`. Pass when all three are `Valid`.
- [ ] Lane 4. Start the installed app and capture the screen. Save `launched.png`. Pass when the Library window shows.
- [ ] Lane 5. NSIS `/S`. Save `nsis.txt`. Pass when the exit code is 0.
- [ ] Lane 6. `msiexec /x /quiet`. Save `uninstall.txt`. Pass when the program folder is gone (Store Policy 10.2.7, sourced).
- [ ] Lane 7. Install, then install the next build over it. Save `upgrade.txt`. Pass when one copy remains and the catalog opens.
- [ ] Lane 8. Install with networking blocked. Save `offline-install.txt`. Pass when it succeeds.
- [ ] Lane 9. Installer size against the model files. Save `size.txt`. Pass when the installer holds the models, so it is not a downloader stub.
- [ ] Lane 10. Remove WebView2 from a runner if the image allows it, then install. Save `webview2.txt`. Pass when the installed app starts. If the runner cannot remove it, record that.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Silent install time and installer size. Trunk has no installer, so both are absolute budgets. Store Policy 10.4.4 asks for a reasonable download time (sourced).
- [ ] Probe. `Measure-Command` around `msiexec /i /quiet`, five runs on fresh runners.
- [ ] Baseline. Record W2's unsigned MSI install time first.
- [ ] Rule. Fail if the median install exceeds 3 minutes (guess) or signing adds more than 20 percent.

**Review gate.** None. W3 is not review-gated. It changes no interaction, and every lane runs headless.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Add offline licence keys for the Windows build (W4, Windows)

**Depends on.** S1. Owner-gated on D9, D10 and D13 route A. Cost 3.5 to 7 engineer-days (RFC-0038 B5.8 3 to 6, plus 0.5 to 1 for its own RFC, guess). The Mac store build never carries this code (sourced, guidelines 2.4.5(vi) and 3.1.1).

**Files.**

- [ ] Create `docs/rfc/RFC-0040-licence-keys.md` first, then `app/src-tauri/src/licensing.rs`.
- [ ] Create the Licence tab in `SettingsDialog.svelte`.

**Build.**

- [ ] Offline Ed25519-signed licence files with a trial and a grace period. Gating covers AI features and export size, never the catalog or the user's data (RFC-0038 B5.8).
- [ ] Compiled only when `own_licence` is on, so never in the `mas` build.

**You see.**

- [ ] The Licence tab shows the trial days left, and importing a valid licence file unlocks the app.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Tests for tampering, clock rollback, expiry and a wrong key. Run `cargo test licensing`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no licensing, so record that and gate that `golden-path` passes at head on the `msstore` build in trial. Save `trial-golden.png`. Pass when it passes.
- [ ] Lane 2. Runner lane, `windows-latest`, `msstore`, fresh install, Licence tab. Save `trial.png`. Pass when the trial days show.
- [ ] Lane 3. Import a valid test licence file. Save `licensed.png`. Pass when the tab says licensed.
- [ ] Lane 4. Import a file with one changed byte. Save `tampered.png`. Pass when it is refused.
- [ ] Lane 5. Expired trial through a test clock. Save `expired.png`. Pass when AI features say so and the catalog still opens and exports.
- [ ] Lane 6. Clock moved back. Save `rollback.png`. Pass when the trial does not extend.
- [ ] Lane 7. `nm` on the `mas` binary. Save `mas-nm.txt`. Pass when no `licensing` symbol is found.
- [ ] Lane 8. Sandbox build, `mas`, Settings. Save `mas-settings.png`. Pass when no Licence tab shows.
- [ ] Lane 9. Licensed, relaunch offline. Save `offline.png`. Pass when it stays licensed.
- [ ] Lane 10. Debug build, default channel, Settings. Save `direct.png`. Pass when the tab matches the owner's choice for direct builds.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` on `windows-latest`. The licence check runs at launch.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` in a Windows runner lane.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 50 ms (guess).

**Review gate.** The operator reviews before merge.

- [ ] Copy lanes 2, 3 and 5 screenshots into `/tmp/store-review/W4-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/W4-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Take payment through a merchant of record (W5, Windows)

**Depends on.** W4. Owner-gated on D10 and D14. Cost 2 to 4 engineer-days (RFC-0038 B5.7, guess). Third-party commerce must name the provider, confirm the transaction and be declared in Partner Center (sourced, Store Policy 10.8.2).

**Files.**

- [ ] Create `services/licence-issuer/`, a small webhook service that turns a paid order into a signed licence file.
- [ ] Edit the Licence tab with a Buy button that opens the checkout page in the browser.

**Build.**

- [ ] The owner opens the merchant-of-record account and stores the webhook secret and the signing key. Evidence is `gh secret list` and a screenshot of the product page.
- [ ] Buy opens the checkout. The webhook issues the licence file by email. The app imports it as in W4.

**You see.**

- [ ] A test-mode purchase delivers a licence file that unlocks the app.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Webhook tests for a valid signature, a forged signature and a replay. Run `npm test --prefix services/licence-issuer`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no Buy button, so record that and gate lane 2. Save `trunk-tab.png`. Pass when the absence is recorded.
- [ ] Lane 2. Runner lane, `windows-latest`, click Buy. Save `buy.png`. Pass when the default browser opens the provider's checkout URL.
- [ ] Lane 3. A provider test-mode webhook sent to the issuer. Save `issued.txt`. Pass when a licence file is produced.
- [ ] Lane 4. Import that file into the app. Save `unlocked.png`. Pass when the tab says licensed.
- [ ] Lane 5. A forged webhook. Save `forged.txt`. Pass when the issuer refuses it.
- [ ] Lane 6. The same webhook sent twice. Save `replay.txt`. Pass when one licence is issued.
- [ ] Lane 7. A refund webhook. Save `refund.txt`. Pass when the issuer records it as the owner's policy says.
- [ ] Lane 8. Sandbox build, `mas`. Save `mas.png`. Pass when no Buy button exists.
- [ ] Lane 9. Issuer down, click Buy. Save `issuer-down.png`. Pass when the app still opens the checkout and says the licence arrives by email.
- [ ] Lane 10. The checkout page names the provider. Save `provider-named.png`. Pass when the name is visible (Store Policy 10.8.2).

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Webhook to licence file latency. Trunk has no issuer, so this is an absolute budget. `first_grid_ms` of the app.
- [ ] Probe. Twenty test-mode webhooks timed, and `perf-interleave.sh` on the app.
- [ ] Baseline. Record the trunk `first_grid_ms` median first.
- [ ] Rule. Fail if the 95th percentile latency exceeds 10 s (guess) or `first_grid_ms` moves by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy lanes 2 and 4 screenshots into `/tmp/store-review/W5-review-<slug>.png`.
- [ ] The operator makes one test-mode purchase end to end and records a 30 to 60 second video saved as `/tmp/store-review/W5-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Add a signed updater and a public release host for Windows (W6, Windows)

**Depends on.** W3 and W4. Owner-gated on D8. Cost 3 to 6 engineer-days (RFC-0038 B5.10 2 to 4 plus B5.11 1 to 2, guess). An MSI or EXE listing leaves updates to the app or the installer (sourced, research table 2.1), and the Store needs a versioned HTTPS installer URL that does not change (sourced, Store Policy 10.2.9).

**Files.**

- [ ] Edit `app/src-tauri/Cargo.toml` (`tauri-plugin-updater`, compiled only when `self_update` is on) and `tauri.conf.json` (`plugins.updater` public key).
- [ ] Edit `.github/workflows/release-msstore.yml` to publish the installer at a versioned URL and write `latest.json`.
- [ ] Edit Settings with an opt-in update check.

**Build.**

- [ ] The owner chooses the host and stores the updater private key as a secret. Losing the key ends updates (sourced, RFC-0038 B5.10). Evidence is `gh secret list` and the host's URL.
- [ ] The check runs only after the user opts in, and installs only a signed update.

**You see.**

- [ ] An older build offers the newer one and installs it after the user agrees.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A test that `latest.json` is generated with a valid signature. Run `cargo test updater && actionlint`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no updater, so record that and gate lane 2. Save `trunk.txt`. Pass when the absence is recorded.
- [ ] Lane 2. Runner lane, install build N, publish N+1 to a test host, opt in, check. Save `offer.png`. Pass when N+1 is offered.
- [ ] Lane 3. Accept. Save `updated.txt`. Pass when the installed version is N+1 and the catalog opens.
- [ ] Lane 4. Not opted in. Save `no-check.txt`. Pass when no request reaches the host.
- [ ] Lane 5. A `latest.json` with a bad signature. Save `bad-sig.png`. Pass when the update is refused.
- [ ] Lane 6. `curl -sI` on the versioned installer URL. Save `url.txt`. Pass when it returns 200 over HTTPS.
- [ ] Lane 7. Publish N+2 and request N+1's URL again. Save `url-stable.txt`. Pass when N+1's URL still serves N+1.
- [ ] Lane 8. `nm` on the `mas` binary. Save `mas-nm.txt`. Pass when no updater symbol is found (guideline 2.4.5(vii), sourced).
- [ ] Lane 9. Host unreachable. Save `host-down.png`. Pass when the check fails quietly and the app keeps working.
- [ ] Lane 10. Update during an open Develop session. Save `busy.png`. Pass when the app asks to restart and loses no edit.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` on `windows-latest`, opted out and opted in.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` in a Windows runner lane.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if either head median exceeds trunk by more than 10 percent. The check must run after the first grid.

**Review gate.** The operator reviews before merge.

- [ ] Copy lanes 2 and 3 screenshots into `/tmp/store-review/W6-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/W6-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Gate every store build before upload (G1, release)

**Depends on.** S2, S3, S4, S10, M6 and W3, because the store edition must be final. Counsel-gated on a recorded sign-off of the D1 to D11 answers. Cost 1 to 2 engineer-days (guess). It extends RFC-0038 Phase C with store rules.

**Files.**

- [ ] Create `scripts/release/check_store_bundle.py` and its fixtures.
- [ ] Edit `docs/RELEASING.md`, `release-mas.yml` and `release-msstore.yml` to run the check before upload.

**Build.**

- [ ] The check reads a `.pkg` or an installer and fails on any of these. Entitlements differ from the committed files, or include network server. The `mas` binary has an `export_plugin`, licensing or updater symbol. The edition file includes a family ADR-0011 rejects. `licenses/`, a notice file, `legal/EULA.md` or `legal/PRIVACY.md` is missing. A quarantine attribute is present. A signature is invalid. The build number did not rise.

**You see.**

- [ ] The upload step runs only after the check prints `store bundle OK`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A self-test with one fixture per failure. Run `python3 scripts/release/check_store_bundle.py --self-test`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no store check, so record that and gate lane 2. Save `trunk.txt`. Pass when the absence is recorded.
- [ ] Lane 2. The check on M6's real `.pkg`. Save `mas-ok.txt`. Pass when it prints `store bundle OK`.
- [ ] Lane 3. The check on W3's real MSI. Save `ms-ok.txt`. Pass when it prints `store bundle OK`.
- [ ] Lane 4. The `.pkg` re-signed with network server added. Save `net-server.txt`. Pass when the check fails and names it.
- [ ] Lane 5. A bundle copy with `licenses/` removed. Save `no-licenses.txt`. Pass when the check fails.
- [ ] Lane 6. A `default` edition file in a `mas` bundle. Save `wrong-edition.txt`. Pass when the check fails.
- [ ] Lane 7. A quarantine attribute set on one file. Save `quarantine.txt`. Pass when the check fails.
- [ ] Lane 8. An installer with one unsigned PE file. Save `unsigned.txt`. Pass when the check fails.
- [ ] Lane 9. `release-mas.yml` with a failing check. Save `upload-blocked.txt`. Pass when the upload step does not run.
- [ ] Lane 10. `docs/RELEASING.md` steps followed once by a fresh agent from a tag. Save `releasing.txt`. Pass when every step runs as written.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Check run time on the real `.pkg`. Trunk has no check, so this is an absolute budget. No app hot path changes.
- [ ] Probe. `time python3 scripts/release/check_store_bundle.py <pkg>`, five runs.
- [ ] Baseline. Record the `.pkg` expansion time alone first.
- [ ] Rule. Fail if the median exceeds 2 minutes (guess).

**Review gate.** None. G1 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Submit to the Mac App Store (G2, release)

**Depends on.** G1, plus M7 when D12 is the trial. Owner-gated on D12, D14 and D15. Cost 1 to 2 engineer-days of agent work (guess), plus App Review time, which Apple does not publish (sourced, research section 6).

**Files.**

- [ ] Create `store/mas/listing.md` (name, subtitle, description, keywords, support URL, review notes).
- [ ] Create `store/mas/privacy-answers.md`, `store/mas/age-rating-answers.md` and `store/mas/export-compliance.md`.
- [ ] Create `app/e2e/specs/store-screenshots.e2e.js`.

**Build.**

- [ ] The review notes explain the folder grant, the bundled helper and the bundled models.
- [ ] Privacy answers follow Apple's rule that data processed only on device is not collected (sourced). Map and search answers follow D17 and the provider's retention terms.
- [ ] The owner signs the Paid Apps Agreement, the tax form and the bank details, enrols in the Small Business Program (15 percent, sourced), sets the price, enters the texts and submits. Evidence is one screenshot per form.

**You see.**

- [ ] App Store Connect shows the build "Waiting for Review". Evidence is the owner's screenshot.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A lint that the listing has no forbidden phrase ("license key", "activate", "update from our site"). Run `node scripts/release/lint-listing.mjs store/mas/listing.md`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no listing, so record that and gate lane 2. Save `trunk.txt`. Pass when the absence is recorded.
- [ ] Lane 2. `store-screenshots` on the `mas` sandbox build. Save `library.png`. Pass when the Library screenshot is at the size App Store Connect asks for at upload time.
- [ ] Lane 3. Same for Develop. Save `develop.png`. Pass when the size matches.
- [ ] Lane 4. Same for Export. Save `export.png`. Pass when the size matches.
- [ ] Lane 5. Same for the map with the online map on. Save `map.png`. Pass when tiles and attribution show.
- [ ] Lane 6. Same for Cull with the eye check. Save `cull.png`. Pass when the result shows.
- [ ] Lane 7. Each screenshot checked against the `store` edition. Save `edition-check.txt`. Pass when no excluded feature appears.
- [ ] Lane 8. `curl -sI` on the support URL and the privacy URL. Save `urls.txt`. Pass when both return 200.
- [ ] Lane 9. The listing lint on the final text. Save `lint.txt`. Pass when it exits 0.
- [ ] Lane 10. The review notes read against the M6 build's behaviour by a fresh agent. Save `notes-check.txt`. Pass when every statement matches a lane result.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the submitted build, the number a reviewer meets first. No code changes here.
- [ ] Probe. `perf-probe` five times on the development-signed copy of the submitted build.
- [ ] Baseline. Record M6's lane 8 median first.
- [ ] Rule. Fail if the submitted build's median exceeds M6's by more than 10 percent.

**Review gate.** The operator reviews before merge and submits.

- [ ] Copy lanes 2 to 6 screenshots into `/tmp/store-review/G2-review-<slug>.png`.
- [ ] The operator records a 30 to 60 second video of the TestFlight build doing the review notes' steps. Save it as `/tmp/store-review/G2-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Submit to the Microsoft Store (G3, release)

**Depends on.** G1, W5 and W6. Owner-gated on D13, D14 and D15. Cost 0.5 to 1 engineer-day of agent work (guess), plus certification time.

**Files.**

- [ ] Create `store/msstore/listing.md`, `store/msstore/iarc-answers.md` and `store/msstore/commerce-declaration.md`.
- [ ] Edit `app/e2e/specs/store-screenshots.e2e.js` for Windows sizes.

**Build.**

- [ ] The description names any dependency outside the app at its start, as policy 10.2.4 asks (sourced).
- [ ] The owner enters the listing, the IARC questionnaire, the privacy URL, the versioned installer URL, the silent install switch and the pricing D13 chose, declares the third-party commerce, and submits. Evidence is one screenshot per form.

**You see.**

- [ ] Partner Center shows the submission in certification. Evidence is the owner's screenshot.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The listing lint on the Windows text. Run `node scripts/release/lint-listing.mjs store/msstore/listing.md`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no listing, so record that and gate lane 2. Save `trunk.txt`. Pass when the absence is recorded.
- [ ] Lane 2. `store-screenshots` in a Windows runner lane, Library. Save `win-library.png`. Pass when the size matches the Partner Center request.
- [ ] Lane 3. Same for Develop. Save `win-develop.png`. Pass when the size matches.
- [ ] Lane 4. Same for Export. Save `win-export.png`. Pass when the size matches.
- [ ] Lane 5. `curl -sI` on the versioned installer URL entered in the form. Save `installer-url.txt`. Pass when it returns 200 and the SHA-256 equals G1's checked installer.
- [ ] Lane 6. Install from that URL with the switch entered in the form. Save `form-switch.txt`. Pass when it installs silently.
- [ ] Lane 7. `curl -sI` on the privacy URL. Save `privacy.txt`. Pass when it returns 200.
- [ ] Lane 8. Buy flow from W5 on the downloaded installer. Save `buy.png`. Pass when the checkout opens.
- [ ] Lane 9. The listing lint. Save `lint.txt`. Pass when it exits 0.
- [ ] Lane 10. Uninstall the downloaded installer. Save `uninstall.txt`. Pass when the program folder is gone.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Download plus silent install time from the real URL (Store Policy 10.4.4, sourced).
- [ ] Probe. `Measure-Command` around download and install, five fresh runners.
- [ ] Baseline. Record W3's install median first.
- [ ] Rule. Fail if the install part exceeds W3's median by more than 10 percent, or the download takes more than 10 minutes on a runner (guess).

**Review gate.** The operator reviews before merge and submits.

- [ ] Copy lanes 2 to 4 screenshots into `/tmp/store-review/G3-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of install, launch and Buy on a Windows lane. Save it as `/tmp/store-review/G3-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator lands it bottom-up.

## Close the program

- [ ] Every box above is checked with its evidence.
- [ ] Reply to the operator with the report the execution playbook names. List each PR with its verdict, the store review outcomes, and the decisions still open.

## Appendix A. Prototype evidence

The owner's session ran one prototype on 2026-11-17. Branch `spike/mas-sandbox`, commit `b8cd7c9`, local only in worktree `.claude/worktrees/agent-a65c05f5c7d6da5bb`, never pushed. It adds only a probe spec and a WebDriver config. No app code changed (measured). The report, the raw probe output and the three entitlements files are copied unchanged into `docs/store-release-plan-appendix/`.

What it proved, on macOS 26.5.2 arm64 with an Apple Development identity (measured).

- The app starts sandboxed and keeps its catalog across a relaunch.
- `app_data_dir()` moves into `~/Library/Containers/dev.alfredwei.emulsion/` with no code change.
- WebGPU works inside the sandbox with no extra entitlement.
- `emulsion-ai`, signed with app sandbox and inherit only, spawns and loads the SAM 2 models.
- Map tiles and place search work with the network client entitlement.
- Bundled models report `ready` from `Contents/Resources/models`.
- Import from outside the container fails, an import of an outside folder silently returns zero, and export outside the container fails with `Operation not permitted`.
- A user's own program started by the export hook is denied `process-exec`. A system binary still runs.
- File-access denials outside the container leave no log line, so a missing log line is not evidence of access.

What stays unproven.

- The open-panel grant, the save panel and their persistence after relaunch. The panel could not be driven headlessly (measured). M2, M3 and M4 carry these as human review-gate boxes.
- Whether a bookmark minted under a temporary-exception entitlement resolves after re-signing without it. M2 lane 2 depends on it (guess).
- Whether ad hoc signing gives the same sandbox behaviour on a GitHub macOS runner. S0 lane 2 tests it (guess).
- Whether the helper sees grants the app starts after the helper spawned (audit question 3). M2's first box answers it.
- Whether Tauri's bundler applies separate entitlements to `externalBin` (audit question 2). M6's first box answers it.
- Distribution signing, `altool` validation, TestFlight, hardened runtime, print, HDR and panorama merges, and the D-FINE, LaMa and face models under the sandbox.
- Everything on Windows. No Windows prototype ran. Silent install, the Windows helper's DLLs (audit question 4) and the Store's payment route for MSI listings are unproven.

## Appendix B. Alternatives rejected

- **Windows route B, MSIX with Microsoft's commerce.** Tauri builds only EXE and MSI (sourced, Tauri Microsoft Store guide). Microsoft's `winapp` CLI can wrap a Tauri app as MSIX, but its guide stages only the `.exe`, so the helper, models and resources would be staged by hand, and the CLI was in preview per a search summary (sourced, research 2.2). MSIX AppData is redirected into the package and may be deleted on uninstall, taking the catalog with it (guess, audit S6). Route B wins only if Microsoft commerce is required, which Store Policy 10.8.1 says it is not for non-game apps (sourced).
- **One build for all channels.** The Mac store build must not carry licence keys, its own updater or code that runs user programs (sourced, guidelines 2.4.5 and 2.5.2), while the Windows route A build needs licence keys and an updater. One binary cannot satisfy both (sourced, research 3.1). S1's channel flag keeps one codebase with three builds.
- **Selling the Mac version outside the store with Developer ID only.** It skips the sandbox work (M2 to M5, about 9 to 19 days, guess) and keeps the plugin hook. The owner asked for the Mac App Store, so it is not the Mac channel here. It stays possible later as a direct channel through RFC-0038 B5.6, and S1 already makes a `direct` build.
- **Keeping the export-plugin hook in the Mac store build.** A user's own program is denied `process-exec` (measured, spike), and guideline 2.5.2 forbids executing code that changes features (sourced). M5 replaces it with opening exports in a picked app.
- **Bookmarking the custom cache folder on the Mac store build.** The asset protocol scope is fixed to `$APPDATA`, so a moved cache likely fails to serve previews already (inferred, audit S3). The `mas` build turns the feature off instead.
- **Moving existing catalogs into the container.** No installer has gone to anyone outside the owner (inferred, RFC-0038 section 3 says not to ship before Phase B). The owner can restore a backup. Revisit if testers received direct builds.
- **Per-file bookmarks.** They miss sidecars, brackets and panorama frames beside the file (inferred, audit B4) and multiply the number of active grants. M2 uses one grant per import root.
- **The Pictures folder entitlement alone.** It covers only `~/Pictures` (guess, unconfirmed key per research section 6). Photographers keep libraries on external drives (guess).

## Appendix C. Risks

- **S1.** The store edition may leave out features the listing promises. G2 lane 7 checks screenshots against the edition.
- **S2, S3, S4.** Counsel may take months, and the retrain path adds up to 72 days and GPU cost (guess, RFC-0038 B5.5). A new detector or inpainter may look worse. The review gates show the owner side-by-side results.
- **S6.** RFC-0039's matcher parity may take longer than 6 days (sourced, RFC-0039 section 7). D5 may void steps 2 and 4.
- **S8.** A CSP can break a custom tile server or a WebGPU worker. Lanes 2 to 4 cover both.
- **S9.** OpenStreetMap may withdraw access (sourced, `map_endpoints.rs` comment and RFC-0038 B5.12). Opt-in lowers load but does not remove the risk.
- **M2 to M4.** Bookmarks touch import, startup, export, merges, Enlarge and backup, so they are invasive. A grant that fails to resolve hides a whole folder. M3's reconnect flow is the recovery. The real panel path depends on a human at the screen every time.
- **M2.** The sandbox may limit how many security-scoped resources one process holds at once (guess). Folder-level grants keep the count low, and lane 10 tests twenty.
- **M6.** Two Mac builds (direct and store) double signing and test cost. App Review may reject the build for the helper, the network use or the face data. The review notes and the privacy answers are the mitigation. Rejection reasons cannot be predicted from the guidelines alone (guess).
- **M6, G2.** The name may clash with the earlier Emulsion app or a mark (D15). A rename after M6 changes listing assets only, because the bundle ID stays `dev.alfredwei.emulsion`.
- **W3.** Certificate cost and SmartScreen reputation are unknown. EV certificates no longer get instant reputation (sourced, RFC-0038 B5.6).
- **W4, W5.** The Windows payment route is unconfirmed. The support ticket in Arm the program settles it. If Microsoft collects payment for Paid MSI listings, W4 and W5 change.
- **Every Windows lane.** No control skill exists for the Windows app. Lanes drive it through `lanes.yml` with WebDriver specs and PowerShell, and screen captures may fail on a runner without a desktop session (guess).
- **Every review gate.** Video capture on a runner with `screencapture` is untested (guess). The owner's Mac is the fallback.

Store questions for counsel, added to RFC-0038 section 4 and RFC-0039 section 9.

- Does Apple's standard licence agreement suffice, or does the paid build need its own EULA in App Store Connect?
- Can LGPL or CDDL notices and source offers be met inside a signed, sandboxed store package the user cannot re-sign? S5 and S6 aim to make this moot.
- Do the privacy policy and the face consent meet the biometric rules of each target market (D11)?

## Appendix D. Links and reading list

Read before editing. RFC-0038 sections 1, 3 (B5) and 4, its appendix `model-training-data.md`, RFC-0039 sections 2, 7, 9 and 10, ADR-0012, the evidence in `docs/store-release-plan-appendix/`, and `.claude/skills/verify-emulsion/SKILL.md`.

Run the pstack `how` skill before M2 and S1, because each crosses many call sites. Run the pstack `interrogate` skill on M2's design and on W4's RFC-0040 before building. Keep the decision trail per the pstack `show-me-your-work` skill in the root agent's store, and commit it with G1 because a store submission needs an auditable record.

Every source below comes from `store-research.md`. Each was retrieved on 2026-11-17. The page date follows each link where the page showed one.

Apple.

- Tauri App Store guide, https://v2.tauri.app/distribute/app-store/, last updated 2026-08-20.
- App Sandbox, https://developer.apple.com/tutorials/data/documentation/security/protecting-user-data-with-app-sandbox.md, no date shown.
- Embedding a command-line tool in a sandboxed app, https://developer.apple.com/tutorials/data/documentation/xcode/embedding-a-helper-tool-in-a-sandboxed-app.md, no date shown.
- App Review Guidelines, https://developer.apple.com/app-store/review/guidelines/, last updated 2026-10-16.
- Certificates overview, https://developer.apple.com/help/account/certificates/certificates-overview/, no date shown.
- Upcoming requirements, https://developer.apple.com/news/upcoming-requirements/, entries dated as cited.
- TestFlight overview, https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/, no date shown.
- Submitting, https://developer.apple.com/app-store/submitting/, no date shown.
- Maximum build file sizes, https://developer.apple.com/help/app-store-connect/reference/maximum-build-file-sizes/, no date shown.
- Enrolment, https://developer.apple.com/programs/enroll/, no date shown.
- D-U-N-S, https://developer.apple.com/help/account/membership/D-U-N-S/, no date shown, via a search summary.
- `tauri-plugin-iap`, https://docs.rs/crate/tauri-plugin-iap/0.8.0, third party.
- `tauri-plugin-apple-iap`, https://docs.rs/crate/tauri-plugin-apple-iap/latest, third party.
- What's included, https://developer.apple.com/programs/whats-included/, no date shown, via a search summary.
- Small Business Program, https://developer.apple.com/app-store/small-business-program/, no date shown.
- Agreements, https://developer.apple.com/help/app-store-connect/manage-agreements/sign-and-update-agreements/, via a search summary.
- Banking, https://developer.apple.com/help/app-store-connect/manage-banking-information/enter-banking-information/, via a search summary.
- Asset pack limits, https://developer.apple.com/help/app-store-connect/reference/app-uploads/apple-hosted-asset-pack-size-limits, no date shown.
- App privacy details, https://developer.apple.com/app-store/app-privacy-details/, no date shown.
- Export compliance, https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance/, no date shown.
- Add a new app, https://developer.apple.com/help/app-store-connect/create-an-app-record/add-a-new-app/, no date shown.
- App name notices, https://www.apple.com/legal/internet-services/itunes/appnamenotices/.

Microsoft.

- Choose a distribution path, https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/choose-distribution-path, 2026-10-07.
- Store Policies, https://learn.microsoft.com/en-us/windows/apps/publish/store-policies, version 7.20, 2026-09-14.
- MSI and EXE package requirements, https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements, updated 2026-08-24.
- MSIX package requirements, https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements, updated 2026-08-24.
- Tauri Microsoft Store guide, https://v2.tauri.app/distribute/microsoft-store/, last updated 2026-06-15.
- winapp CLI with Tauri, https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri, 2026-10-07.
- Open a developer account, https://learn.microsoft.com/en-us/windows/apps/publish/partner-center/open-a-developer-account, 2026-10-29.
- Store developer sign-up, https://storedeveloper.microsoft.com.
- App Developer Agreement v8.11, https://go.microsoft.com/fwlink/?linkid=528905, published 2026-03-17, effective 2026-04-17.
- MSI price and availability, https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/price-and-availability, 2026-08-03.
- MSI set app pricing, https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/set-app-pricing, 2022-10-30, updated 2024-07-23.
- Microsoft Q&A on MSI trials, https://learn.microsoft.com/en-us/answers/questions/5656944/, unanswered and unofficial.
- Implement a trial version, https://learn.microsoft.com/en-us/windows/uwp/monetize/implement-a-trial-version-of-your-app, 2017-08-25, updated 2025-06-10.
- WebView2 distribution, https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution, 2024-06-27, updated 2026-10-02.
- Age ratings, https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/age-ratings, 2026-08-21.
- MSI app submission, https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/create-app-submission, updated 2026-08-24.
- Reserve your app's name, https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/reserve-your-apps-name, updated 2026-08-24.

Name and trademark.

- USPTO clearance search guidance, https://www.uspto.gov/trademarks/search/comprehensive-clearance-search-similar-trademarks, via a search summary. The research also names tmsearch.uspto.gov, the WIPO Global Brand Database and EUIPO TMview without a fetched page.
- PetaPixel on the earlier Emulsion app, https://petapixel.com/2015/06/22/hands-on-emulsion-for-mac-is-a-simpler-alternative-to-adobe-lightroom, secondary.
- Softpedia listing of Emulsion 1.4.1, https://mac.softpedia.com/get/Graphics/Emulsion-The-Escapers.shtml, secondary.
