# RFC-0038 appendix: what an installer really contains (A1)

Measured 2026-11-15 on the commit this file was committed with. It records which binary carries which library and how, so the
notices and the questions for counsel (RFC-0038 section 4, questions 1 to 5) rest on a measurement rather than a belief. It
states what was observed; it draws no conclusion about what any licence requires.

## macOS (arm64), `cargo build --release`

Built here with `cargo build --release` in `app/src-tauri` (the two binaries `emulsion` and `emulsion-ai`, then the library
targets). The measurements below are of those binaries, not of a packaged installer. The packaged app is covered in the
section "The packaged macOS app" below (added 2026-11-15, same arm64 Mac, unsigned).

| Binary | LibRaw | `lensfun` crate and its database | Dynamic libraries it loads |
| --- | --- | --- | --- |
| `target/release/emulsion` (54 MB, the app) | **Statically linked.** `libraw_init` is a defined text symbol, 573 symbols contain "libraw"; no LibRaw dylib appears in `otool -L` | **Statically linked.** 122 symbols contain "lensfun"; the database parser's strings (`<lensdatabase>`) are in the binary | Only system libraries: AppKit, WebKit, Security, CoreGraphics, CoreFoundation, CoreVideo, ColorSync, CoreServices, Foundation, ApplicationServices, Carbon, `libc++`, `libiconv`, `libobjc`, `libSystem` |
| `target/release/emulsion-ai` (28 MB, the AI helper process) | **None** (0 symbols) | **None** (0 symbols, no database strings) | `libc++`, `libSystem`, Foundation, CoreFoundation, CoreML, `libobjc` |

So on macOS both LGPL components are in the one executable `emulsion`, and the helper carries neither. LibRaw is the vendored
source compiled by `vendor/rsraw-sys/build.rs` into a static library (`static=raw`), the `lensfun` crate is ordinary Rust linked
statically.

`libraw_version` and the LibRaw version string were not found as a plain string in the binary; the version comes from the
vendored header, 0.21.3 (`LIBRAW_MAJOR/MINOR/PATCH_VERSION`, checked by `scripts/check_libraw_version.py`).

### Other artefacts a plain `cargo build --release` emits

`Cargo.toml` has `crate-type = ["staticlib", "cdylib", "rlib"]` on the library (the Tauri template default), so a plain
`cargo build --release` (not `--bins`) also writes, next to the binaries:

| File | Size | Observed content |
| --- | --- | --- |
| `libemulsion_lib.a` | 173 MB | A static library of the whole app: **contains** `libraw_init` (1 defined symbol) and 362 symbols containing "lensfun" |
| `libemulsion_lib.dylib` | 21 MB | **No** LibRaw or lensfun symbols and no database strings (nothing links them in, since the dylib exports no app code) |
| `libemulsion_lib.rlib` | 36 MB | Rust intermediate |

Release packaging should publish only the installer bundles (RFC-0038 A1). `tauri build` was later run; see the next section for
what it did with these three files.

## The packaged macOS app (measured 2026-11-15)

`npm run build:release` steps run by hand on a clean target directory (the fetch script with `--from` a local cache, then
`prepare-ai-helper.mjs`, then `tauri build` with both bundle configs), default edition, arm64, no signing identity set.
`tauri build` finished and wrote `Emulsion.app` and a 549 MB `Emulsion_0.1.0_aarch64.dmg`.

- **The static library is not copied and not even written.** `target/release/` held no `libemulsion_lib.a`, `.dylib` or `.rlib`
  after `tauri build` (it builds the binaries only), and `find` over `target/release/bundle` found no `.a`, `.dylib` or `.rlib`.
  The bundle directory holds `macos/`, `dmg/` and a `share/create-dmg` helper folder. So the 173 MB library above comes from a
  plain `cargo build --release`, not from the release path.
- **Inside the app.** `Contents/MacOS/` has `emulsion` (54 MB, `libraw_init` defined, one match) and `emulsion-ai` (28 MB, no
  libraw symbol). `otool -L emulsion` lists only `/System` and `/usr/lib` libraries.
- **Resources.** `THIRD_PARTY_NOTICES.md`, `THIRD_PARTY_LIBRARIES.md`, `licenses/` (10 files, byte-identical to the repository's
  `licenses/`, including `README.txt`), and `models/` (the seven model files and `bundled-models.edition.json`).
  `resource_path` and `editions.rs` look at `Resources/<name>` and `Resources/models/`, which are those paths.
  Note that `models/bundled-models.json`, the manifest in the repository, is not bundled; the file the app reads is the
  edition file the fetch script writes.


## Windows (x64, MSVC), the CI runner

Not measurable on this machine, so the CI job `Rust core (build + test) (windows-latest)` has an information-only step,
"Record what the vcpkg LibRaw is made of", and the results below are read from that log (runs of 2026-11-15). It states what the
vcpkg port installs and declares; it does not look inside the linked `emulsion.exe`, which CI does not keep.

