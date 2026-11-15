# ADR-0012: Model editions: an allow-list of model families per build, and a "not included" state

- Status: **Accepted** as a mechanism (RFC-0038 slice 4, "no regret under any D1 answer"). **Which real editions exist is not decided here:** that is RFC-0038 D1, D7 and D11, and ADR-0011 is reserved for those.
- Date: 2026-11-15
- Relates to: [RFC-0038](../rfc/RFC-0038-lgpl-and-database-licence-plan.md) (B5.5, section 5 slice 4), [RFC-0029](../rfc/RFC-0029-ai-upscale.md) (a route to remove a model if a licence reading is challenged), [ADR-0010](ADR-0010-object-removal-engine.md)

## Context

Every build has shipped every model. RFC-0038 found that some models carry training-data terms that may matter for a paid product (`docs/rfc/RFC-0038-appendix/model-training-data.md`), and that whether they do is an open legal question. Whatever is decided, the app needs a way to leave a model out of a build without a code change, and to say so plainly where the feature would be.

## Decision

1. **An edition is a named allow-list of model families** in `models/bundled-models.json` (`editions`). A family is one of `denoise`, `segment`, `detect`, `inpaint`, `upscale`, `blink`, `face`; each bundled file names its family. Anything not listed is excluded, so a model added later never joins a restricted edition by accident. A test makes `default` list every family, so the default build is unchanged.
2. **Two inputs choose the edition, and they can only narrow each other.** The build's name is `EMULSION_EDITION` at compile time (`option_env!`, default `default`); the installer also carries `models/bundled-models.edition.json`, written by `app/scripts/fetch-bundled-models.mjs --edition <name>` (or the same variable) beside the bundled files. The run uses the intersection. So a restricted bundle that loses its file still follows the compiled name, a `--edition` flag without the variable still narrows a default binary, and an unknown name or an unreadable file includes nothing (loud, not permissive). A debug build reads only the variable, because a stale `target/debug/models` can hold an old bundle's file.
3. **Excluded beats cached.** An excluded family is never used, even when a valid copy is on disk from an earlier build: its status is `not_in_edition`, and nothing is downloaded, imported, seeded or run. Cached files are left alone. Whether an upgrade should delete them is an owner and counsel question, not done here.
4. **The user's data is never touched.** Stored removal patches, denoised copies, enlargements and face rows stay readable; only new runs are refused.
5. **The UI says one sentence and no reason:** "{Feature} is not included in this edition of Emulsion." Every frontend check that used to treat "not ready" as "offer the download" goes through `modelGate` (`app/src/lib/modelEdition.js`); an unknown state counts as excluded, never as download.
6. **The fetch script prunes.** A no-LaMa build made after a default build in the same checkout must not ship the 208 MB file, so files of other editions (and their `.part`) are removed from `app/src-tauri/bundled-models/` before anything is fetched, and the edition file is written last, after every file verified.

## Consequences

- Dropping a model from a build is a manifest line plus `EMULSION_EDITION=<name> npm run build:release`. The `release.yml` that RFC-0038 B1 adds needs an `edition` input that sets the variable for the whole job; Phase C should check that the bundle's edition file lists what was meant.
- `THIRD_PARTY_NOTICES.md` is still one file and lists every model; whether a restricted edition needs its own is a question for counsel.
- Face detection and grouping have no bundled file (YuNet and SFace are downloaded); they are a family all the same, and an edition without `face` refuses before any download. The Remove tool has no download at all, so for it "missing" and "not in this edition" differ only in the sentence.
- `faces.embedding` records no model id, so which model produced a stored embedding cannot be told. A restricted edition keeps stored rows readable; whether it must hide or purge them is D11.
- The old `removal_available` command became `removal_model_status` (`ready`, `missing`, `not_in_edition`).

## Alternatives rejected

- **An exclude-list.** A model added later would join every restricted edition quietly: the wrong default for a compliance control.
- **A runtime file only.** If it were lost from a restricted bundle the downloads would reopen. **A compile-time name only.** `--edition` without the variable would bundle one set and compile another.
- **A Cargo feature per family.** It changes the binary, multiplies CI cells and cannot express a family with no file.
- **Hiding the tools.** The existing "not installed" pattern already explains instead of hiding, and "where is Remove?" is a worse support question than a sentence.
- **Deleting cached models on upgrade.** Irreversible and surprising.
- **A remote kill switch.** It conflicts with local-first (PRD section 3).

## Not verified

Windows; a face-excluded build on a catalog that already holds embeddings; the person preset with `detect` excluded; whether export applies a stored removal patch without its model; a release build with the variable set (the packaged `Resources/models/` listing and the app's first run).
