# Emulsion store release plan

The owner sells Emulsion on the Mac App Store first, paid upfront, with an on-device purchase check. The Microsoft Store waits until the Mac release ships (owner message 2026-11-18, paraphrased).
That is RFC-0038 decision D1 option (b), public and paid. RFC-0038 section B5 is the real path, and this plan builds on it.
The first rule the program enforces is that no build reaches the store until the RFC-0038 Phase C gate and this plan's store gate pass on that exact build.
The second rule is that every feature whose licence or data terms need a lawyer's answer is compiled out of the store edition. The default build, the open source build and development builds keep every feature.
Active PR ids in dependency order. Shared S0 to S5 and S7 to S11, Mac M1 to M6 and M8, release G1 and G2. Deferred PRs are S6, M7, W1 to W6 and G3.
Total cost of the active PRs is about 32.5 to 61 engineer-days (guess), plus research, account and App Review time.
**Status on 2026-11-23.** The owner answered the decisions on 2026-11-18, and four more on 2026-11-23. Wave 0 is four PRs (S0, S7, S8, M1). They started on 2026-11-18 and have merged. D15 (the name) is decided and recorded privately by the owner, and its trademark search, store-name check and domain are open. D18 now says macOS 13. D20 (the bundle ID) is settled as a rule, and the owner chooses its value at M6. D21 (what a failed purchase check does) is decided as a read-only mode. The licence research memo answered D2, D3 and D5 on 2026-11-18 as research, not legal advice, and waits for the owner to accept its stated risk.

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a screenshot, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs the `pstack` plugin's `skills/poteto-mode/playbooks/autopilot-stack.md`. The owner chose this execution policy on 2026-11-18 (owner message 2026-11-18, paraphrased). The owner agent of a PR that is not review-gated merges it once local checks and every CI check are green. A review-gated PR stops at verified, and the operator reviews and lands it. Each owner agent runs its own PR's live lanes, one after another. No ten-worker swarm runs per PR. A full swarm per the pstack `skills/swarm/SKILL.md` stays available when the operator asks for one. Autopilot-stack stays the program shape because 13 of the 20 active PRs are review-gated and stop for the owner. Run it in waves. A wave is the set of PRs whose parents have merged, listed under **Spawn owners**.

Four sets matter for scheduling.

- Decision-gated, because a decision or an owner action is still open. M6 waits for the trademark search and for the bundle ID (D20), which the owner chooses at M6 once the domain exists. G2 waits for the trademark search and the store-name check, because every listing asset carries the name. M8 is no longer gated, because D21 is answered. S5 waits for the owner to accept the licence research memo's stated risk before merge. S10 waits for the privacy-policy research memo. G1 waits for the owner's written acceptance of the risk each research memo states. Every other active PR has an answered decision.
- Review-gated, because they change an interaction or need a human at the screen. S2, S3, S4, S9, S10, M1, M2, M3, M4, M5, M6, M8 and G2.
- Need accounts, secrets or hardware that no agent holds. M6, M8 and G2. M8 needs the owner's TestFlight install and, for its copied-app lane, a second Mac with another Apple ID. The owner puts each secret into GitHub Actions secrets. Agents dispatch workflows that use the secrets and never read them.
- Deferred, out of every wave and every cost total until the owner reopens them. S6, because the store edition builds without lens profile correction. M7, because D12 has no trial. W1 to W6 and G3, because the owner deferred the Microsoft Store until the Mac release ships (owner message 2026-11-18, paraphrased). Their sections stay so the plan keeps its shape.

Costs are engineer-days for one person who knows the codebase. Every cost is a guess unless a box says otherwise. RFC-0038 B5 figures are reused where a row matches. The active total is about 32.5 to 61 engineer-days (guess). The plan of 2026-11-17 said about 50 to 168 for both stores and about 38 to 142 for the Mac store alone. The total fell for three reasons. The deferred PRs leave it (S6 6.5 to 12, M7 3 to 6, W1 to W6 12.5 to 25, G3 0.5 to 1, all guesses). The retrain routes of S2, S3 and S4 are gone, because the owner took the recommended defaults. S3 and S4 now compile features out instead of replacing models. S1 grew by 1.5 to 3 days for the compile-time gating and the clean rewrite of one lens kernel. S5 grew by 1 to 1.5 days for the public source repository and the notice gaps. M8 adds 2.5 to 5 days for the purchase check (guesses). The rewrite of M8 on 2026-11-23 keeps that figure, because the receipt parsing leaves and the Swift bridge, the purchase state machine and the read-only guard come in (guess).

Critical path. It is the Mac sandbox chain S0, S1, M2, M4, M6, M8, G1, G2, about 18 to 33 engineer-days (guess). The plan of 2026-11-17 gave S0, S1, M2, M4, M6, G1, G2 at about 14 to 28 days (guess). The path is longer because M8 sits between M6 and G1, and S1 carries the gating. Calendar time runs beside it. The trademark search and the bundle ID (D20, chosen at M6 after the owner buys a domain) gate M6. M8 needs M6's TestFlight build, the Swift bridge, the macOS 13 baseline and, for its copied-app lane, the owner's second Mac. App Review time is not published (sourced, research section 6). Those calendar items are likely the real critical path (guess).

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

## Program checklist

### Arm the program

- [ ] State the protocol and this plan to the operator, then stop. Start execution only on the operator's explicit go.
- [ ] Record the owner's answers in ADR-0011 through S1. Evidence is the owner's message in chat, quoted as "owner message 2026-11-18, paraphrased". A decision answered later is appended to ADR-0011 by the PR it gates.

The owner answered on 2026-11-18, and on 2026-11-23 for D15, D18, D20 and D21. Every answer below is owner message 2026-11-18, paraphrased, unless its row says 2026-11-23. The decisions of 2026-11-17, with their sources and alternatives, are kept in Appendix E.

| Decision | Owner's answer | Status | Gates |
| --- | --- | --- | --- |
| D14. Entity and store accounts | The owner already holds an Apple Developer account as an individual. No D-U-N-S number and no organisation. Microsoft Partner Center and the name reservation there are skipped for now. | Answered for Apple. Deferred for Microsoft. | M6, G2 |
| D7 and D11. Model data and face features | Follow the recommended defaults. Weights whose main training data has non-commercial terms stay out, and face features stay out of the first store release. | Answered | S1, S2, S3, S4 |
| D12. Mac price model | Paid upfront, one price, no trial. The app must check that the purchase is real, so a copied app does not run for other people. On 2026-11-23 D21 settled what "does not run" means. A copy that fails the check opens a read-only mode and is not blocked. The check does not stop a patched binary (Appendix C). | Answered. M7 is skipped and M8 is added. | M8, G2 |
| D16. Channel order | Mac App Store first. | Answered | Wave order |
| D18. Mac architecture and minimum macOS | Apple silicon only, as recommended (owner message 2026-11-18, paraphrased). On 2026-11-23 the owner raised the minimum to macOS 13, because StoreKit 2 `AppTransaction` needs it (sourced, its documentation page). `minimumSystemVersion` is 13.0 in the Mac store config (M6). Macs on macOS 12 cannot install the store build. The 2026-11-17 text said at least 12.0 (Appendix E). | Answered, raised 2026-11-23 | M6, M8 |
| D17. Map and place-search services | Online map and place search are off by default in release builds and on in debug builds. The user turns them on in Settings. | Answered | S9 |
| D8. Relicensing | Approved. The repository goes private, and future versions become proprietary. | Answered | S5 |
| D2 and D3. LibRaw licence | The owner has no lawyer and asked what CDDL is. CDDL-1.0 is a file-level licence that LibRaw LLC offers beside LGPL-2.1 (RFC-0038 D2, and `licenses/LibRaw-COPYRIGHT`). The licence research memo of 2026-11-18 elects CDDL-1.0 as an engineering default. CDDL section 3.5 lets the executable ship under the owner's own terms, and CDDL asks no relink. The only duty is to publish LibRaw's own source and say where it is (S5). | Answered by research, not legal advice. Waiting for the owner to accept the memo's stated risk. | S5 |
| D5. Lens database | The memo answers D5 by the compile-time switch. The store edition ships no lens profiles, which removes the `lensfun` crate, its LGPL-3.0 terms and the CC-BY-SA database (the reconciliation below). For a later release the memo says to ship the database files unmodified with a credit line and the licence URI. | Answered by research. Not gating. | S6 (deferred) |
| D15. The name | The owner chose a new product name on 2026-11-23 (owner message, paraphrased). The project name and the product name differ, and the repository, the open source build and the docs keep the name Emulsion. The chosen product name is recorded privately by the owner and is not in this repository. Until the owner has registered it and says so, no file, commit message, PR text or branch name here carries it. Where this plan needs it, it reads "the chosen product name" or `<product name>`. | Decided. Open items below, all the owner's | M6, G2 |
| D20. Bundle ID | The owner has no domain (owner message 2026-11-23, paraphrased). The bundle ID is chosen at M6, the first upload, not now. It becomes permanent only when the first build is uploaded. Before M6 the owner buys a small neutral studio domain, about US$10 to 15 per year (guess), and the ID is its reverse DNS plus a last label that does not use the product name, such as `photo`. The current identifier shows a personal handle and the old product name, so the store build must not keep it. The new identifier is set only in the Mac store config. Until a domain exists, the privacy policy URL can be hosted on GitHub Pages (S10). | Decided as a rule. The value is chosen at M6 | M6 |
| D21. A failed purchase check | The owner's decision of 2026-11-23 (paraphrased). Use StoreKit 2 `AppTransaction`, not the receipt APIs. Never terminate the app. If the check throws (offline or signed out), keep the full app for a grace period, 30 days (guess). If the result is unverified, the device does not match or the purchase is revoked, open a read-only mode (browsing and viewing, no editing, no export, no new imports) with "Verify purchase" and "Quit" buttons. The user restores by verifying or by buying the app again from the read-only screen (the owner's "resume payment" flow). The exit code 173 idea of 2026-11-18 is dropped, because Apple's current guidance says not to terminate the app and the API is deprecated. M8 lists the blocked and allowed actions. | Decided. Review risk medium (inferred, Appendix C) | M8 |
| D13, D19 and D9. Windows route, signing and activation | Skipped. | Deferred | W2 to W6, G3 |

**D15, the open items.** The owner does all of these. None of them needs an agent.

- Trademark search, by hand, for the chosen product name in Nice classes 9, 41 and 42. The registers are the USPTO, the WIPO Global Brand Database, EUIPO TMview, the UKIPO, TIPO, J-PlatPat and CNIPA. Agents could not query them, because they returned a bot challenge, a firewall rejection or a 403 (measured, private name research of 2026-11-18 and 2026-11-19). The result pages are kept privately.
- Store-name check at record creation. A public App Store search found no app whose title equals the name or begins with it, and the near matches sat in unrelated categories (measured, same research). That does not prove App Store Connect accepts the name, because it also blocks names held by unreleased or removed records (inferred). Creating the record is the only check that shows those (inferred).
- Domain. The owner has no domain. The same research found the `.com`, `.app` and `.dev` forms of the chosen name already registered by others (measured with RDAP), so a website named after the product needs another form. The studio domain of D20 serves instead.
- Reading of the name. Public search results for the word will mix in unrelated uses (inferred). A native Mandarin speaker should say the name aloud and react, and so should speakers of the other languages the owner sells in (guess).

**D20, the order of account actions.** Apple says a bundle ID can't change after a build is uploaded, and that deleting the explicit App ID after an upload is not possible (sourced, Apple pages in Appendix D, retrieved 2026-11-23). Removing a record does not free its bundle ID once a build was uploaded, and a SKU can never be reused in the same account (sourced). Before any upload, the bundle ID on a record and its explicit App ID can still change (inferred, from the "after you upload" wording of those pages). So the plan sets this order.