| Item | Observed |
| --- | --- |
| Port and triplet | `libraw:x64-windows-static-md`, version 0.21.3 (equal to the vendored header; `scripts/check_libraw_version.py` fails CI if they differ) |
| Licence the port declares | `LGPL-2.1-only OR CDDL-1.0` |
| Port files | `portfile.cmake`, `dependencies.patch`, `fix-install.patch`, `vcpkg-cmake-wrapper.cmake`, `usage` |
| Source archives (`portfile.cmake`) | `LibRaw/LibRaw` (SHA512 `c88d0268…`) and `LibRaw/LibRaw-cmake` (REF `eb98e432…`, SHA512 `63e68a4d…`) |
| Libraries the port installs (pinned tree) | `raw_r.lib` (LibRaw) and, besides it, `jasper.lib`, `jpeg.lib`, `turbojpeg.lib`, `lcms2.lib`, `zlib.lib`, `freeglut.lib` (plus the system `GlU32.Lib` and `OpenGL32.Lib` import libraries) |

The extra static libraries come from the port's dependencies. The versions below are the ones CI installs from the pinned vcpkg
tree (measured on the CI run of branch `chore/vcpkg-registry-pin`, 2026-11-15, `vcpkg list` and the port manifests). The same
versions are recorded in `scripts/vcpkg_expected_versions.json`, and both Windows jobs fail if an installed one differs.

| Library | Version | Declared licence (its port's `vcpkg.json`) | Copyright holder named in its `share/<port>/copyright` |
| --- | --- | --- | --- |
| jasper | 4.2.1 | none (`"license": null`); the copyright file is "JasPer License Version 2.0" | Michael David Adams (2001-2016); Image Power, Inc. and The University of British Columbia (1999-2000) |
| lcms (Little CMS) | 2.14 | MIT | Marti Maria Saguer (1998-2020) |
| libjpeg-turbo | 3.0.4 | `BSD-3-Clause` (the copyright file also carries the IJG licence) | (IJG and libjpeg-turbo notices in one file) |
| zlib | 1.3.1 | `Zlib` | Jean-loup Gailly and Mark Adler |
| freeglut | 3.4.0#3 | none (`"license": null`); its copyright file was not read yet | not read yet |

Pulled in by jasper's default `opengl` feature, which is why `freeglut.lib` is linked: `freeglut`, `opengl`, `opengl-registry`
and `egl-registry` (the last two are headers). The installed set is exactly: `egl-registry` 2024-01-25, `freeglut` 3.4.0#3,
`jasper` 4.2.1, `lcms` 2.14, `libjpeg-turbo` 3.0.4, `libraw` 0.21.3, `opengl` 2022-12-04#3, `opengl-registry` 2024-02-10#1,
`zlib` 1.3.1.

The rustc command line of `rsraw-sys` in the Windows log shows the link directives
`-l raw_r -l zlib -l lcms2 -l jasper -l freeglut -l jpeg -l turbojpeg`, so these libraries are asked for at link time, not merely
installed. Whether the release `emulsion.exe` keeps all of them was not inspected.

Two lines in lcms's `vcpkg.json` say `GPL-3.0-or-later` in the newer lcms 2.19.1 port that the runner image carried before the
pin. They belong to the **optional features** `fastfloat` and `threaded`. At the pinned tree the plan lists `lcms` with no feature
and only `lcms2.lib` is installed, so those features are not built; whether the 2.14 port has the same features was **not checked**.

**The whole vcpkg `ports/` tree is pinned** to commit `cdf98aaa2411f565eacb3c7566e5e20542ae151a` (before this change only
`ports/libraw` was, and the other libraries followed the `windows-latest` image: lcms 2.19.1, jasper 4.2.9, libjpeg-turbo 3.2.0,
libspng 0.7.4, zlib 1.3.2). Pinning changed what is linked: `libspng` is gone (the pinned libraw does not use it), zlib is now
`zlib.lib` rather than `zs.lib`, and `freeglut` appeared. The versions above are now as stable as that commit, not as the runner
image. One thing is still not pinned: the vcpkg tool binary itself comes from the runner image (the ports tree is older than the
binary and worked with it on this run).

The build-only ports (`vcpkg-cmake`, `vcpkg-cmake-config`, `vcpkg-cmake-get-vars`, `vcpkg-tool-meson`) install no library.

None of these libraries is named in `THIRD_PARTY_NOTICES.md` yet; `THIRD_PARTY_LIBRARIES.md` lists them in a Windows-only row, with their texts still to be copied into `licenses/` once the exact files are chosen
(not done here: the texts in the vcpkg package should be copied from a build, not retyped).

## What is not covered

- A packaged installer on Windows (`.msi`/`.exe`). The macOS `.app`/`.dmg` was built (see above); it was not signed or notarised, and the `.dmg` was not opened.
- Whether the release `emulsion.exe` keeps the extra libraries (the debug link line names them; the binary was not inspected).
- Linux (not a target, PRD section 5).
- `lipo`/universal builds, signing and notarisation.
- Whether the macOS binary embeds the 5 MB lens database as bytes or as a compressed blob (strings only found the parser).
