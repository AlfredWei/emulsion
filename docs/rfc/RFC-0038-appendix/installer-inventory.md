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

Not measurable here. The CI job `Rust core (build + test) (windows-latest)` now has an information-only step, "Record what the
vcpkg LibRaw is made of", that prints the installed version, `vcpkg depend-info`, the port's files, the lines of its
`portfile.cmake` that name the source archive, patches and hash, and the libraries it installs. The results are recorded below
from that log.

(Windows results: see the next section once filled in from the run log.)

## What is not covered

- A packaged installer on either platform (the `.app`/`.dmg`, the Windows `.msi`/`.exe`): `tauri build` was not run.
- Linux (not a target, PRD section 5).
- `lipo`/universal builds, signing and notarisation.
- Whether the macOS binary embeds the 5 MB lens database as bytes or as a compressed blob (strings only found the parser).
