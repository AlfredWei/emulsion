# The public source repository: the owner's checklist

This page is for the owner. It lists what the owner does, in order, to publish the source that the LibRaw and MPL-2.0
licences ask for once this repository goes private (store plan D8 and S5). Agents prepared everything here. Every step
below publishes something, changes who can see a repository, or accepts a risk, so only the owner runs it. Nothing here
is legal advice.

## Why the repository exists

Emulsion links LibRaw and uses it under the CDDL-1.0 (the licence research memo of 2026-11-18). CDDL section 3.1 asks
that LibRaw's source be available to everyone who receives the app, and that the app says where. MPL-2.0 section 3.2(a)
asks the same for the five MPL-2.0 crates. This repository goes private, so the source needs a small public home.
`app/src/lib/openSource.js` holds its address. About, `LICENSE`, `licenses/README.txt` and `THIRD_PARTY_LIBRARIES.md`
show that address. `scripts/release/export_open_source.sh` writes its contents for one release tag.

What the public repository holds:

| Folder | Contents | Licence |
| --- | --- | --- |
| `libraw/` | `app/src-tauri/vendor/rsraw-sys/LibRaw` at the tag, unchanged, with `COPYRIGHT`, `LICENSE.CDDL` and `LICENSE.LGPL` | CDDL-1.0 (elected) |
| `rsraw-sys/` | The wrapper crate's `build.rs`, `Cargo.toml`, `src/` (with the generated `bindings.rs`), `PATCH.md`, `README.md` | MIT |
| `rsraw/` | The patched wrapper crate | MIT |
| `mpl/` | `cssparser`, `dtoa-short`, `dyn-eq`, `option-ext` and `selectors` at the versions `Cargo.lock` pins | MPL-2.0 |
| `README.md` | Which release the source belongs to, the CDDL election, and the crates.io links | |

Nothing of Emulsion's own code goes there.

## Steps, in order

1. **Accept the risk, or ask a lawyer.** Read the risk paragraph in the S5 pull request. Accept it in writing, for
   example as a comment on that pull request, or ask a lawyer first. S5 does not merge before this.

2. **Choose the address.** The default is `https://github.com/AlfredWei/emulsion-open-source`. To use another account or
   organisation, change the one line in `app/src/lib/openSource.js` and the address in `LICENSE`, then regenerate and
   commit:

   ```sh
   python3 scripts/gen_third_party_libraries.py
   python3 scripts/gen_third_party_libraries.py --check
   ```

   The generator refuses to run while `LICENSE` names a different address from `openSource.js`.

3. **Create the empty public repository.**

   ```sh
   gh repo create AlfredWei/emulsion-open-source --public \
     --description "Source of the open-source components in Emulsion releases (LibRaw under CDDL-1.0, MPL-2.0 crates)"
   ```

4. **Merge S5** once CI is green and step 1 is done.

5. **Publish the source for each release, before the build goes to App Store Connect.** From a clean checkout of this
   repository at the release tag:

   ```sh
   git checkout <release-tag>
   git clone https://github.com/AlfredWei/emulsion-open-source ../emulsion-open-source
   scripts/release/export_open_source.sh <release-tag> ../emulsion-open-source
   cd ../emulsion-open-source
   git add -A
   git commit -m "Source for Emulsion <release-tag>"
   git tag <release-tag>
   git push origin HEAD --tags
   ```

   The script stops when the checkout's vendored crates or `Cargo.lock` differ from the tag. A release that changes none
   of these files still gets its own tag, so every version a buyer has points to a tag.

6. **Check it from outside.** Sign out of GitHub, or use a private browser window, and open the address. Then:

   ```sh
   git ls-remote --tags https://github.com/AlfredWei/emulsion-open-source <release-tag>
   ```

   The tag must be listed. The store gate (G1) checks the same before upload.

7. **Make this repository private** (D8), any time after step 3 and before the first store upload:

   ```sh
   gh repo edit AlfredWei/emulsion --visibility private --accept-visibility-change-consequences
   ```

   Copies that others made while it was public keep the MIT licence. Making it private does not change that.

## Later

- The Microsoft Store build links LibRaw from vcpkg, not from `vendor/`. When the Windows PRs reopen, add a `windows/`
  folder with the vcpkg LibRaw port at the pinned commit and the tarball it fetches (the memo's recommendation).
- A store build compiles `lensfun` out (S1). Its licence list comes from
  `python3 scripts/gen_third_party_libraries.py --channel mas --out <dir>`, which the store build copies into the bundle
  in place of the committed `THIRD_PARTY_LIBRARIES.md` and `licenses/`.