- Step 1. The owner runs the trademark search above.
- Step 2. The owner buys the studio domain. Bundle IDs use only A to Z, a to z, 0 to 9, hyphen and period, and are case-insensitive, so the ID is written in lowercase (sourced). The reverse DNS of the domain keeps it unique (sourced, Apple's advice). A last label such as `photo` survives a later rename of the product (inferred).
- Step 3, optional. The owner creates the App Store Connect record early, only to hold the name, and only after step 1. Creating the record needs an explicit App ID equal to the bundle ID, so creating it early means choosing the bundle ID early. It also needs a SKU. The SKU is neutral, with no product name in it, because it can never be reused (sourced). The name stays editable until the app is submitted for review (sourced). Apple's current pages state no time limit on a record without a build, and older forum reports of 120 or 180 days are unconfirmed (unofficial).
- Step 4. At M6 at the latest, the owner chooses the bundle ID, registers the explicit App ID and uploads the first build. That upload makes the ID permanent. An early upload to remove doubt about name reservation waits until the bundle ID is final (inferred).
- The owner, not an agent, performs every account action. That covers buying the domain, registering the App ID, creating the record, creating certificates and keys, and signing agreements. The owner reports each result in chat. Agents never see a secret.
- The current identifier of the default config shows a personal handle and the old product name, and it appears in every `Info.plist`, in the sandbox container path and in crash reports (inferred). The store build must not keep it. M6 sets the new identifier only in `tauri.appstore.conf.json`. The default build keeps its current identifier, and a sandboxed store build gets its own container anyway (inferred). S0's lane builds use their own per-lane identifiers and are never uploaded.
- The privacy policy URL is required for a macOS app (sourced, App Store Connect help). Until the owner has a domain it is a GitHub Pages page (S10).

**Compile-time legal gating, and how it meets the recommended defaults.** The owner asked that every feature whose licence or data terms need a lawyer's answer is switched off at compile time in the first store release, while the default build and the open source build keep everything (owner message 2026-11-18, paraphrased). The plan reconciles that with the recommended defaults as follows. The owner can overrule any line.

- Remove ships in the store edition. It runs PatchMatch (`inpaint_patchmatch.rs`, S2), which is Emulsion's own code and needs no lawyer's answer. LaMa stays out.
- Face grouping is compiled out of the store edition. The eye check goes with it, because `blink_commands.rs` refuses unless both the `blink` and the `face` families are in the edition, and it loads the YuNet detector (measured, lines 40 to 57). This differs from the 2026-11-17 default, which kept the eye check with a swapped detector. The detector swap becomes a later release.
- AI Upscale is compiled out, because the Real-ESRGAN training data is unknown and the maintainer question is skipped.
- The D-FINE person detector is compiled out, because of the ImageNet backbone question.
- Lens profile correction is compiled out with a Cargo feature. That removes the LGPL-3.0 `lensfun` crate and the CC-BY-SA lens database it embeds from the store binary (S1). The profile path of the render code goes with it, because it holds app code described as a port of lensfun math (RFC-0039 section 3). Manual lens corrections stay. Manual distortion calls one ported kernel, so S1 rewrites that kernel clean, about half a day (guess), or the store edition drops the Manual distortion slider. S6, the clean-room replacement of RFC-0039, is deferred.
- The store edition leaves out the LaMa, Real-ESRGAN, D-FINE, YuNet, SFace and Face Mesh models, as the licence research memo lists. It keeps NAFNet (AI Denoise) and SAM 2 (Select Subject). The memo rates NAFNet clean and keeps SAM 2 with one residual question, because the SA-1B licence text was not read (memo section 9). Whether training-data terms reach weights at all is still a lawyer's question (Appendix C).
- Online map and place search stay in every build behind the D17 switch. They are not compiled out, because the owner asked for a setting.

"Compiled out" means two mechanisms, and S1 states which one each feature uses. A model-driven feature is fixed out by the compiled edition name of ADR-0012 together with the absent weights and no download path. The lens profile path is removed from the binary by a Cargo feature, because its open question is about code and data inside the binary. S1 gives the evidence for that split.

- [ ] Track the owner's actions. No agent can do these. Evidence for an owner action is the owner's message in chat, unless the box names other evidence.
  - [ ] Request the D-U-N-S number. Skipped, because the owner enrols as an individual (owner message 2026-11-18, paraphrased).
  - [x] Enrol in the Apple Developer Program. Done before 2026-11-18. Evidence is owner message 2026-11-18, paraphrased.
  - [ ] Open the Microsoft Partner Center account and reserve the name there. Skipped for now. Deferred with the Windows PRs.
  - [ ] Run the trademark search for the chosen product name, by hand, in Nice classes 9, 41 and 42, on the USPTO, WIPO Global Brand Database, EUIPO TMview, UKIPO, TIPO, J-PlatPat and CNIPA registers. Agents could not query them (D15, open items). Wave 0 and the sandbox work do not need it. It must be done before the App Store Connect record, before any listing asset and before G2. Evidence is the owner's message that it is done, with the result pages kept privately.
  - [ ] Buy a small neutral studio domain, about US$10 to 15 per year (guess), before M6, and say so in chat. It gives the D20 bundle ID and a home for the privacy policy URL. Evidence is the owner's message. The domain name is not written in this repository until the owner says so.
  - [ ] Create the App Store Connect app record, optionally early to hold the name. Do this only after the trademark search is done. The record needs an explicit App ID equal to the bundle ID and a SKU that can never be reused, so choose the bundle ID first (D20) and use a neutral SKU with no product name. Creating the record is also the store-name check. Evidence is a screenshot of the record.
  - [ ] Tell the agents when the chosen product name is registered, so that it may enter the repository (the Mac store build's display name at M6, the listing at G2). Until then the build reads it from a GitHub Actions secret. Evidence is the owner's message.
  - [ ] Keep a second Mac, signed in to an Apple ID that never got the app, available for M8 lane 10. Evidence is the owner's message, or a note that none is available.
  - [ ] Get a research memo for each open licence question, written by an agent from primary sources, with the risk the owner accepts stated in it. Evidence is the memo file and the owner's message accepting its risk. Appendix C lists the questions a lawyer would still need to answer.
    - [ ] D2, D3 and D5, the licence research memo. Written on 2026-11-18 as research, not legal advice. It is held outside the repository. Check this box when the owner accepts its stated risk, and commit the memo beside ADR-0011 through S1.
    - [ ] D8, the published MIT history and the AI-assisted commits. The licence research memo names it a question that still needs a lawyer (its section 10, question 1). S5 states the risk. Check this box when the owner accepts it or asks a lawyer.
    - [ ] The privacy policy and the licence agreement for the Mac store. The licence research memo gives the open-source clause (its section 4). The privacy text is not started, and it must cover the purchase check of M8. S10 needs both, and the policy URL it publishes (GitHub Pages until the owner has a domain).
  - [ ] Ask Microsoft Partner Center support about Paid MSI payment collection. Skipped, deferred with D13.
  - [ ] Ask the Real-ESRGAN maintainer about the training data. Skipped, so AI Upscale is off in the first store release.
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

- [ ] Spawn one owner per active PR with the full lifecycle the execution playbook names. Owners write code on `sonnet medium` (the `feature, refactoring` line of the pstack model rule). S1, M2 and M8 go to `opus high` (the `hardest tasks` line). S1 crosses many call sites, M2 holds the bookmark layer, and M8 holds the purchase state machine and the read-only guard that spans every write command.
- [ ] Follow this dependency graph. Start dependent work only after its parent merges, or base it on the parent branch when the execution playbook stacks.
  - [ ] Wave 0, no decision needed. S0, S7, S8 and M1 are independent and first. All branch from `main`. All four started on 2026-11-18.
  - [ ] Wave 1. S1 after S0. S5 after S0, with its merge held until the owner accepts the licence research memo's stated risk. S9 after S0.
  - [ ] Wave 2. S2 and S3 after S1. S4 after S3, because both edit `adaptive_commands.rs`. S11 after S1. M2 after S1 and M1. M5 after S1.
  - [ ] Wave 3. M3 and M4 after M2. S10 after S5, S9 and the privacy-policy research memo.
  - [ ] Wave 4. M6 after S11, M2 and M4, and after the trademark search and the bundle ID (D20), which the owner settles at M6 start.
  - [ ] Wave 5. M8 after M6, because its live lanes need a TestFlight build. It also needs the macOS 13 baseline of D18, the Swift bridge (its first box) and, for lane 10, the owner's second Mac. D21 is answered.
  - [ ] Wave 6. G1 after S2, S3, S4, S5, S7, S8, S9, S10, M1, M3, M4, M5 and M8, and after the owner accepts each research memo's stated risk.
  - [ ] Wave 7. G2 after G1, the trademark search and the store-name check.
  - [ ] Deferred, in no wave. S6, M7, W1, W2, W3, W4, W5, W6 and G3. When the owner reopens one, its own **Depends on** block names its parents.
- [ ] Hold the file boundaries. Two PRs in the same wave may run in parallel only on disjoint files.
  - [ ] S0 touches only `.github/workflows/lanes.yml`, `scripts/lanes/**`, `app/e2e/wdio.sandbox.conf.js`, `app/e2e/specs/{perf-probe,sandbox-smoke}.e2e.js` and `app/src-tauri/entitlements/lane-*.entitlements`.
  - [ ] Only S1 creates `app/src-tauri/src/channel.rs`, the Cargo features and the `store` edition in `models/bundled-models.json`. S3 and S4 do not edit the manifest. Both edit `resolve_adaptive_region` in `adaptive_commands.rs`, so S4 starts after S3 merges.
  - [ ] S1 adds the `store-edition` job to `.github/workflows/ci.yml`. S11 later moves the vcpkg step of `ci.yml` into a shared action. S11 runs after S1, so they never edit `ci.yml` at once.
  - [ ] M2 owns `app/src-tauri/src/sandbox_access/**`, `catalog/folder_grants.rs` and the setup block of `lib.rs`. S1 edits the `lens_profile` lines of the same setup block and merges first.
  - [ ] S11 owns `.github/workflows/release.yml` as a reusable workflow. M6 adds only `release-mas.yml`. M8 adds the `mas-purchase-check` feature, the Swift package under `app/src-tauri/swift/`, the `purchase/` module and one row per command in its policy table, and it edits only the feature list that `release-mas.yml` passes to the build. M8 starts after M2 and M6 have merged, so it never edits the setup block of `lib.rs` at the same time as M2, and no PR after M8 edits command files before G1.
- [ ] Hold the review gate. S2, S3, S4, S9, S10, M1, M2, M3, M4, M5, M6, M8 and G2 wait for the operator's review in chat with screenshots and a video before merge.

### PR mechanics, for every PR

- [ ] Resolve the forge once. Default to `gh`; if `command -v origin` succeeds and Origin can resolve the repository, use `origin pr` for every PR operation. Record any fallback to `gh`. Never require `gt`.
- [ ] Open the PR ready, never draft, per **Opening a PR**. Use the run's built-in PR tool when it has one, else `origin pr create --status open --base <base-branch>` or `gh pr create --base <base-branch>` according to the resolved forge. A stack child targets its parent branch.
- [ ] Run the repo's lint and typecheck once before the PR-facing push (`npm run check`, `npx vitest run`, `cargo test` in `app/src-tauri`). Push with hooks on.
- [ ] Run `/deslop` before each commit and `/no-comments` before review.
- [ ] Triage every Bugbot and security-reviewer comment per the pstack `skills/poteto-mode/references/bugbot-triage.md`.
- [ ] Add a `PROGRESS.md` entry in the house style with a "Not verified" list, and update `docs/PROJECT_STRUCTURE.md` when a module moves (owner's standing practice).
- [ ] Rebase onto current trunk before the code-ready report and babysit. Keep that merge base in fix rounds. Rebase again only at merge prep, on a `git merge-tree` conflict with trunk, on a CI failure that comes from a change on trunk, or when a known concurrent change to the same files lands.

### Verdict and merge, for every PR

- [ ] The owner agent runs the verification itself at the code-ready head, one lane after another. That is the local checks, the ten live lanes from the PR's **Verify, live** block, and the perf lane from its **Verify, perf** block. The owner chose this on 2026-11-18 instead of a ten-worker swarm per PR (owner message 2026-11-18, paraphrased). A full swarm per the pstack `skills/swarm/SKILL.md` runs only when the operator asks for one. A rebase, a merge-conflict resolution, or a minor fix reruns the lanes the change can reach, and a wording-only change reruns none. The root audits the receipts in the merge-ready report before the verdict. Lanes run in sequence take more wall time and give less independence than a swarm, because the agent that wrote the code also checks it (inferred). The root's audit of the receipts is the counterweight.
- [ ] Judge every finding by the execution playbook's blocking rule. The verdict is clean when none blocks. A failure on trunk behavior in a lane marked `Regression lane against trunk.` is recorded against trunk and does not block the PR. A new head gets a fresh verdict, except for results that stay valid under the patch-id rule in `playbooks/shipping.md`.
- [ ] Merge by the owner's standing practice. A PR that is not review-gated is squash-merged by its owner agent once the verdict is clean, local checks pass and every CI check is green. A review-gated PR stops at verified. The root lists it with its verdict, and the operator reviews and lands it. A restack that leaves a PR's `git patch-id` unchanged keeps its verdict (the patch-id rule in `playbooks/shipping.md`). Nobody pushes to a review-gated PR after the handoff without the operator's go.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] The owner agent squash-merges it once local checks and every CI check are green.

## Add the channel flag, the store edition and compile-time legal gating, and record the decisions (S1, shared)

**Depends on.** S0. D7, D11, D12, D16 and D17 are answered (owner message 2026-11-18, paraphrased), and the licence research memo gives the compile-out list. Cost 3 to 5.5 engineer-days (guess). That is 1.5 to 3 days more than the 2026-11-17 figure, for the Cargo feature, the profile render path, the clean rewrite of one lens kernel, the edition fields and the CI job. One codebase makes the default build and the Mac store build. The Windows store channel joins when the owner reopens the Windows PRs.

**Files.**

- [ ] Edit `app/src-tauri/Cargo.toml`. Make `lensfun` optional, and add the features `default = ["lens-profiles"]`, `lens-profiles = ["dep:lensfun"]` and `channel-mas = []`.
- [ ] Create `app/src-tauri/src/channel.rs`. Edit `app/src-tauri/src/editions.rs` (`compiled_name`, `EditionInfo`) and `app/src-tauri/src/lib.rs` (`mod lens_profile`, `lookup_lens_profile`, the `warm_lens_db` call in `setup`, and the download commands in the handler list).
- [ ] Edit the profile path of `develop_engine/lens.rs` and of `app/src/lib/gpu/shaders/lens.js`, and rewrite `lens_undist_poly3` and its WGSL twin `lensUndistPoly3` clean.
- [ ] Edit the crate-dependent tests in `lens_profile.rs` and `develop_engine/tests/geometry.rs` so they compile only with `lens-profiles`. Edit `export_plugin.rs`, the download functions of the `*_models.rs` files, and `storage.rs` `move_cache_dir`.
- [ ] Edit `models/bundled-models.json` (new `store` edition), `app/src/lib/modelEdition.js`, `app/src/lib/api/system.js`, the Lens Corrections panel in Develop, and the Settings rows that read the channel.
- [ ] Edit `.github/workflows/ci.yml` (new `store-edition` job). Create `docs/adr/ADR-0011-paid-store-distribution.md` with every answered decision of RFC-0038 and of this plan, and add a dated note to ADR-0012 that points at it.

**Build.**

- [ ] Record the evidence for the split in ADR-0011. These facts were measured on `main` at `93ee992`.
  - [ ] Lens profiles. `lens_profile.rs` says it is the only file in the crate that touches the `lensfun` types (its header, lines 3 to 8). The database ships inside the crate binary through `Database::load_bundled()` (lines 12 to 13 and 35). Production reaches it only through `lookup_lens_profile` and `warm_lens_db` in `lib.rs`. The render path in `develop_engine/lens.rs` works from coefficients baked into the edit stack and never touches the crate (its header, lines 8 to 10). So one Cargo feature on one module removes the LGPL-3.0 crate and the CC-BY-SA database from the binary.
  - [ ] Model families. `editions.rs` fixes the edition name at compile time through `option_env!("EMULSION_EDITION")`. An excluded family is refused even when a copy is cached (ADR-0012 rule 3), and the fetch script leaves its weights out of the bundle (rule 6). ADR-0012 rejected a Cargo feature per family, because it multiplies CI cells and cannot express a family with no file. That reason still holds, and the open questions attach to the weights and their training data, not to Emulsion's own code (RFC-0038 B5.5). So face grouping, the eye check, AI Upscale and the person detector stay on the edition mechanism.
  - [ ] Online services. `geocode.rs` and `mapTiles.js` stay in every build. S9 gives them the D17 default.
- [ ] Compile out the profile path of the render code too. RFC-0039 regions 2 to 4 are the app's own code, described as ports of lensfun math (sourced, RFC-0039 section 3). Whether they are LGPL-3.0 code is a lawyer's question (licence research memo, section 10, question 3). Without `lens-profiles`, `develop_engine/lens.rs` compiles out `LensNorm::for_profile` and the profile kernels. The store build sets `VITE_LENS_PROFILES=off`, and `lens.js` adds the profile functions to the shader only when the flag is not `off`, so Vite drops them from the store bundle (inferred). A stored profile op then renders without its profile part in the store build, and the op stays in the catalog (inferred, the memo says the same).
- [ ] Rewrite the one ported kernel that Manual distortion calls. That is `lens_undist_poly3` and its WGSL twin `lensUndistPoly3` (measured in the memo, `develop_engine/lens.rs` lines 60 and 406 to 408, `lens.js` lines 71 and 213). A fresh agent session writes both from the published Newton iteration under RFC-0039 section 5's clean-room rules, about half a day (guess, the memo's figure). The rewrite replaces the port in every build only if the existing hand-derived `geometry.rs` tests pass and the old and new kernels agree within 0.05 px on a sample grid. If it does not, the store edition drops the Manual distortion slider instead, and the default build keeps the old kernel.
- [ ] `channel-mas` fixes the edition at compile time. Under `cfg(feature = "channel-mas")`, `compiled_name()` returns `Some("store")` whatever `EMULSION_EDITION` says. A `compile_error!` fires when `channel-mas` and `lens-profiles` are both on, so a store build that leaves the default features on does not compile.
- [ ] The `store` edition includes `denoise` and `segment` only. It leaves out `inpaint` (LaMa, and S2 gives Remove its PatchMatch path), `detect` (D-FINE), `upscale` (Real-ESRGAN), `face` (YuNet and SFace) and `blink`. `blink` goes because `blink_commands.rs` refuses unless `face` is also in the edition (measured, line 40).
- [ ] `Channel` is `Direct` or `MacAppStore`, chosen by the `channel-mas` feature. `ChannelCaps` is one table row per channel with `export_command_hook`, `model_downloads` and `custom_cache_dir`. The Mac store binary compiles out `export_plugin` and every model download command, so it holds no code that starts a user program or fetches a model (Apple guidelines 2.5.2 and 2.4.5(iv), sourced).
- [ ] The frontend learns what is compiled in from the command it already calls. `system.js` calls `model_edition` (measured, line 58). `EditionInfo` gains `channel` and `compiled_out`. `compiled_out` lists `lens_profiles` when the build lacks `lens-profiles`. No new command is added.
- [ ] Without `lens-profiles`, `lookup_lens_profile` stays registered and returns `None`, so the frontend call never fails. The Lens Corrections panel shows "Lens profile correction is not included in this edition of Emulsion." in the Profile part when `compiled_out` lists `lens_profiles`. The Manual sliders stay. `editionAboutLine` names the feature in Settings, About.
- [ ] Settings hides Plugins, download buttons and "Move cache" when the cap is off.
- [ ] The `store-edition` CI job on `macos-latest` runs `cargo test --no-default-features --features channel-mas` and `cargo tree -e normal --no-default-features --features channel-mas -i lensfun`, and fails when the tree finds the package. The existing default jobs do not change.
- [ ] Check first that `npx tauri build -- --no-default-features --features channel-mas` reaches cargo with both flags. The Tauri CLI passes arguments after `--` to cargo (inferred, not tried). Record the result in the PR.

**You see.**

- [ ] In the Mac store sandbox build, Settings has no Plugins section, Lens Corrections shows the sentence in place of the profile, and `nm` on `emulsion` finds no `lensfun` and no `export_plugin` symbol.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The `geometry.rs` Manual distortion tests pass unchanged with the rewritten kernel. `channel.rs` tests each row of the caps table. The `editions` tests assert that `store` lists exactly `denoise` and `segment`, that a `channel-mas` build resolves to `store`, and that `compiled_out` serialises as `["lens_profiles"]` without the feature. `modelEdition.test.js` gains the About line with lens profiles. Run `cargo test && cargo test --no-default-features --features channel-mas && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Run `golden-path` on the debug build, default features, at trunk and head. Save `golden.png`. Pass when both pass with the same grid.
- [ ] Lane 2. Sandbox build, channel `mas`, open Settings. Save `mas-settings.png`. Pass when Plugins and "Move cache" are absent.
- [ ] Lane 3. `nm -gU` and `strings` on the `mas` binary for `lensfun`, `export_plugin`, `huggingface.co` and `media.githubusercontent.com`, and `grep` of the store frontend bundle for `lensNormForProfile`. Save `mas-symbols.txt`. Pass when none is found.
- [ ] Lane 4. Sandbox build, `mas`, open `exif-shot-info.jpg` in Develop and open Lens Corrections. Save `mas-lens.png`. Pass when the Profile part shows the sentence and a Manual distortion change moves the preview.
- [ ] Lane 5. Sandbox build, `mas`, open Enlarge, face grouping, the Cull eye check and the person preset in turn. Save `mas-excluded.png`. Pass when each shows "{Feature} is not included in this edition of Emulsion." and no dialog opens.
- [ ] Lane 6. Debug build, default features, `exif-shot-info.jpg` in Lens Corrections. Save `default-lens.png`. Pass when the profile matches and applies as on trunk.
- [ ] Lane 7. Debug build, default features, Settings. Save `direct-settings.png`. Pass when Plugins, downloads and "Move cache" are present as on trunk.
- [ ] Lane 8. `cargo build --features channel-mas` with the default features left on. Save `gating-guard.txt`. Pass when the build fails with the `compile_error!` text.
- [ ] Lane 9. `cargo tree -e normal -i lensfun` for the default build and for the store build. Save `lensfun-tree.txt`. Pass when the default build lists the crate and the store build finds no such package.
- [ ] Lane 10. Sandbox build, `mas`, Settings, About. Save `mas-about.png`. Pass when the edition line names exactly the features the `store` edition and `compiled_out` leave out.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the default build at trunk and head. Diff-added work is the size of the `mas` `emulsion` binary.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` on `macos-latest`, and `stat -f %z` on both binaries.
- [ ] Baseline. Record the trunk `first_grid_ms` median and the default binary size first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent. Fail if the `mas` binary is not smaller than the default binary, because the embedded lens database leaves it.

**Review gate.** None. S1 is not review-gated. The default build is unchanged, and the store build reuses the existing "not included" sentence.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] The owner agent squash-merges it once local checks and every CI check are green.

## Make Remove work without LaMa in the store edition (S2, shared)

**Depends on.** S1. D7 is answered with the recommended default (owner message 2026-11-18, paraphrased), so the route is PatchMatch, RFC-0038 B5.5 LaMa route (i). Cost 1 to 2 engineer-days (guess). The retrain route (ii) is dropped from this plan.

**Files.**

- [ ] Edit `app/src-tauri/src/removal_commands.rs`, the Remove status in the frontend and `FAMILY_FEATURE` handling in `app/src/lib/modelEdition.js`.
- [ ] Create `docs/adr/ADR-0013-remove-without-lama.md`, which records the reversal of ADR-0010 for the store edition.

**Build.**

- [ ] When `inpaint` is not in the edition, Remove runs `inpaint_patchmatch.rs` and reports `removal_model_status` as a new `patchmatch` state instead of `not_in_edition`.
- [ ] The About edition line stops naming Remove when Remove runs on PatchMatch, because the feature is present.

**You see.**

- [ ] In the `store` edition, Remove fills a brushed area, and no "not included" sentence appears.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A `removal_commands` test asserts the engine choice per edition. Run `cargo test removal`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Run `develop-remove-tool-real-model` on the default edition at trunk and head. Save `lama.png`. Pass when both pass and LaMa ran.
- [ ] Lane 2. Sandbox build, `store` edition, `develop-remove-tool` on `Field-corn-Liechtenstein-landscape.jpg`. Save `store-remove.png`. Pass when the patch is applied and no "not included" text is in the DOM.
- [ ] Lane 3. Same, then export one JPEG. Save `store-remove-export.png`. Pass when the exported file shows the patch.
- [ ] Lane 4. Open, in the `store` build, a catalog copy holding a LaMa patch made by the default build. Save `old-patch.png`. Pass when the stored patch still renders (ADR-0012 rule 4).
- [ ] Lane 5. `store` build, remove, then undo and redo. Save `undo-redo.png`. Pass when History lists one Remove entry and redo restores it.
- [ ] Lane 6. `store` build, `develop-removal-no-longer-fits`. Save `no-longer-fits.png`. Pass when the spec passes.
- [ ] Lane 7. Runner lane, `windows-latest`, `store` edition, `develop-remove-tool`. Save `win-remove.png`. Pass when the patch is applied.
- [ ] Lane 8. `ls Contents/Resources/models` of the `store` release artifact. Save `store-models.txt`. Pass when no LaMa file is present.
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
- [ ] Stop at verified. The operator reviews and lands it.

## Compile the face features out of the store edition (S3, shared)

**Depends on.** S1. D7 and D11 are answered (owner message 2026-11-18, paraphrased). Cost 0.5 to 1.5 engineer-days (guess), down from 3 to 29. S1 already leaves `face` and `blink` out of the `store` edition. This PR makes every face surface say so, and proves that nothing scans or downloads. The detector swap of RFC-0038 B5.5 (YuNet row, 2 to 4 days) becomes a later release.

**Files.**

- [ ] Edit the People view and the face-grouping entry points in the Library frontend, and the eye-check result in `app/src/lib/components/SimilarView.svelte`.
- [ ] Edit `resolve_adaptive_region` in `app/src-tauri/src/adaptive_commands.rs` for the `face` region, and the face and blink tests in `app/src-tauri/src/face_models.rs` and `app/src-tauri/src/blink_commands.rs`.

**Build.**

- [ ] Every face entry point in an edition without `face` shows the one sentence and starts no scan and no download. ADR-0012 says an edition without `face` refuses before any download. This PR adds the test that proves it.
- [ ] Stored face rows from another build stay readable, and no new scan runs (ADR-0012 rule 4). ADR-0012 lists this case as not verified, and lane 4 closes it.
- [ ] The adaptive `face` region refuses with the sentence in an edition without `face`. Today it reads stored face rows and may load YuNet to score them (measured, `adaptive_commands.rs` lines 80 to 121).
- [ ] The Cull eye check shows the sentence in place of its result, because `blink_for_images` returns `not_in_edition` (measured, `blink_commands.rs` line 40).
- [ ] No face-consent dialog is built. No store build scans faces, so the face half of the old S9 leaves the plan.

**You see.**

- [ ] In the `store` edition, People and the eye check each show "{Feature} is not included in this edition of Emulsion." The default build behaves as on trunk.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A `face_models` test asserts that an edition without `face` refuses before the download function runs. Vitest cases cover the People and eye-check gate states. Run `cargo test face blink && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Run `similar-eyes-check` on the default edition at trunk and head. Save `eyes.png`. Pass when the same photos are flagged.
- [ ] Lane 2. Sandbox build, `store`, open People. Save `store-people.png`. Pass when the sentence shows and no scan starts.
- [ ] Lane 3. Sandbox build, `store`, eye check on `Smiling-woman-pink-shirt-portrait.jpg`. Save `store-eyes.png`. Pass when the sentence shows in place of the result.
- [ ] Lane 4. `store` build on a catalog copy that holds face rows from a default build. Save `old-faces.png`. Pass when the rows stay readable and no new scan runs.
- [ ] Lane 5. `/usr/bin/log show` and the spec's network log during lanes 2 to 4. Save `no-download.txt`. Pass when no request reaches `media.githubusercontent.com`.
- [ ] Lane 6. Runner lane, `windows-latest`, `EMULSION_EDITION=store`, open People. Save `win-people.png`. Pass when lane 2's result repeats.
- [ ] Lane 7. Default edition, face grouping on the portrait fixture. Save `default-group.png`. Pass when grouping works as on trunk.
- [ ] Lane 8. `ls Contents/Resources/models` of the `store` artifact. Save `store-face-models.txt`. Pass when `face_landmarks.onnx` is absent and no YuNet or SFace file is present.
- [ ] Lane 9. `store` build, call `blink_for_images` from the spec. Save `blink-refused.txt`. Pass when it returns `not_in_edition`.
- [ ] Lane 10. `store` build, Settings, About. Save `store-about.png`. Pass when the edition line names face grouping and the eye check.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the default edition, which a gate check at startup could slow.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms`.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge. The owner sees what a store user gets in place of face grouping and the eye check.

- [ ] Copy lane 2 and lane 3 screenshots into `/tmp/store-review/S3-review-people.png` and `/tmp/store-review/S3-review-eyes.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S3-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator reviews and lands it.

## Compile AI Upscale and the person detector out of the store edition (S4, shared)

**Depends on.** S1, then S3, because both edit `adaptive_commands.rs`. D7 is answered, and the Real-ESRGAN maintainer question is skipped, so AI Upscale stays out (owner message 2026-11-18, paraphrased). Cost 0.5 to 1.5 engineer-days (guess), down from 0.5 to 13. S1 already leaves `upscale` and `detect` out of the `store` edition. This PR makes both surfaces and the batch path say so. A retrain or a SAM 2 fallback for the person preset is a later release.

**Files.**

- [ ] Edit `app/src/lib/components/EnlargeDialog.svelte`, `app/src/lib/actions/enlargeActions.js` and the adaptive person preset in the Develop frontend.
- [ ] Edit `resolve_adaptive_region` in `app/src-tauri/src/adaptive_commands.rs` for the `person` region, and the step list of the adaptive batch.

**Build.**

- [ ] Enlarge and the person preset each show the one sentence in an edition without their family.
- [ ] The adaptive batch skips a step whose family is excluded and names the skipped feature in its result. ADR-0012 lists the person preset with `detect` excluded as not verified, and lanes 3 and 10 close it.

**You see.**

- [ ] In the `store` edition, Enlarge and the person preset each show the "not included" sentence, and a batch finishes with the skipped step named.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] An `adaptive_commands` test for the skipped step, and Vitest cases for both gate states. Run `cargo test adaptive editions && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Run `develop-adaptive-person-real-model` on the default edition at trunk and head. Save `person.png`. Pass when both produce the same mask.
- [ ] Lane 2. Sandbox build, `store`, Enlarge on the landscape fixture. Save `store-enlarge.png`. Pass when the sentence shows and no job starts.
- [ ] Lane 3. Sandbox build, `store`, person preset on the portrait fixture. Save `store-person.png`. Pass when the sentence shows and no mask is drawn.
- [ ] Lane 4. `store` build, a catalog copy holding an Enlarge result from a default build. Save `old-enlarge.png`. Pass when the stored file opens.
- [ ] Lane 5. Runner lane, `windows-latest`, `EMULSION_EDITION=store`, lanes 2 and 3. Save `win-s4.png`. Pass when the results equal the Mac lanes.
- [ ] Lane 6. Default edition, Enlarge. Save `default-enlarge.png`. Pass when it works as on trunk.
- [ ] Lane 7. `store` build, Settings, About. Save `store-about.png`. Pass when the edition line names Enlarge and person detection.
- [ ] Lane 8. `ls Contents/Resources/models` of the `store` artifact. Save `store-models.txt`. Pass when only the NAFNet and SAM 2 files are present.
- [ ] Lane 9. `store` build, call `upscale_run` from the spec. Save `upscale-refused.txt`. Pass when it returns the refusal and writes no file.
- [ ] Lane 10. `library-adaptive-batch` on the `store` build. Save `batch.png`. Pass when the batch completes and names the skipped person step.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Person-mask time and Enlarge time on one fixture, default edition, trunk and head.
- [ ] Probe. `perf-probe` cases for both, through `perf-interleave.sh`.
- [ ] Baseline. Record both trunk medians first.
- [ ] Rule. Fail if either head median exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge. The owner sees what a store user gets in place of Enlarge and the person preset.

- [ ] Copy lane 2 and lane 3 screenshots into `/tmp/store-review/S4-review-enlarge.png` and `/tmp/store-review/S4-review-person.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S4-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator reviews and lands it.

## Change the licence terms, publish the LibRaw source and close the notice gaps (S5, shared)

**Depends on.** S0 for the lanes, and nothing in code. D8 is answered (owner message 2026-11-18, paraphrased). The licence research memo of 2026-11-18 answers D2 and D3 as an engineering default, which is research and not legal advice. It elects CDDL-1.0 for LibRaw. CDDL section 3.5 lets the executable ship under the owner's own terms, and CDDL has no relink duty (inferred in the memo, checked against `licenses/LibRaw-LICENSE.CDDL`). The owner agent holds the merge until the owner accepts the memo's stated risk. Cost 2.5 to 4.5 engineer-days (guess). That is RFC-0038 B5.2 1 to 2 plus B5.4 (a) 0.5 to 1, plus about half a day for the public source repository (the memo's guess) and 0.5 to 1 for the notice gaps (guess). B5.1 is done (measured, PROGRESS slice 1 and A1).

**Files.**

- [ ] Edit `LICENSE`, the `license` fields in `app/package.json` and `app/src-tauri/Cargo.toml`, `README.md` and line 3 of `THIRD_PARTY_NOTICES.md`.
- [ ] Edit `scripts/gen_third_party_libraries.py`, `THIRD_PARTY_LIBRARIES.md` and `licenses/README.txt`. Create `licenses/rust-crates.txt`, `licenses/LibRaw-third-party-BSD.txt` and `licenses/IJG-notice.txt` through the generator.
- [ ] Create `scripts/release/export_open_source.sh`, which exports the public source repository's folders at a release tag.
- [ ] Edit the About copy in `app/src/lib/components/SettingsDialog.svelte`.

**Build.**

- [ ] The relicensing text is the owner's. The memo names the published MIT history and the AI-assisted commits as a question that still needs a lawyer (its section 10, question 1). The PR body states that risk and quotes the owner's acceptance.
- [ ] Fill the "elected licence" column. LibRaw's row says "Emulsion uses LibRaw 0.21.3 under the CDDL-1.0, as LibRaw LLC permits. Its source is at {URL}." `LibRaw-LICENSE.CDDL` becomes the operative text. `LibRaw-COPYRIGHT` and the LGPL text stay, because the COPYRIGHT file names both (memo section 9).
- [ ] Publish the source that CDDL section 3.1 and MPL-2.0 section 3.2(a) ask for. The repository itself goes private (D8), so the owner creates one small public repository, tagged per release. It holds LibRaw 0.21.3 exactly as built, with its CDDL text and COPYRIGHT file. On macOS that is `vendor/rsraw-sys/LibRaw` unchanged, because `vendor/rsraw-sys/PATCH.md` changes only `build.rs` and `Cargo.toml` (measured in the memo). It also holds the two MIT wrapper crates `rsraw-sys` and `rsraw`, including the generated `src/bindings.rs`, and the five MPL-2.0 crates from `cargo vendor` or links to them on crates.io. `export_open_source.sh` writes those folders at a release tag and fails when a file differs from the tag. Evidence is the public URL and the tag.
- [ ] Link that repository from the About window and from `licenses/README.txt`.
- [ ] Close the three notice gaps the memo found (measured in the memo, section 7). The generator writes each runtime crate's own licence files, with their copyright lines, into `licenses/rust-crates.txt`. It copies the BSD notices of LibRaw's DCB, FBDD and X3F parts from their source headers into `licenses/LibRaw-third-party-BSD.txt`. It writes the IJG sentence that `jpeg-encoder` needs into `licenses/IJG-notice.txt`.
- [ ] Run the generator per channel, so the store bundle's list matches what the store binary links. The store list has no `lensfun` row and no lens database row (memo section 9).

**You see.**

- [ ] `THIRD_PARTY_LIBRARIES.md` names CDDL-1.0 as LibRaw's elected licence and links the public source. About states the new product licence and links the same URL. `licenses/` holds the three new files.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The generator self-test gains an election case, a per-channel case and a missing-copyright-line case. The `bundled_models` test lists the three new files. Run `python3 scripts/gen_third_party_libraries.py --self-test && python3 scripts/gen_third_party_libraries.py --check && cargo test --lib bundled_models`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Run `settings-about-open` on the debug build at trunk and head. Save `about.png`. Pass when both pass and head shows the new licence sentence and the source link.
- [ ] Lane 2. A local `npm run build:release` bundle, `ls Contents/Resources/licenses`. Save `licenses.txt`. Pass when `LibRaw-LICENSE.CDDL` is present, `README.txt` names it operative, and the three new files are present.
- [ ] Lane 3. Debug build, "Open library licences". Save `libraries-open.png`. Pass when no error line appears.
- [ ] Lane 4. Runner lane, `windows-latest`, `--check`. Save `win-check.txt`. Pass when it exits 0.
- [ ] Lane 5. For every runtime crate that `cargo metadata` resolves for the store build, look for its copyright line in the shipped `licenses/rust-crates.txt`. Save `notice-coverage.txt`. Pass when every MIT, BSD and ISC crate's line is found.
- [ ] Lane 6. `grep` the shipped `licenses/` for the DCB, FBDD and X3F authors and for "Independent JPEG Group". Save `notice-gaps.txt`. Pass when all four are found.
- [ ] Lane 7. `export_open_source.sh` at the head, then `diff -r` of its `libraw/` folder against `vendor/rsraw-sys/LibRaw`. Save `source-export.txt`. Pass when the diff is empty and the folder holds `LICENSE.CDDL` and `COPYRIGHT`.
- [ ] Lane 8. Sandbox build, `mas`, About. Save `mas-about.png`. Pass when the licence sentence and the source link show, and the library list has no `lensfun` row.
- [ ] Lane 9. Throwaway edit of `licenses/LibRaw-LICENSE.CDDL`, then `--check`. Save `drift.txt`. Pass when `--check` exits 1 and names the file.
- [ ] Lane 10. `cat app/package.json | grep license` and `cargo metadata` for the app crate. Save `license-fields.txt`. Pass when both carry the identifier the owner chose.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms`, the one metric a text change could move if the About tab loaded eagerly. Bundle size of `licenses/`.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` and `du -k` of `licenses/`.
- [ ] Baseline. Record the trunk values first.
- [ ] Rule. Fail if `first_grid_ms` exceeds trunk by more than 10 percent, or `licenses/` grows by more than 1 MB (guess, the crate texts are the new bulk).

**Review gate.** None. S5 is not review-gated. It changes text, not an interaction.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] The owner agent squash-merges it once the owner has accepted the licence research memo's stated risk, and local checks and every CI check are green.

## Remove the lensfun crate per RFC-0039 (S6, shared, deferred)

**Depends on.** Deferred by this revision. The store edition builds without lens profile correction (S1), so the first store release does not need RFC-0039. When reopened, the D5 answer of the licence research memo applies. Ship the 56 XML files unmodified as resources, with the credit line, the licence URI and an "unmodified" statement, and leave the XSD and DTD out until upstream states their licence (memo section 9). S6 is out of every wave and every cost total until the owner reopens it, for example to bring lens profiles to a later store release. When reopened, it depends on RFC-0039 steps 1 to 5, each its own PR under RFC-0039's build order, then S5 for the generator. It is gated on the D5 research memo and RFC-0039 Q1 to Q7. Cost 6.5 to 12 engineer-days for all of RFC-0039, up to about 14 (sourced, RFC-0039 section 7). This section is RFC-0039 step 6.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] The owner agent squash-merges it once local checks and every CI check are green.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] The owner agent squash-merges it once local checks and every CI check are green.

## Keep online maps and place search off in release builds until the user turns them on (S9, shared)

**Depends on.** S0 for the lanes. D17 is answered (owner message 2026-11-18, paraphrased). Cost 1 to 2 engineer-days (guess), down from 2 to 4, because the face-consent half left with the face features (S3). Microsoft requires opt-in before personal data goes to a third party (sourced, policy 10.5.2). Apple asks for a privacy policy that names what leaves the device (sourced, guideline 5.1.1).

**Files.**

- [ ] Edit `app/src-tauri/src/geocode.rs`, the map settings in `app/src-tauri/src/catalog/geo.rs`, `app/src/lib/mapTiles.js` and `LibraryMapView.svelte`.
- [ ] Edit the Map section of `SettingsDialog.svelte` and `docs/USER_GUIDE.md`.

**Build.**

- [ ] New setting `online_map_enabled`. When the catalog has no stored value, its default is `cfg!(debug_assertions)`. Release builds start with it off, and debug builds start with it on, so development does not change.
- [ ] While it is off, the map shows a card that turns it on, and `geocode::search` and `reverse` refuse without a network request.
- [ ] A stored value always wins over the default, so a user's choice survives an upgrade.

**You see.**

- [ ] A fresh release install opens the map with the card and makes no network request until the user turns it on. A debug build draws tiles at once, as on trunk.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A `geocode` test for the refusal and a settings test for the default and the stored value. Vitest for the card state. Run `cargo test geocode geo && cargo test --release geo && npx vitest run`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Open the map on a fresh catalog in a release build at trunk and head. Save `map-first.png`. Pass when trunk draws tiles at once and head shows the card.
- [ ] Lane 2. Head release build, turn the map on. Save `map-on.png`. Pass when tiles draw.
- [ ] Lane 3. Head release build, place search while off, through the command. Save `search-off.txt`. Pass when it returns the refusal and the request log is empty.
- [ ] Lane 4. Head debug build, fresh catalog, open the map. Save `debug-map.png`. Pass when tiles draw at once, as on trunk.
- [ ] Lane 5. Head release build, turn the map on, then place search for "Singapore". Save `search-on.png`. Pass when results show.
- [ ] Lane 6. Head release build, turn the map on and then off again. Save `map-off-again.png`. Pass when the card returns and no tile request follows.
- [ ] Lane 7. Head release build, quit and relaunch after turning the map on. Save `map-persist.png`. Pass when the map stays on.
- [ ] Lane 8. Sandbox build, `mas`, `/usr/bin/log show` and `lsof -i` for the app during its first minute. Save `no-network.txt`. Pass when no outbound connection appears.
- [ ] Lane 9. Runner lane, `windows-latest`, lanes 1 and 2. Save `win-map.png`. Pass when both match the Mac lanes.
- [ ] Lane 10. A catalog copy from a trunk build that already holds a custom tile server, opened by the head release build. Save `custom-tiles.png`. Pass when the card shows and, after turning it on, the custom server's tiles draw.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms`. The setting adds a read at startup.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms`.
- [ ] Baseline. Record the trunk median first.
- [ ] Rule. Fail if the head median exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 1, lane 2 and lane 4 screenshots into `/tmp/store-review/S9-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the change on a lane job. Save it as `/tmp/store-review/S9-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator reviews and lands it.

## Ship the EULA and privacy policy in the app (S10, shared)

**Depends on.** S5 and S9. S6 is deferred and drops out. The texts come from the privacy-policy and licence-agreement research memo, which an agent writes from primary sources and which states the risk the owner accepts. No lawyer drafts them. The public privacy policy URL is a GitHub Pages page until the owner has a domain (D20). Cost 1.5 to 3 engineer-days (RFC-0038 B5.9, guess). The Windows installer's licence page is deferred with the Windows PRs. RFC-0038 B5.9 says "first-run acceptance". The Mac store forbids licence screens at launch (sourced, guideline 2.4.5(vi)), so the Mac store build shows none.

**Files.**

- [ ] Create `legal/EULA.md` and `legal/PRIVACY.md` from the research memo's texts, unchanged once the owner accepts them. The EULA carries the open-source clause of the licence research memo (its section 4). It says the open-source licences are not limited, that differing terms come from the seller alone and not from LibRaw LLC, and where the LibRaw and MPL source is. If the store build keeps Apple's Standard EULA, the same clause goes into About and `licenses/README.txt` (memo section 9). Both texts call the product "the app" and never name it, because the chosen product name stays out of the repository until the owner says it is registered (D15). Adding the name later changes the hash that the test records.
- [ ] Create the source of the public privacy page, a copy of `legal/PRIVACY.md` that the owner publishes. GitHub Pages on a private repository needs a paid plan (inferred, to confirm on the owner's plan), and D8 makes the main repository private. So the page is served from the small public source repository of S5, or from a public repository of a neutral GitHub organisation, which keeps a personal handle out of the address (inferred). The owner enables Pages and publishes it, because publishing is the owner's action. When the owner has a domain, the URL moves there, and App Store Connect takes the new URL with a new version (inferred).
- [ ] Edit `app/src-tauri/tauri.conf.json` (`bundle.resources`) and the About tab.

**Build.**

- [ ] About gains "Licence agreement" and "Privacy policy" buttons that open the bundled files through Rust commands.
- [ ] No build shows a licence screen at launch. The Mac store build must not (guideline 2.4.5(vi), sourced), and the default build keeps its trunk behaviour.

**You see.**

- [ ] About opens both texts. The privacy text covers geocoding, map tiles, the D17 default, the purchase check (the app asks Apple's StoreKit whether the purchase is real, sends nothing of its own, and keeps the time of the last success on the device), local face data in builds that have face features, and the absence of model downloads in store builds.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A Rust test asserts both files are bundled and match the SHA-256 of the accepted texts recorded in the test. Run `cargo test --lib bundled_models`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Open About at trunk and head. Save `about.png`. Pass when trunk has two buttons and head has four that all open without error.
- [ ] Lane 2. Sandbox build, `mas`, cold launch. Save `mas-launch.png`. Pass when the Library shows with no licence dialog.
- [ ] Lane 3. Release artifact, `ls Contents/Resources/legal`. Save `legal.txt`. Pass when both files are present with the recorded hashes.
- [ ] Lane 4. Debug build, default channel, About, Licence agreement. Save `direct-eula.png`. Pass when the text opens without an error line.
- [ ] Lane 5. `diff` of the bundled `legal/` files against the texts the owner accepted. Save `legal-diff.txt`. Pass when the diff is empty.
- [ ] Lane 6. `grep` the privacy text for geocoding, tiles, faces and downloads, and the EULA or About for the open-source clause and the source URL. Save `privacy-terms.txt`. Pass when each topic and the clause are found.
- [ ] Lane 7. Sandbox build, `mas`, About, Privacy policy. Save `mas-privacy.png`. Pass when it opens inside the sandbox.
- [ ] Lane 8. Runner lane, `windows-latest`, About, both buttons. Save `win-about.png`. Pass when both open.
- [ ] Lane 9. Debug build, default channel, cold launch. Save `direct-launch.png`. Pass when it behaves as the owner chose for direct builds.
- [ ] Lane 10. The privacy policy URL that G2 will enter, the GitHub Pages page until the owner has a domain, fetched with `curl -sI`. Save `privacy-url.txt`. Pass when it returns 200 and the page text equals `legal/PRIVACY.md`.

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
- [ ] Stop at verified. The operator reviews and lands it.

## Build every channel from a tag in CI (S11, shared)

**Depends on.** S1. No owner gate. Cost 1.5 to 3 engineer-days (RFC-0038 B1 and B2, plus build numbers, guess). CI never builds an installer today (measured, RFC-0038 section 0).

**Files.**

- [ ] Create `.github/workflows/release.yml` as a reusable workflow with inputs `channel`, `edition` and `os`, plus a `push` trigger on `v*` tags.
- [ ] Create `.github/actions/vcpkg-libraw/action.yml` and use it from `ci.yml` and `release.yml`.
- [ ] Create `scripts/release/build-number.sh` and `docs/RELEASING.md`.

**Build.**

- [ ] Each run builds unsigned bundles for one channel and edition, asserts `licenses/` and both notice files are in the bundle, and uploads them as artifacts.
- [ ] `build-number.sh` sets `CFBundleVersion` and the Windows version from the run number, so every upload has a higher number (audit M6).
- [ ] For `channel=mas`, the workflow builds with `--no-default-features --features channel-mas` (S1). Other channels keep the default features.
- [ ] RFC-0038 B2's source bundle is not attached. The licence research memo meets CDDL's source condition with S5's public repository, tagged per release, instead.

**You see.**

- [ ] `gh workflow run release.yml -f channel=mas -f edition=store` produces an `.app` artifact with the `store` edition file.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `build-number.sh --self-test`. Run `scripts/release/build-number.sh --self-test && actionlint`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Trunk has no release workflow, so record that and gate that `ci.yml` stays green at head with the shared vcpkg action. Save `ci-green.txt`. Pass when every `ci.yml` job passes.
- [ ] Lane 2. `release.yml` with `channel=mas`. Save `mas-artifact.txt`. Pass when the `.app` has `Resources/models/bundled-models.edition.json` naming `store`.
- [ ] Lane 3. `release.yml` with `channel=direct` and `os=windows-latest`. Save `win-artifact.txt`. Pass when an `.msi` and an `.exe` are uploaded.
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
- [ ] The owner agent squash-merges it once local checks and every CI check are green.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] Stop at verified. The operator reviews and lands it.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] Stop at verified. The operator reviews and lands it.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] Stop at verified. The operator reviews and lands it.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] Stop at verified. The operator reviews and lands it.

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

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

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
- [ ] Stop at verified. The operator reviews and lands it.

## Sign, package and upload the Mac store build (M6, Mac)

**Depends on.** S11, M2 and M4. M4 is a parent because the review gate exports outside the container. D14 and D18 are answered, so the account is individual, the build is Apple silicon only and the minimum macOS is 13 (owner messages 2026-11-18 and 2026-11-23, paraphrased). Owner-gated on the trademark search and on the bundle ID (D20), which the owner chooses at M6 start after buying the domain. The App ID, the provisioning profile and the App Store Connect record carry the bundle ID, which cannot change once a build is uploaded (sourced, Apple "Changing the bundle identifier"). Owner-gated also on the secrets below. Cost 3 to 5 engineer-days (audit B3, guess). The universal build is dropped with D18.

**Files.**

- [ ] Create `app/src-tauri/tauri.appstore.conf.json` with `bundle.macOS.entitlements`, `minimumSystemVersion` set to 13.0 (D18), the `identifier` set to the D20 bundle ID, the `embedded.provisionprofile` mapping and `ITSAppUsesNonExemptEncryption` (sourced, Tauri App Store guide).
- [ ] Create `app/src-tauri/entitlements/mas-app.entitlements` and `mas-helper.entitlements`.
- [ ] Create `scripts/release/mas-sign-and-package.sh` and `.github/workflows/release-mas.yml`.

**Build.**

- [ ] The owner creates the Apple Distribution and Mac Installer Distribution certificates, the App ID for the D20 bundle ID, the Mac App Store provisioning profile and an App Store Connect API key, then stores them as GitHub secrets. The owner is the Account Holder of an individual membership. Only the Account Holder or an Admin can create distribution certificates (sourced). Evidence is `gh secret list` showing the names.
- [ ] The store build's display name is the chosen product name, and the name is not committed. The release workflow reads it from a GitHub Actions secret and merges it into the Tauri config at build time, until the owner says the name is registered (D15). The binary name stays `emulsion`, so lane paths and `nm` checks hold (inferred, M6 checks `mainBinaryName`). The default `tauri.conf.json` keeps its current identifier, which shows a personal handle and the old product name, and the store build never uses it (measured, the file's `identifier`).
- [ ] Answer audit question 2 first. Run `codesign -d --entitlements :-` on `Contents/MacOS/emulsion-ai` in Tauri's output to see whether the bundler applies its own entitlements to `externalBin`. Record it in the PR.
- [ ] `mas-app.entitlements` holds app sandbox, user-selected read-write, bookmarks app-scope, network client and print, plus the application and team identifiers. `mas-helper.entitlements` holds exactly app sandbox and inherit (sourced, Apple helper-tool page).
- [ ] `release-mas.yml` builds through S11's workflow with `channel=mas`, so the binary has no default features. The script signs the helper, then the app, clears extended attributes, and runs `productbuild --sign`. The workflow runs `altool --validate-app`, and uploads only on a manual approval step.

**You see.**

- [ ] A build appears in App Store Connect and TestFlight. Evidence is the owner's screenshot.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] The script's `--dry-run` prints the signing order for a fixture bundle. Run `scripts/release/mas-sign-and-package.sh --dry-run && actionlint`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Trunk has no Mac store package, so record that and gate that the `release-mas.yml` artifact validates. Save `validate.txt`. Pass when `altool --validate-app` reports no errors.
- [ ] Lane 2. Expand the `.pkg` and run `codesign -d --entitlements :-` on the app. Save `app-ent.txt`. Pass when the set equals `mas-app.entitlements` and has no network server.
- [ ] Lane 3. Same on `emulsion-ai`. Save `helper-ent.txt`. Pass when the set is exactly app sandbox and inherit.
- [ ] Lane 4. `pkgutil --check-signature` on the `.pkg`. Save `pkg-sig.txt`. Pass when it names the Mac Installer Distribution identity.
- [ ] Lane 5. `xattr -lr` on the expanded app. Save `xattr.txt`. Pass when no `com.apple.quarantine` is found (sourced rule since 2025-02-18).
- [ ] Lane 6. `plutil -p` on the app's `Info.plist`. Save `plist.txt`. Pass when `ITSAppUsesNonExemptEncryption` and the category are present, `LSMinimumSystemVersion` is 13.0 (D18), and `CFBundleIdentifier` equals the D20 bundle ID and differs from the identifier in the default config.
- [ ] Lane 7. `lipo -info` on both executables. Save `arch.txt`. Pass when both are `arm64` only.
- [ ] Lane 8. The same bundle re-signed with the Apple Development identity on the owner's Mac, `sandbox-smoke`. Save `dev-signed.png`. Pass when the spec passes.
- [ ] Lane 9. `nm` on the packaged `emulsion`. Save `nm.txt`. Pass when no `export_plugin` or `lensfun` symbol and no download host string are found.
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
- [ ] Stop at verified. The operator reviews and lands it.

## Offer a trial through In-App Purchase on the Mac (M7, Mac, deferred)

**Depends on.** Skipped by the owner. D12 is paid upfront with no trial (owner message 2026-11-18, paraphrased), so M7 is out of every wave and every cost total. If the owner reopens a trial, it depends on M6 and M8. Cost 3 to 6 engineer-days (guess). Accounts needed are the In-App Purchase products and a sandbox tester in App Store Connect.

**Files.**

- [ ] Create `app/src-tauri/src/store_purchase.rs` over a StoreKit 2 bridge. `tauri-plugin-iap` needs macOS 13 (sourced, docs.rs), and D18 now sets macOS 13, so it is allowed.
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

## Check the purchase at launch and open a read-only mode when it fails (M8, Mac)

**Depends on.** M6, because the live lanes that only the owner can run need a TestFlight build. D21 is answered (owner message 2026-11-23, paraphrased), so M8 is no longer decision-gated. It also depends on the macOS 13 baseline of D18, on a small Swift static library that the app links (its first box proves the toolchain), and on a second Mac with an Apple ID that never got the app, for lane 10. Cost 2.5 to 5 engineer-days (guess). The 2026-11-18 figure stays, because the PKCS #7 and device-hash work leaves, and the Swift bridge, the purchase state machine and the read-only guard come in (guess). D12 asks for a check that the purchase is real, so a copied app does not run for other people (owner message 2026-11-18, paraphrased). D21 turns "does not run" into "opens read-only". This is the owner's decision. The review risk is medium (inferred, Appendix C).

Why `AppTransaction`, and what it cannot do. These facts come from Apple pages retrieved on 2026-11-23 (Appendix D).

- Apple marks the receipts collection deprecated and says the on-device receipt is not needed when the app uses `AppTransaction` (sourced). `Bundle.appStoreReceiptURL` and `SKReceiptRefreshRequest` are deprecated from macOS 15.0 (sourced), and guideline 2.4.5(viii) forbids deprecated technologies (sourced). So the 2026-11-18 route, which read the receipt file, is dropped (Appendix B).
- `AppTransaction.shared` returns a record of the purchase that the App Store signs, as verified or unverified. It throws when the record is not available or the user is not signed in, and it may need the network (sourced). `AppTransaction.refresh()` shows an App Store sign-in prompt, so it runs only from an explicit user action (sourced).
- Guideline 2.4.5(vi) forbids licence keys and the app's own copy protection (sourced). No Apple page says in words that checking the App Store's own signed purchase record is exempt. Apple documents `AppTransaction` for validating the app download, and guideline 3.1.1 links to receipt validation for content access, so the check is Apple's mechanism and not the developer's own (inferred, high confidence). Appendix C keeps it as a risk.
- Apple's only written instruction for a failed check is "Don't try to terminate the app", and to give the user a way to handle the case (sourced, "Validating receipts on the device"). The exit code 173 idea is dropped for that reason. Its old archive page now redirects to the current page, which does not mention it, and a developer forum thread reports that apps built with the macOS 15 SDK show an alert that the exit(173) API is no longer available (unofficial, no Apple reply).
- The check does not stop a patched binary, because Apple says an app can't rely on its own logic to check itself (sourced, "Establishing your app's integrity"). It also passes Family Sharing and one Apple ID on several Macs, which Apple's terms allow (inferred). The most it does is move a copier from "copy the app" to "patch the binary" (inferred).

What the check does (D21). One Rust enum, `PurchaseState`, holds the answer. The outcomes map to it as follows.

- `AppTransaction.shared` returns verified, its bundle ID equals the app's, its `deviceVerification` equals the value computed from `AppStore.deviceVerificationID` and its nonce, and it has no revocation date. The state is `Full`, and the app stores the time of this success.
- `AppTransaction.shared` throws, which covers offline, signed out, a StoreKit error and no answer within a short timeout (the timeout value is a guess). The state is `Grace`. The app stays fully usable for 30 days from the last success, or from the first launch when none is stored. 30 days is a guess. After that it is read-only, with the reason "could not check".
- The result is unverified, the bundle ID differs, the device value differs or the purchase is revoked. The state is `ReadOnly` at once.
- The code never calls `exit`. "Quit" is always a click.
- The check runs in the main process, on a background thread at launch. The Library opens at once in the state stored by the last launch, and a definite failure switches the running app to read-only without a restart. A write that is already running finishes (inferred design, the `how` run checks it).

Read-only mode, exactly. The guard blocks actions that change photos, edits, files or catalog content. It never blocks looking at, or getting out, the user's own work.

- Allowed. Browsing the Library, search, filter, sort and select. The map, as the D17 switch allows. Compare. Opening a photo to look at it with its stored edits applied, where Develop is a viewer with every control off. Show in Finder and Copy path. Copy originals, described below. Backing up the catalog. Settings, About, Licence agreement, Privacy policy and the library licences. Reconnect for a lost folder (M3), because the photos must stay viewable. Thumbnail and preview generation for photos already in the catalog, which writes only cache files. Verify purchase, Open in the App Store and Quit.
- Blocked. Every edit in Develop, which means sliders, crop, tools, Remove, presets, paste and sync settings, reset, and undo and redo that write history. Every AI action (Denoise, Select Subject, adaptive presets). Export of rendered files, including the "after export, open in {app}" row and Print. Enlarge, HDR merge and panorama merge, because each makes a new file. Every new import, which means Import Files, Import Folder and a drop. Every change to catalog content, which means ratings, flags, labels, keywords, collections and Remove from catalog. Organising counts as editing here, and the owner can loosen that line.
- Never, in any mode. The app does not delete, move, rename or rewrite an original or the catalog, does not hide or migrate data because of the mode, and does not show a licence screen at launch (guideline 2.4.5(vi), sourced). The originals stay accessible in the file system, in the folders where the user keeps them, and the catalog stays readable.
- Copy originals is a new menu item of the store build. It copies the original files and their sidecars byte for byte into a folder the user picks, through M4's destination choice. It is present in every mode, so it is not a special case. It is how "exportable of the originals" is met. The app has no such action today (measured, a search of `app/src` and `app/src-tauri/src` found none).
- Restore. "Verify purchase" runs `AppTransaction.refresh()` and returns the app to full on success, without a restart. "Open in the App Store" opens the app's Mac App Store page, where the user buys the app or gets it again if the account bought it before. The owner calls this the resume payment flow. A paid-upfront app is bought in the App Store and not through StoreKit inside the app (inferred), and the page address needs the app's numeric Apple ID, which exists only once the App Store Connect record does (D20).
- The interface is a banner above the Library with the three buttons, not a modal window and not a screen before the Library. That is the closest reading of Apple's wording that avoids a licence screen at launch (inferred).

**Files.**

- [ ] Create the Swift package `app/src-tauri/swift/PurchaseCheck/` (a static library with a few C functions for the status and the refresh, and its tests) and `app/src-tauri/src/purchase/` with `mod.rs` (the state machine, the grace clock and the stored times), `bridge.rs` (the FFI and the scripted backend) and `policy.rs` (the table of commands).
- [ ] Edit `app/src-tauri/Cargo.toml` (feature `mas-purchase-check = ["channel-mas"]` and the bridge dependency), `app/src-tauri/build.rs` (build the Swift package for this feature), `app/src-tauri/src/lib.rs` (managed state, the check at setup, the handler wrapper, and the commands `purchase_status`, `purchase_verify` and `purchase_open_store`), `.github/workflows/release-mas.yml` (turn the feature on) and `docs/RELEASING.md`.
- [ ] Create the read-only banner in the Library frontend, the disabled states of the blocked controls, and the Copy originals menu item.

**Build.**

- [ ] Prove the bridge first, and pick the route with the pstack `how` skill. The candidates are a small Swift package linked through `swift-rs` (the pattern of `tauri-plugin-apple-iap`, sourced from its docs.rs listing) and the `storekit-rs` crate. `objc2` alone cannot call `AppTransaction`, a Swift async API (inferred, high confidence). `storekit-rs` 0.5.2 needs Xcode with the macOS 26.4 SDK or newer to build (sourced, docs.rs), so check that the `macos-latest` runner and the owner's Mac have it before choosing it. It is a young crate with one maintainer (inferred). Any new crate passes the `COPYLEFT_ALLOWED` rule of `gen_third_party_libraries.py`. Record the route, the SDK version and the added build time in the PR. Read the Apple pages for `revocationDate` and `deviceVerification` again and record their availability on macOS 13.
- [ ] Compile the check only into the Mac store package. Only `release-mas.yml` turns on `mas-purchase-check`. The default build, the open source build, debug builds and the lane sandbox builds never contain it, so they are always full and need no Swift toolchain.
- [ ] Put the backend behind a `PurchaseBackend` trait with two implementations, the Swift bridge and a scripted one. The scripted backend compiles only with `wdio-webdriver`, so lane specs can set each outcome. The release package holds neither the scripted backend nor its command, and G1 checks that.
- [ ] Build the state machine of D21 above, with an injected clock. Store the last success time and the first launch time in the container, outside the catalog, so restoring a catalog backup does not move them (guess, M8 chooses the place).
- [ ] Put the guard in one place. Wrap the `generate_handler!` closure so that, in `ReadOnly`, a command whose row in the `policy.rs` table says blocked returns the typed error `read_only` before it runs (inferred, the `how` run weighs this against a guard inside each command). Every registered command has a row, and a test fails when one does not. Frontend plugin permissions that write are listed in the same table. The frontend reads `purchase_status` and shows each blocked control disabled with the sentence "Editing is off until the purchase is verified."
- [ ] Build the banner with Verify purchase, Open in the App Store and Quit, the Copy originals item, and the log line. The log names the outcome class and the time taken, and holds no Apple ID, no device ID and no purchase record text.
- [ ] Keep the helper as it is. The check runs in the main process, the sandboxed helper `emulsion-ai` calls nothing and needs no new entitlement, and the main app needs none for StoreKit (inferred, Apple enables In-App Purchase by default for an explicit App ID).
- [ ] Handle TestFlight. Apple says the receipt is always present in TestFlight on macOS (sourced). What `AppTransaction.shared` returns there, including its environment value, is unproven (Appendix A), so lane 9 records it.
- [ ] Commit no purchase data. Tests use the scripted backend and fixed values.

**You see.**

- [ ] A TestFlight install opens the Library in full mode with no banner. A copy that fails the check opens the Library with the banner. Browsing and viewing work, editing, export and import are off, and Verify purchase turns everything back on without a restart.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `purchase` tests run the state machine on the scripted backend with a fake clock. The cases are verified, a throw inside grace, a throw after grace, a throw on a first launch, unverified, a wrong bundle ID, a wrong device value, revoked, a refresh that fixes it and a refresh that fails. A policy test fails on a registered command with no row. A guard test calls a blocked command in `ReadOnly` and expects the `read_only` error and no file change. A Swift test pins the device-verification hash on a fixed vector. Run `cargo test --no-default-features --features mas-purchase-check purchase && swift test --package-path app/src-tauri/swift/PurchaseCheck`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another. Lanes 9 and 10 need a human at the screen, and only the owner can run them.

- [ ] Lane 1. Regression lane against trunk. Trunk has no purchase check, so record that and gate that `golden-path` passes at head on the default build and on a `mas` lane build without `mas-purchase-check`. Save `golden.png`. Pass when both pass and no banner shows.
- [ ] Lane 2. `nm` and `strings` on the default binary and on the `mas` lane binary without the feature. Save `nm-no-check.txt`. Pass when neither holds a purchase-check symbol.
- [ ] Lane 3. `nm` on the `emulsion` inside the `release-mas.yml` package. Save `nm-check.txt`. Pass when the purchase-check symbol is present and the scripted backend and its command are absent.
- [ ] Lane 4. A sandbox build with `mas-purchase-check` and `wdio-webdriver`, scripted outcome verified. Save `full.png`. Pass when no banner shows and one edit works.
- [ ] Lane 5. The lane 4 build with scripted outcome unverified. Save `readonly.png`. Pass when the banner shows, the Library and a photo open, and a slider change, an export and an import are each refused with no file written and no catalog row changed (`sqlite3` on a copy before and after), and `/usr/bin/log show` names the outcome class with no Apple ID, device ID or purchase text.
- [ ] Lane 6. The lane 4 build with scripted outcome throws, on a first launch, then with a stored success 29 days old, then with one 31 days old. Save `grace.txt`. Pass when the first two are full and the third is read-only with the reason "could not check".
- [ ] Lane 7. Scripted outcome unverified, then Verify purchase with the scripted refresh set to verified. Save `restored.png`. Pass when editing works again with no restart.
- [ ] Lane 8. Scripted outcome unverified. Use Show in Finder, Copy originals into a new folder, the catalog backup, Settings and About, then watch the process for 60 seconds, then click Quit. Save `allowed.txt`. Pass when each allowed action works, the copied files have the same SHA-256 as the originals, the originals and the catalog are unchanged, the process stays alive until the click, and it exits after it.
- [ ] Lane 9. Owner-only lane. The owner installs the TestFlight build of the M6 package and launches it. Save `testflight-ok.png`. Pass when the Library opens in full mode, and the outcome class, the environment value and the check time are in the log.
- [ ] Lane 10. Owner-only lane. The owner copies the installed TestFlight app to a second Mac signed in to an Apple ID that never got the app, launches it, then launches it again with the network off. Save `copied-app.png`. Pass when the outcome class of both launches is recorded and, if the class is unverified or mismatch, read-only mode shows. If the class is a throw, the copy is in grace and the owner decides whether to shorten it (Appendix C). If it is verified, record that the check does not stop that copy. If no second Mac is available, record that.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `first_grid_ms` of the default build at trunk and head, which must not move. Diff-added work is the check in the `mas` build, which runs in the background.
- [ ] Probe. `perf-interleave.sh <trunk> <head> first_grid_ms` for the default build, and the timing log line around the check in the lane 4 and lane 9 runs.
- [ ] Baseline. Record the trunk `first_grid_ms` median first.
- [ ] Rule. Fail if the default build's head median exceeds trunk by more than 10 percent. Fail if the first grid in lane 9 comes more than 50 ms after the first grid in lane 4 (absolute budget, guess).

**Review gate.** The operator reviews before merge. A distribution-signed build runs only after TestFlight installs it (inferred), so the owner runs lanes 9 and 10 at the screen. The operator also confirms the read-only lists above, because they are a product choice.

- [ ] Copy lane 5 and lane 9 screenshots into `/tmp/store-review/M8-review-<slug>.png`.
- [ ] The operator records a 30 to 60 second video of the TestFlight launch and of the read-only banner on the lane 5 build. Save it as `/tmp/store-review/M8-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto its base after the verdict, patch-id unchanged.
- [ ] Stop at verified. The operator reviews and lands it.

## Keep Windows data in the local profile (W1, Windows, deferred)

**Depends on.** Deferred by the owner on 2026-11-18 with every Windows PR (owner message 2026-11-18, paraphrased). W1 is out of Wave 0, out of every other wave and out of every cost total until the owner reopens it. When reopened, it depends on nothing. Cost 1 to 2 engineer-days (audit S6, guess). All state goes to `app_data_dir()`, which is the roaming profile on Windows (inferred, audit S6), and multi-GB caches live under it (measured, audit S6).

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

## Build and stage the Windows helper and models (W2, Windows, deferred)

**Depends on.** Deferred by the owner on 2026-11-18 with every Windows PR (owner message 2026-11-18, paraphrased). Out of every wave and every cost total until the owner reopens it. When reopened, it depends on S11 and W1. Cost 1 to 2 engineer-days (audit B6, guess). Only an `aarch64-apple-darwin` helper exists in `app/src-tauri/binaries/` (measured, audit B6).

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

## Sign an offline Windows installer that installs silently (W3, Windows, deferred)

**Depends on.** Deferred by the owner on 2026-11-18 with every Windows PR (owner message 2026-11-18, paraphrased). Out of every wave and every cost total until the owner reopens it. When reopened, it depends on W2, on a Microsoft account under D14, on D19 and on the signing secret. Cost 2 to 4 engineer-days (audit B6, guess), plus certificate calendar time. The installer must be a signed standalone offline `.msi` or `.exe` that installs silently (sourced, Store Policy 10.2.9).

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

## Add offline licence keys for the Windows build (W4, Windows, deferred)

**Depends on.** Deferred by the owner on 2026-11-18 with every Windows PR (owner message 2026-11-18, paraphrased). Out of every wave and every cost total until the owner reopens it. When reopened, it depends on S1, on D9, D10 and D13 route A, and on a Windows store row with an `own_licence` cap in S1's caps table. Cost 3.5 to 7 engineer-days (RFC-0038 B5.8 3 to 6, plus 0.5 to 1 for its own RFC, guess). The Mac store build never carries this code (sourced, guidelines 2.4.5(vi) and 3.1.1).

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

## Take payment through a merchant of record (W5, Windows, deferred)

**Depends on.** Deferred by the owner on 2026-11-18 with every Windows PR (owner message 2026-11-18, paraphrased). Out of every wave and every cost total until the owner reopens it. When reopened, it depends on W4, D10 and D14. Cost 2 to 4 engineer-days (RFC-0038 B5.7, guess). Third-party commerce must name the provider, confirm the transaction and be declared in Partner Center (sourced, Store Policy 10.8.2).

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

## Add a signed updater and a public release host for Windows (W6, Windows, deferred)

**Depends on.** Deferred by the owner on 2026-11-18 with every Windows PR (owner message 2026-11-18, paraphrased). Out of every wave and every cost total until the owner reopens it. When reopened, it depends on W3 and W4, and on a `self_update` cap in S1's caps table. D8 is answered. Cost 3 to 6 engineer-days (RFC-0038 B5.10 2 to 4 plus B5.11 1 to 2, guess). An MSI or EXE listing leaves updates to the app or the installer (sourced, research table 2.1), and the Store needs a versioned HTTPS installer URL that does not change (sourced, Store Policy 10.2.9).

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

**Depends on.** S2, S3, S4, S5, S7, S8, S9, S10, M1, M3, M4, M5 and M8, and through M8 on M6, because the store build must be final. W3 is deferred and drops out. Owner-gated on the owner's written acceptance of the risk each research memo states, recorded in ADR-0011. No lawyer signs off. Cost 1 to 2 engineer-days (guess). It extends RFC-0038 Phase C with store rules.

**Files.**

- [ ] Create `scripts/release/check_store_bundle.py` and its fixtures.
- [ ] Edit `docs/RELEASING.md` and `release-mas.yml` to run the check before upload.

**Build.**

- [ ] The check reads a `.pkg` and fails on any of these. Entitlements differ from the committed files, or include network server. The `mas` binary has an `export_plugin`, `lensfun`, licensing or updater symbol, or the M8 scripted purchase backend, or lacks the M8 purchase check. The edition file includes a family ADR-0011 rejects. `licenses/`, a notice file, `legal/EULA.md` or `legal/PRIVACY.md` is missing. A quarantine attribute is present. A signature is invalid. The build number did not rise.

**You see.**

- [ ] The upload step runs only after the check prints `store bundle OK`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A self-test with one fixture per failure. Run `python3 scripts/release/check_store_bundle.py --self-test`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Trunk has no store check, so record that and gate lane 2. Save `trunk.txt`. Pass when the absence is recorded.
- [ ] Lane 2. The check on M6's real `.pkg`. Save `mas-ok.txt`. Pass when it prints `store bundle OK`.
- [ ] Lane 3. A `.pkg` built without the `mas-purchase-check` feature. Save `no-purchase-check.txt`. Pass when the check fails and names the missing purchase check.
- [ ] Lane 4. The `.pkg` re-signed with network server added. Save `net-server.txt`. Pass when the check fails and names it.
- [ ] Lane 5. A bundle copy with `licenses/` removed. Save `no-licenses.txt`. Pass when the check fails.
- [ ] Lane 6. A `default` edition file in a `mas` bundle. Save `wrong-edition.txt`. Pass when the check fails.
- [ ] Lane 7. A quarantine attribute set on one file. Save `quarantine.txt`. Pass when the check fails.
- [ ] Lane 8. A `.pkg` whose `emulsion` was built with the default features. Save `lensfun-in.txt`. Pass when the check fails and names the `lensfun` symbol.
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
- [ ] The owner agent squash-merges it once local checks and every CI check are green.

## Submit to the Mac App Store (G2, release)

**Depends on.** G1. D12 and D14 are answered (owner message 2026-11-18, paraphrased). Owner-gated on the trademark search and the store-name check, because every listing asset carries the name. D15 is decided, and the name enters the repository only after the owner says it is registered. Cost 1 to 2 engineer-days of agent work (guess), plus App Review time, which Apple does not publish (sourced, research section 6).

**Files.**

- [ ] Create `store/mas/listing.md` (name, subtitle, description, keywords, support URL, review notes). The name field reads `<product name>` in the repository until the owner says the name is registered, and the owner enters the real name in App Store Connect.
- [ ] Create `store/mas/privacy-answers.md`, `store/mas/age-rating-answers.md` and `store/mas/export-compliance.md`.
- [ ] Create `app/e2e/specs/store-screenshots.e2e.js`.

**Build.**

- [ ] The review notes explain the folder grant, the bundled helper, the bundled models and the purchase check at launch. They say that the app never terminates, that a check which cannot finish keeps the full app, and that a failed check opens a read-only mode with Verify purchase. They name the reviewer's path to full mode, because a reviewer who lands in read-only mode may reject under guideline 2.1 (inferred, Appendix C).
- [ ] Privacy answers follow Apple's rule that data processed only on device is not collected (sourced). Map and search answers follow D17 and the provider's retention terms. The privacy URL is the GitHub Pages page of S10 until the owner has a domain.
- [ ] The owner signs the Paid Apps Agreement, the tax form and the bank details, enrols in the Small Business Program (15 percent, sourced), sets the price, enters the texts and submits. Evidence is one screenshot per form.

**You see.**

- [ ] App Store Connect shows the build "Waiting for Review". Evidence is the owner's screenshot.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] A lint that the listing has no forbidden phrase ("license key", "activate", "update from our site"). Run `node scripts/release/lint-listing.mjs store/mas/listing.md`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet medium` at the PR head, per the boot recipe. The owner agent runs them itself, one after another.

- [ ] Lane 1. Regression lane against trunk. Trunk has no listing, so record that and gate lane 2. Save `trunk.txt`. Pass when the absence is recorded.
- [ ] Lane 2. `store-screenshots` on the `mas` sandbox build. Save `library.png`. Pass when the Library screenshot is at the size App Store Connect asks for at upload time.
- [ ] Lane 3. Same for Develop. Save `develop.png`. Pass when the size matches.
- [ ] Lane 4. Same for Export. Save `export.png`. Pass when the size matches.
- [ ] Lane 5. Same for the map with the online map on. Save `map.png`. Pass when tiles and attribution show.
- [ ] Lane 6. Same for Remove with PatchMatch. Save `remove.png`. Pass when the patch shows.
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
- [ ] Stop at verified. The operator reviews and lands it.

## Submit to the Microsoft Store (G3, release, deferred)

**Depends on.** Deferred by the owner on 2026-11-18 with every Windows PR (owner message 2026-11-18, paraphrased). Out of every wave and every cost total until the owner reopens it. When reopened, it depends on G1, W5 and W6, and on D13, a Microsoft account under D14, and D15. Cost 0.5 to 1 engineer-day of agent work (guess), plus certification time.

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
- Everything on Windows. No Windows prototype ran. Silent install, the Windows helper's DLLs (audit question 4) and the Store's payment route for MSI listings are unproven. The Windows PRs are deferred.

The revisions of 2026-11-18 and 2026-11-23 ran no prototype. The first read the code on `main` at `93ee992` for the compile-time gating (measured facts in S1, S3 and S4). The second read Apple's `AppTransaction`, receipt and bundle ID pages and developer forum threads for M8, D20 and D21 (sourced, Appendix D). What those revisions leave unproven.

- Whether `npx tauri build -- --no-default-features --features channel-mas` passes both flags to cargo (inferred). S1's last build box answers it.
- Whether the store binary built without `lens-profiles` is smaller and still renders stored profile edits (inferred from the module headers). S1 lanes 3, 4 and 9 answer it.
- What `AppTransaction.shared` returns for a TestFlight build on the owner's Mac, including its environment value (guess that it is verified). M8 lane 9 answers it.
- What it returns for a copied app on a second Mac signed in to an Apple ID that never got the app. A developer forum report says verified came back with no receipt and while signed out (unofficial). If a never-bought account gets a throw, the copy runs in grace for 30 days. M8 lane 10 answers it, and only the owner can run it.
- Whether a Swift static library builds and links into the Tauri binary through `swift-rs` on the `macos-latest` runner, and whether `storekit-rs` builds without the macOS 26.4 SDK it needs (sourced, docs.rs). M8's first build box answers both.
- Whether a wrapper around the `generate_handler!` closure can guard every command from one place (inferred). M8's design run answers it.
- Whether App Store Connect holds a name for a record with no build, and for how long. Apple's pages state no limit (sourced, absence).

## Appendix B. Alternatives rejected

- **Windows route B, MSIX with Microsoft's commerce.** Tauri builds only EXE and MSI (sourced, Tauri Microsoft Store guide). Microsoft's `winapp` CLI can wrap a Tauri app as MSIX, but its guide stages only the `.exe`, so the helper, models and resources would be staged by hand, and the CLI was in preview per a search summary (sourced, research 2.2). MSIX AppData is redirected into the package and may be deleted on uninstall, taking the catalog with it (guess, audit S6). Route B wins only if Microsoft commerce is required, which Store Policy 10.8.1 says it is not for non-game apps (sourced).
- **One build for all channels.** The Mac store build must not carry licence keys, its own updater or code that runs user programs (sourced, guidelines 2.4.5 and 2.5.2), while the Windows route A build needs licence keys and an updater. One binary cannot satisfy both (sourced, research 3.1). S1's channel flag keeps one codebase with three builds.
- **Selling the Mac version outside the store with Developer ID only.** It skips the sandbox work (M2 to M5, about 9 to 19 days, guess) and keeps the plugin hook. The owner asked for the Mac App Store, so it is not the Mac channel here. It stays possible later as a direct channel through RFC-0038 B5.6, and S1 already makes a `direct` build.
- **Keeping the export-plugin hook in the Mac store build.** A user's own program is denied `process-exec` (measured, spike), and guideline 2.5.2 forbids executing code that changes features (sourced). M5 replaces it with opening exports in a picked app.
- **A Cargo feature per model-driven feature (face grouping, the eye check, AI Upscale, person detection).** It would take Emulsion's own face and upscale code out of the store binary as well as the weights. ADR-0012 rejected a feature per family because it multiplies CI cells and cannot express a family with no file. The open questions attach to the weights and their training data, which the store bundle does not carry. It costs about 1 to 2 more days in S1 (guess). The owner can choose it instead.
- **Keeping lens profiles in the store edition through RFC-0039 (S6).** It keeps a feature Lightroom users expect, and costs 6.5 to 12 engineer-days plus the D5 answer (sourced, RFC-0039 section 7). Deferred, because the owner asked to compile out everything that needs a lawyer's answer.
- **Keeping the eye check with a swapped face detector (the 2026-11-17 D11 default).** It needs the MediaPipe card read and a parity run, 2 to 4 days (sourced, RFC-0038 B5.5). It becomes a later release.
- **Reading the receipt file for the purchase check (the M8 route of 2026-11-18).** It needed PKCS #7 decoding, Apple's root certificate in the binary, the device hash and fixtures from a test root. Apple prefers `AppTransaction` and marks the receipt APIs deprecated from macOS 15 (sourced). The owner raised D18 to macOS 13 on 2026-11-23, which removed the reason for the receipt, so M8 now uses `AppTransaction` through a Swift bridge.
- **Exit code 173 on a failed purchase check.** The owner asked for it on 2026-11-18, and on 2026-11-23 chose the read-only mode instead. Apple says not to terminate the app (sourced). The archive page that described 173 now redirects to a page that does not. A forum thread reports an alert on macOS 15 SDK builds that says the exit(173) API is no longer available (unofficial). Other forum threads report rejections of apps that quit at launch and rejections when the check failed in review (unofficial).
- **A blocking window with "Check again" and "Quit" (the D21 default of 2026-11-18).** It grants no access at all, which is close to terminating, and it fails hard when the check misfires in review or on a buyer's Mac (inferred). The read-only mode keeps the user's work in reach and is closer to Apple's wording.
- **Server-backed attestation (App Attest) for the purchase check.** It needs a server, which the local-first design does not have, and it would add a service to run (sourced for the requirement, inferred for the fit).
- **`tauri-plugin-iap` for the purchase check.** It covers in-app products and purchases, not `AppTransaction` (sourced, docs.rs).
- **A licence key or an online activation for the Mac store build.** Guideline 2.4.5(vi) forbids licence keys and the app's own copy protection (sourced).
- **Bookmarking the custom cache folder on the Mac store build.** The asset protocol scope is fixed to `$APPDATA`, so a moved cache likely fails to serve previews already (inferred, audit S3). The `mas` build turns the feature off instead.
- **Moving existing catalogs into the container.** No installer has gone to anyone outside the owner (inferred, RFC-0038 section 3 says not to ship before Phase B). The owner can restore a backup. Revisit if testers received direct builds.
- **Per-file bookmarks.** They miss sidecars, brackets and panorama frames beside the file (inferred, audit B4) and multiply the number of active grants. M2 uses one grant per import root.
- **The Pictures folder entitlement alone.** It covers only `~/Pictures` (guess, unconfirmed key per research section 6). Photographers keep libraries on external drives (guess).

## Appendix C. Risks

- **S1.** The store edition may leave out features the listing promises. G2 lane 7 checks screenshots against the edition.
- **S1, lens code in the store build.** Compiling out `lens-profiles` removes the crate, its database, the rescale functions of `lens_profile.rs` (RFC-0039 region 1) and the profile path of the render code. One ported kernel stays unless S1 rewrites it, the Newton inverse that Manual distortion calls (measured in the licence research memo). The half-day figure for the rewrite is a guess. A rewrite that drifts from the old kernel changes Manual distortion in every build, so S1 swaps it in only when the tests and a sample-grid comparison agree, and otherwise drops the slider from the store edition. The Vite flag that leaves the profile functions out of the store bundle is untried (inferred). S1 lane 3 checks the bundle.
- **S1, SAM 2 in the store edition.** RFC-0038's appendix rates SAM 2 "no known blocker" with medium confidence, and says the SA-1B licence text was not read (sourced, `model-training-data.md`). The licence research memo keeps SAM 2 and calls this its one AI keep with a residual question (memo section 9). NAFNet is rated no known blocker with high confidence (sourced, both). If the owner reads the compile-time rule strictly, Select Subject leaves the store edition too, which is one manifest line in S1.
- **S5.** The memo's readings are an informed layman's, untested in court, and a rights holder could disagree (memo section 8). Under CDDL a breach can be cured within 30 days of learning of it, and buyers' licences survive (sourced in the memo, CDDL 6.1 and 6.3). The likely failure is a missing notice or source pointer, which an update fixes (inferred in the memo). Whether `rsraw-sys` 0.1.1 changed any LibRaw file against the upstream 0.21.3 tarball was not checked (memo section 1). S5 publishes the tree exactly as built, which covers either case (inferred).
- **S2.** PatchMatch fills large areas worse than LaMa (sourced, RFC-0032 section 9 via RFC-0038 B5.5). S2 lane 10 shows the owner both results.
- **S3, S4.** The store release has no face grouping, no eye check, no AI Upscale and no person preset. Reviews may compare it with the direct build (guess).
- **S8.** A CSP can break a custom tile server or a WebGPU worker. Lanes 2 to 4 cover both.
- **S9.** OpenStreetMap may withdraw access (sourced, `map_endpoints.rs` comment and RFC-0038 B5.12). The D17 default lowers load but does not remove the risk.
- **M2 to M4.** Bookmarks touch import, startup, export, merges, Enlarge and backup, so they are invasive. A grant that fails to resolve hides a whole folder. M3's reconnect flow is the recovery. The real panel path depends on a human at the screen every time.
- **M2.** The sandbox may limit how many security-scoped resources one process holds at once (guess). Folder-level grants keep the count low, and lane 10 tests twenty.
- **M6.** Two Mac builds (direct and store) double signing and test cost. App Review may reject the build for the helper or the network use. The review notes and the privacy answers are the mitigation. Rejection reasons cannot be predicted from the guidelines alone (guess).
- **M6, G2, D15, D20.** The bundle ID cannot change after the first upload, and deleting a record does not free it (sourced, Apple "Changing the bundle identifier"). D20 keeps the product name out of the bundle ID, so a rename after M6 changes listing assets and not the identifier. The App Store Connect record waits for the trademark search. The chosen name has had no trademark register check, because agents could not query the registers (measured, private name research), so a live mark in classes 9, 41 or 42 is still possible. Store search results for the word will mix in unrelated apps (inferred). A name that is free in a public search may still be refused by App Store Connect (inferred).
- **D20, no domain.** Until the owner buys one, the bundle ID has no neutral prefix. A prefix on someone else's domain may collide and suggests an affiliation (inferred), so the plan waits for the owner's own domain. A GitHub Pages address for the privacy policy may show a personal handle unless it is served from a neutral organisation (inferred).
- **M8, guideline 2.4.5(vi).** No Apple page says in words that checking the App Store's own signed purchase record is outside "own copy protection". That it is Apple's mechanism is inferred from Apple documenting `AppTransaction` and receipt validation for this purpose (Appendix D). If App Review objects, M8's check is removed from the store build and the owner decides again under D12.
- **M8, review risk, medium (inferred).** A reviewer whose environment makes the check fail definitely would land in read-only mode and may reject under guideline 2.1, as forum reports of rejected checks suggest (unofficial). The grace period for a throw, the banner in place of a modal window, and the review notes of G2 are the mitigations. The owner chose this knowing the risk (owner message 2026-11-23, paraphrased).
- **M8, a paying buyer locked out.** `AppTransaction.shared` has misbehaved on real macOS releases (unofficial, forum reports of a parse error on macOS 15.4 and of verified results where none was expected). A definite false failure puts a buyer in read-only mode. The mode never deletes or hides data, the originals stay in the file system, Copy originals and the catalog backup work, and Verify purchase and the App Store page offer ways back. A buyer who stays offline for more than the grace period also lands in read-only mode, which matters for a local-first app used on location. The 30 days are a guess, and the owner may lengthen them. StoreKit keeps its record up to date and may answer offline from its cache (sourced for the upkeep, inferred for the offline answer).
- **M8, a copy gets the grace period.** If a never-bought account on a second Mac gets a throw and not an unverified result, the copy runs fully for 30 days from its first launch. Lane 10 measures which one happens. If it throws, the owner decides whether to shorten the grace period or treat a signed-in throw as a failure.
- **M8, what the check cannot do.** It does not stop a patched binary, Family Sharing or one Apple ID on several Macs (sourced for the first, inferred for the others). It raises the bar from copying the app to patching it.
- **M8, the Swift toolchain.** The store build gains a Swift static library and a build step that needs a macOS runner with a recent Xcode. `storekit-rs` needs the macOS 26.4 SDK (sourced, docs.rs), so the choice of bridge depends on what the runner and the owner's Mac have. M8's first box answers it.
- **M8, the guard.** Blocking by command name misses a write path that has no command, such as a frontend plugin call. M8 lists plugin permissions in the policy table, and the guard test and lane 5 look for a write that slips through. A patched binary bypasses the guard in any case.
- **Every Windows lane in an active PR.** No control skill exists for the Windows app. Lanes drive it through `lanes.yml` with WebDriver specs and PowerShell, and screen captures may fail on a runner without a desktop session (guess). These lanes check that the open source build still works on Windows. They do not ship anything to the Microsoft Store.
- **Deferred PRs.** W1 to W6 and G3 carry their 2026-11-17 risks unchanged. Certificate cost and SmartScreen reputation are unknown (sourced, RFC-0038 B5.6), and the Windows payment route is unconfirmed. They matter again only when the owner reopens the Windows PRs.
- **Every review gate.** Video capture on a runner with `screencapture` is untested (guess). The owner's Mac is the fallback.

Questions a lawyer would still need to answer. There is no lawyer (owner message 2026-11-18, paraphrased). An agent writes a research memo from primary sources for each question, and the memo states the risk the owner accepts. A memo does not answer a question in law. This list keeps the record of what a lawyer would still be asked.

The licence research memo of 2026-11-18 names three questions that most need a lawyer (its section 10).

1. May future versions of Emulsion be sold closed, given the public MIT history and the AI-assisted commits (RFC-0038 D8 and question 8)? No compile-time switch avoids it. S5 states the risk, and the owner accepts it or asks a lawyer.
2. Do non-commercial dataset terms reach model weights that a third party licensed as Apache-2.0, MIT or BSD (RFC-0038 question 7)? The store edition avoids it for LaMa, Real-ESRGAN, D-FINE, YuNet and SFace by leaving them out. SAM 2 stays with the residual question above. The answer decides which AI features can return.
3. Are the lensfun-derived kernels in the app's own files LGPL-3.0 code, and does a rewrite that the math dictates still derive from the port (RFC-0039 Q4 and Q5)? The store edition avoids it by compiling out the profile path and rewriting the one Manual distortion kernel (S1). The answer decides how lens profiles return.

What the memo settles by reading the primary texts, or the switches avoid, for the first store release.

| Question | Where it is written | Outcome for the first store release |
| --- | --- | --- |
| LibRaw under LGPL-2.1 or CDDL-1.0, and the relink question | RFC-0038 D2, D3, B5.4 and question 2 | Settled by reading (memo sections 2 and 3). Elect CDDL-1.0, publish LibRaw's source (S5). |
| Whether a source link is enough | RFC-0038 question 3 | Settled by reading for CDDL and MPL-2.0 (memo sections 2 and 7). A public repository tagged per release, linked from About (S5). |
| Whether the shipped notices are enough | RFC-0038 question 6 | The memo found three gaps. S5 closes them. |
| Whether Apple's Standard EULA works with these licences | This plan, S10 | Read in the memo (section 4). A custom EULA, or About and `licenses/README.txt`, carries the open-source clause (S10). |
| Whether `bindings.rs` is CDDL Covered Software | Memo section 2 | Avoided by publishing it (S5). |
| The CC-BY-SA lens database, LGPL-3.0 relinking for `lensfun` and Installation Information | RFC-0038 D5, D6, questions 1, 4 and 5, RFC-0039 Q1 to Q3, Q6 and Q7 | Avoided. The crate and the database are not in the store binary. The memo reads Installation Information as not triggered by a software sale (section 5). |
| Biometric duties for face embeddings | RFC-0038 question 9, D11 | Avoided. The store edition has no face features. |
| California venue in LibRaw's CDDL notice | Memo section 8 | Open, low stakes (guess in the memo). |
| Licence keys, payment and updater terms on Windows | D9, D13, D19 | Deferred with the Windows PRs. |

## Appendix D. Links and reading list

Read before editing. RFC-0038 sections 1, 3 (B5) and 4, its appendix `model-training-data.md`, RFC-0039 sections 2, 3, 5, 7, 9 and 10, ADR-0012, `app/src-tauri/src/editions.rs`, the evidence in `docs/store-release-plan-appendix/`, the licence research memo of 2026-11-18 once it is committed, and `.claude/skills/verify-emulsion/SKILL.md`.

Run the pstack `how` skill before S1, M2 and M8, because S1 and M2 cross many call sites and M8 picks the Swift bridge and the design of the read-only guard. Run the pstack `interrogate` skill on M2's and M8's designs before building. W4's RFC-0040 waits with the Windows PRs. Keep the decision trail per the pstack `show-me-your-work` skill in the root agent's store, and commit it with G1 because a store submission needs an auditable record.

The revision of 2026-11-18 retrieved these Apple pages for M8, D20 and D21. Each was retrieved on 2026-11-18. The revision of 2026-11-23 re-read them and added the pages in the next list.

- Validating receipts on the device, https://developer.apple.com/documentation/appstorereceipts/validating-receipts-on-the-device, no date shown.
- Choosing a receipt validation technique, https://developer.apple.com/documentation/storekit/choosing-a-receipt-validation-technique, no date shown.
- App Store Receipts, https://developer.apple.com/documentation/appstorereceipts, no date shown. It marks the `verifyReceipt` endpoint deprecated.
- `AppTransaction`, https://developer.apple.com/documentation/storekit/apptransaction, availability macOS 13.0 and later.
- `SKReceiptRefreshRequest`, https://developer.apple.com/documentation/storekit/skreceiptrefreshrequest, availability macOS 10.9 to 15.0.
- `Bundle.appStoreReceiptURL`, https://developer.apple.com/documentation/foundation/bundle/appstorereceipturl, availability macOS 10.7 to 15.0.
- Changing the bundle identifier, https://developer.apple.com/documentation/xcode/changing-the-bundle-identifier, no date shown.
- App Review Guidelines section 2.4.5, https://developer.apple.com/app-store/review/guidelines/, last updated 2026-10-16, re-read for items (vi) and (viii).

The revision of 2026-11-23 retrieved these pages for D20 and D21. Each was retrieved on 2026-11-23. Apple documentation pages were read through their DocC JSON, which carries the page text.

- `AppTransaction.shared`, https://developer.apple.com/documentation/storekit/apptransaction/shared, macOS 13.0 and later.
- `AppTransaction.refresh()`, https://developer.apple.com/documentation/storekit/apptransaction/refresh().
- `AppTransaction.deviceVerification`, https://developer.apple.com/documentation/storekit/apptransaction/deviceverification, and `AppStore.deviceVerificationID`, https://developer.apple.com/documentation/storekit/appstore/deviceverificationid.
- `AppTransaction.revocationDate`, https://developer.apple.com/documentation/storekit/apptransaction/revocationdate. Its availability on macOS 13 is for M8 to confirm.
- Supporting business model changes by using the app transaction, https://developer.apple.com/documentation/storekit/supporting-business-model-changes-by-using-the-app-transaction.
- TN3138, handling App Store receipt signing certificate changes, https://developer.apple.com/documentation/technotes/tn3138-handling-app-store-receipt-signing-certificate-changes.
- Establishing your app's integrity, https://developer.apple.com/documentation/devicecheck/establishing-your-app-s-integrity.
- `CFBundleIdentifier`, https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleidentifier.
- Preparing your app for distribution, https://developer.apple.com/documentation/xcode/preparing-your-app-for-distribution.
- Register an App ID, https://developer.apple.com/help/account/identifiers/register-an-app-id.
- Add a new app, https://developer.apple.com/help/app-store-connect/create-an-app-record/add-a-new-app.
- Remove an app, https://developer.apple.com/help/app-store-connect/create-an-app-record/remove-an-app.
- App information, https://developer.apple.com/help/app-store-connect/reference/app-information/app-information.
- `storekit-rs`, https://docs.rs/crate/storekit-rs/latest, and `tauri-plugin-iap`, https://docs.rs/crate/tauri-plugin-iap/latest. Third party.
- Developer forum threads, https://developer.apple.com/forums/thread/766399 (exit code 173 alert), 133948 and 727936 and 774033 (review rejections), 764537 (a three-outcome wrapper by Apple DTS), 779028 (a parse error on macOS 15.4) and 799311 (verified with no receipt), all at the same path. Unofficial, and none has an Apple staff answer that settles it.

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

Name and trademark. The name research of 2026-11-18 and 2026-11-19 is held privately by the owner, and its text is not copied here.

- USPTO clearance search guidance, https://www.uspto.gov/trademarks/search/comprehensive-clearance-search-similar-trademarks, via a search summary. The research also names tmsearch.uspto.gov, the WIPO Global Brand Database and EUIPO TMview without a fetched page.
- PetaPixel on the earlier Emulsion app, https://petapixel.com/2015/06/22/hands-on-emulsion-for-mac-is-a-simpler-alternative-to-adobe-lightroom, secondary.
- Softpedia listing of Emulsion 1.4.1, https://mac.softpedia.com/get/Graphics/Emulsion-The-Escapers.shtml, secondary.

## Appendix E. Decisions as recommended on 2026-11-17

This is the decision table of the plan as written on 2026-11-17, kept unchanged as the record the owner answered on 2026-11-18. Its "Who" column names counsel. There is no lawyer, so research memos replace counsel (Arm the program), and Appendix C lists what a lawyer would still be asked. The sources it cites are in Appendix D. The answers of 2026-11-23 for D15, D18, D20 and D21 are in the table in Arm the program.

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

