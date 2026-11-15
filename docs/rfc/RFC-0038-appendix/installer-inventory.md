# RFC-0038 appendix: what an installer really contains (A1)

Measured 2026-11-15 on the commit this file was committed with. It records which binary carries which library and how, so the
notices and the questions for counsel (RFC-0038 section 4, questions 1 to 5) rest on a measurement rather than a belief. It
states what was observed; it draws no conclusion about what any licence requires.

## macOS (arm64), `cargo build --release`

Built here with `cargo build --release` in `app/src-tauri` (the two binaries `emulsion` and `emulsion-ai`, then the library
targets). **Not** a packaged installer: no `tauri build`, no `.app`, no signing. The `bundle/macos/Emulsion.app` in `target/`
is from 2026-11-01 and was not used.

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

`cargo tauri build` is the intended way to make an installer and was not run for this inventory; whether it emits or copies
the static library into the bundle was **not checked**. Release packaging should publish only the installer bundles (RFC-0038 A1).

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
| Libraries the port installs | `raw_r.lib` (LibRaw) and, besides it, **`jasper.lib`, `jpeg.lib`, `turbojpeg.lib`, `lcms2.lib`, `spng_static.lib`, `zs.lib`** |

The extra static libraries come from the port's dependencies. This is what each declares in its own vcpkg port:

| Library | Version | Declared licence | Copyright holder named in its `share/<port>/copyright` |
| --- | --- | --- | --- |
| jasper | 4.2.9 | none in `vcpkg.json` (`"license": null`); the copyright file is "JasPer License Version 2.0" | Michael David Adams (2001-2016); Image Power, Inc. and The University of British Columbia (1999-2000) |
| lcms (Little CMS) | 2.19.1 | MIT | Marti Maria Saguer |
| libjpeg-turbo | 3.2.0 | `BSD-3-Clause AND IJG` | (IJG and libjpeg-turbo notices in one file) |
| libspng | 0.7.4 | `BSD-2-Clause AND libpng-2.0` | Randy |
| zlib | 1.3.2 | `Zlib` | Jean-loup Gailly and Mark Adler |

Two further vcpkg dependency lines of lcms (`GPL-3.0-or-later`) appeared in the grep of its `vcpkg.json`. They sit on lines that
look like feature or dependency entries and were **not examined**; whether any GPL-licensed code reaches the link is open until they are.
The build-only ports (`vcpkg-cmake`, `vcpkg-cmake-config`, `vcpkg-cmake-get-vars`, `vcpkg-tool-meson`) install no library.

None of the five libraries is named in `THIRD_PARTY_LIBRARIES.md` or `THIRD_PARTY_NOTICES.md` before this change; the generator now
lists them in a Windows-only row, with their texts still to be copied into `licenses/` once the exact files are chosen
(not done here: the texts in the vcpkg package should be copied from a build, not retyped).

## What is not covered

- A packaged installer on either platform (the `.app`/`.dmg`, the Windows `.msi`/`.exe`): `tauri build` was not run.
- Whether the Windows `emulsion.exe` really contains the five extra libraries (the port installs them; the linked binary was not inspected).
- The two `GPL-3.0-or-later` lines in lcms's `vcpkg.json` (see above).
- Linux (not a target, PRD section 5).
- `lipo`/universal builds, signing and notarisation.
- Whether the macOS binary embeds the 5 MB lens database as bytes or as a compressed blob (strings only found the parser).
