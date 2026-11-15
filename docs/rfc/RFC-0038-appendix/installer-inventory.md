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
vcpkg port installs and declares. The binary itself was inspected in a separate throwaway run, reported in the section "What the
release `emulsion.exe` contains" below.

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
| freeglut | 3.4.0#3 | none (`"license": null`); its copyright file is an MIT-style permission notice, see "The freeglut copyright file" below | Pawel W. Olszta (1999-2000) |

Pulled in by jasper's default `opengl` feature, which is why `freeglut.lib` is linked: `freeglut`, `opengl`, `opengl-registry`
and `egl-registry` (the last two are headers). The installed set is exactly: `egl-registry` 2024-01-25, `freeglut` 3.4.0#3,
`jasper` 4.2.1, `lcms` 2.14, `libjpeg-turbo` 3.0.4, `libraw` 0.21.3, `opengl` 2022-12-04#3, `opengl-registry` 2024-02-10#1,
`zlib` 1.3.1.

The rustc command line of `rsraw-sys` in the Windows log shows the link directives
`-l raw_r -l zlib -l lcms2 -l jasper -l freeglut -l jpeg -l turbojpeg`, so these libraries are asked for at link time, not merely
installed. Whether the release `emulsion.exe` keeps them is answered in "What the release `emulsion.exe` contains" below (measured:
most of them are not in it).

Two lines in lcms's `vcpkg.json` say `GPL-3.0-or-later` in the newer lcms 2.19.1 port that the runner image carried before the
pin. They belong to the **optional features** `fastfloat` and `threaded`. The lcms 2.14 port at the pinned commit has no `features`
block at all (read from `ports/lcms/vcpkg.json` at `cdf98aaa`, 2026-11-16), and only `lcms2.lib` is installed, so there is nothing
GPL-labelled in the pinned lcms port.

**The whole vcpkg `ports/` tree is pinned** to commit `cdf98aaa2411f565eacb3c7566e5e20542ae151a` (before this change only
`ports/libraw` was, and the other libraries followed the `windows-latest` image: lcms 2.19.1, jasper 4.2.9, libjpeg-turbo 3.2.0,
libspng 0.7.4, zlib 1.3.2). Pinning changed what is linked: `libspng` is gone (the pinned libraw does not use it), zlib is now
`zlib.lib` rather than `zs.lib`, and `freeglut` appeared. The versions above are now as stable as that commit, not as the runner
image. One thing is still not pinned: the vcpkg tool binary itself comes from the runner image (the ports tree is older than the
binary and worked with it on this run).

The build-only ports (`vcpkg-cmake`, `vcpkg-cmake-config`, `vcpkg-cmake-get-vars`, `vcpkg-tool-meson`) install no library.

`THIRD_PARTY_NOTICES.md` does not name these libraries; `THIRD_PARTY_LIBRARIES.md` lists them in a Windows-only row, and their
texts are in `licenses/` (next section).

## Licence texts of the Windows libraries (added 2026-11-16, branch `feature/windows-library-licence-texts`)

A throwaway CI step copied `installed/x64-windows-static-md/share/<port>/copyright` of every port in
`scripts/vcpkg_expected_versions.json` and uploaded the nine files as an artifact; they were downloaded with `gh run download` and
read in full. Five are now committed unchanged under `licenses/` as `vcpkg-<port>-copyright.txt`, pinned by SHA-256 in
`VCPKG_TEXTS` of the generator, and a permanent CI step (`cmp`, a few milliseconds) fails if the pinned vcpkg tree installs a
different text. What each file says in its first lines (no conclusion drawn):

| File | Size | Header |
| --- | --- | --- |
| `vcpkg-jasper-copyright.txt` | 2,744 B | "JasPer License Version 2.0"; Michael David Adams (2001-2016), Image Power, Inc. and The University of British Columbia (1999-2000) |
| `vcpkg-lcms-copyright.txt` | 1,083 B | "Little CMS"; Marti Maria Saguer (1998-2020); an MIT-style permission notice |
| `vcpkg-libjpeg-turbo-copyright.txt` | 5,620 B | "libjpeg-turbo Licenses"; says the library is covered by the IJG License and the Modified (3-clause) BSD License; D. R. Commander (2009-2023), Viktor Szathmáry (2015) |
| `vcpkg-zlib-copyright.txt` | 1,002 B | zlib licence text; Jean-loup Gailly and Mark Adler (1995-2022) |
| `vcpkg-freeglut-copyright.txt` | 1,439 B | "Freeglut Copyright"; Pawel W. Olszta (1999-2000); an MIT-style permission notice |

The libjpeg-turbo file points to a `README.ijg` for the IJG licence and says that a binary distribution's documentation must
include the sentence "This software is based in part on the work of the Independent JPEG Group." (clause 2 of the IJG licence, as
its roll-up states it). `README.ijg` itself is not part of the copyright file. That sentence is not in `THIRD_PARTY_NOTICES.md` or
in `licenses/`; it is a question for counsel (RFC-0038 question 2), and the file's own scope statement says its roll-up "is not a
license in and of itself".

Not shipped, with reasons:

- `libraw-copyright` (43,333 B). It is byte-equal, after squashing whitespace, to the three vendored LibRaw files already in
  `licenses/` (`COPYRIGHT`, `LICENSE.LGPL`, `LICENSE.CDDL` concatenated; 41,878 normalised characters on both sides, measured).
- `opengl-copyright` (100 B). It reads "See https://developer.microsoft.com/windows/downloads/windows-10-sdk for the Windows 10 SDK
  license." The `opengl` port installs no library (its manifest has only the `opengl-registry` dependency).
- `opengl-registry-copyright` (147 B). It says the installed files carry different licences, each in a comment at the top of the
  file. A header-only port.
- `egl-registry-copyright` (1,236 B). A Khronos MIT-style notice for `khrplatform.h` and Apache-2.0 for `EGL/*` and `egl.xml`. A header-only port.
  The three registry ports install headers and XML only, and no library of theirs appears in the rsraw-sys link line or in the linker map of the
  release exe (next section). They are build-time inputs of jasper and freeglut, not linked.

### The freeglut copyright file (read in full)

freeglut's port declares `"license": null`. Its copyright file opens "Freeglut code without an explicit copyright is covered by the
following copyright" and gives "Copyright (c) 1999-2000 Pawel W. Olszta. All Rights Reserved." followed by a permission notice
that grants use, copy, modify, merge, publish, distribute, sublicense and/or sell, requires the copyright and permission notice in
all copies or substantial portions, disclaims warranty, and says the name of Pawel W. Olszta shall not be used in advertising or to
promote sale or use without prior written authorisation. It is the text shipped as `licenses/vcpkg-freeglut-copyright.txt`. No
conclusion about what it requires is drawn here.

### Why a GLUT toolkit is installed (finding)

`freeglut` is installed only because the pinned jasper port enables its `opengl` feature by default. The port manifest at
`cdf98aaa` has `"default-features": ["default-features"]`, and that feature depends on `jasper[opengl]` on every platform except
macOS, Windows-on-ARM, UWP and iOS. `jasper[opengl]` depends on `freeglut` and `opengl`. `libraw` asks for
`{"name": "jasper", "default-features": false}`, yet the measured plan for `vcpkg install libraw:x64-windows-static-md` on a clean
tree still lists `jasper[core,default-features,opengl]`. Why vcpkg's classic mode does so was not investigated (guess: classic mode
applies the default features to a dependency regardless).

An install without that feature **is possible**, measured as a dry run on the CI runner with the pinned ports tree
(`vcpkg install --dry-run "jasper[core]:x64-windows-static-md" libraw:x64-windows-static-md`): the plan holds `jasper` (no
features), `lcms`, `libjpeg-turbo`, `libraw`, `zlib` and the two host cmake ports, and no `freeglut`, `opengl`, `opengl-registry`
or `egl-registry`. The build was not changed in this PR (the measured install of `jasper[core]` was not run, only the plan).

## What the release `emulsion.exe` contains (measured 2026-11-16)

Method. A throwaway workflow on this branch (not merged) ran, on `windows-latest` with the pinned vcpkg tree,
`cargo build --release --target x86_64-pc-windows-msvc --bin emulsion` with `RUSTFLAGS=-C link-arg=/MAP:<file>` (Rust 1.99.1,
`link.exe` of MSVC 14.51). That is the cargo release build of the app binary, not `tauri build`, so no installer was made; the
exe is 50,171,904 bytes. A debug build links the same libraries but is not what ships. `/MAP` lists every public symbol that
survived `link.exe`'s `/OPT:REF`, with the library and object member that defines it (`Lib:Object`). I counted symbols per
library and searched for the named entry points. I also counted the strings in the exe. `dumpbin /symbols` is no use here, because
a release Windows exe carries no symbol table (names live in the PDB).

| Library | In the exe? | Evidence (map of the release exe) |
| --- | --- | --- |
| LibRaw (vcpkg `raw_r.lib`) | **Found** | 4,974 symbols from `raw_r:*.obj`; `libraw_init` is defined in `raw_r:libraw_c_api.cpp.obj` |
| zlib (vcpkg `zlib.lib`) | **Found, inflate side only** | 93 symbols from `zlib:`, objects `adler32`, `crc32`, `inffast`, `inflate`, `inftrees`, `uncompr`, `zutil`; `inflate` defined in `zlib:inflate.c.obj`; no `deflate` symbol; the string "inflate 1.3.1 ... Mark Adler" is in the exe once |
| lcms (vcpkg `lcms2.lib`) | **Not found** | 0 symbols from a `lcms2:` object. The lcms code that is in the exe comes from the Rust crate `lcms2-sys` 4.0.7 (feature `static`, its own bundled lcms 2.19 source, MIT, Marti Maria Saguer): 1,995 symbols from `liblcms2_sys-*:*.o`, `cmsOpenProfileFromMem` and `cmsCreateTransform` among them |
| jasper (`jasper.lib`) | **Not found** | 0 symbols named `jas_*` and 0 map lines naming jasper; LibRaw's `redcine_load_raw` (the jasper user) is not in the image either |
| libjpeg-turbo (`jpeg.lib`, `turbojpeg.lib`) | **Not found** | 0 symbols `jpeg_CreateDecompress`, `jpeg_create*`, `jpeg_read*`, `tjInitDecompress`; `lossy_dng_load_raw` is present in LibRaw but no libjpeg symbol is. The JPEG code that is in the exe is the Rust crates `zune-jpeg` and `jpeg-encoder` |
| freeglut (`freeglut.lib`) | **Not found** | 0 symbols `glutInit` or `glut*` |

So `rsraw-sys` asks for seven static libraries on the link line, and the linker kept objects from two of them (LibRaw and zlib).
Whether `jasper.lib`, `jpeg.lib`, `turbojpeg.lib` and `freeglut.lib` were never pulled in or were pulled in and discarded by
`/OPT:REF` was not distinguished; the final image has none of their symbols either way. The statement holds for this build
configuration (same Cargo profile and features, no LTO setting changed). It was not repeated on the exe inside a `tauri build`
installer, and a different dependency set or a LibRaw built with `dng-lossy` would change it.

What this changes for the notice. Naming the five vcpkg libraries in the Windows row is broader than the binary: measured, the exe
holds LibRaw, zlib's inflate code and an lcms that comes from a crate (a different copy and version than the vcpkg one). The files
for jasper, libjpeg-turbo and freeglut are shipped anyway (cheap, and the installer still names them as libraries the Windows
build links, which is what the link line shows); the question whether they are needed belongs to counsel (question 2). The
`lcms2-sys` copy is an MIT crate already in the Rust crate list, its `vendor/LICENSE` is not among the files in `licenses/`.

## What is not covered

- A packaged installer on Windows (`.msi`/`.exe`). The macOS `.app`/`.dmg` was built (see above); it was not signed or notarised, and the `.dmg` was not opened. The Windows exe above is the cargo release build, not the one inside an installer.
- Linux (not a target, PRD section 5).
- `lipo`/universal builds, signing and notarisation.
- Whether the macOS binary embeds the 5 MB lens database as bytes or as a compressed blob (strings only found the parser).
