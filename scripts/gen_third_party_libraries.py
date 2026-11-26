#!/usr/bin/env python3
"""Regenerates THIRD_PARTY_LIBRARIES.md: the Rust crates compiled into the app and the npm packages bundled into
its frontend, with the licence each declares. Run from anywhere:  python3 scripts/gen_third_party_libraries.py

Also writes licenses/, the licence texts the installer ships beside this file (see licence_outputs() below).

  python3 scripts/gen_third_party_libraries.py            write THIRD_PARTY_LIBRARIES.md and licenses/
  python3 scripts/gen_third_party_libraries.py --check    write nothing; exit 1 if either differs from what the build would give
  python3 scripts/gen_third_party_libraries.py --self-test  the rules below on made-up crates, no cargo needed
  python3 scripts/gen_third_party_libraries.py --channel mas --out DIR
                                                          write the Mac App Store build's list and licenses/ into DIR

The committed files describe the direct channel (the default Cargo features). A channel is a set of Cargo feature flags
(CHANNELS). Rows and licence files follow the crates that channel links, so the store list has no `lensfun` row and no
lens database row once the store build leaves the crate out. The store build copies DIR's files into its bundle in place
of the committed ones. Before S1 adds the `channel-mas` feature, `--channel mas` fails in cargo.

Either way it refuses a copyleft-only dependency that the conditions table does not name (RFC-0038 A4): a crate or package
whose licence expression has no permissive alternative (`MIT OR Apache-2.0 OR LGPL-2.1-or-later` is fine; `MPL-2.0` and
`LGPL-3.0-or-later` are not) must be listed in COPYLEFT_ALLOWED, which mirrors the hand-written table.

Rust: `cargo tree -e normal --target all` (what the app links on any platform; build scripts, dev dependencies and
proc-macros do not ship). npm: the non-dev entries of app/package-lock.json (what Vite bundles).
The hand-written parts of the output are the constants below; edit them here, not in the output."""
import difflib, hashlib, json, os, re, subprocess, sys, unittest
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# CI sets CARGO_TERM_COLOR=always, which makes `cargo tree` wrap its "(*)" repeat markers in colour codes and breaks the
# parsing below (found by this script's own --check on the first Linux run). Parsed output must never be coloured.
NO_COLOR_ENV = {**os.environ, "CARGO_TERM_COLOR": "never", "NO_COLOR": "1"}
CHANNELS = {"direct": [], "mas": ["--no-default-features", "--features", "channel-mas"]}
OPEN_SOURCE_JS = ROOT / "app" / "src" / "lib" / "openSource.js"
LIBRAW = Path("app") / "src-tauri" / "vendor" / "rsraw-sys" / "LibRaw"
# The one text committed by hand: the Creative Commons legal code for the Lensfun database, downloaded once from
# https://creativecommons.org/licenses/by-sa/3.0/legalcode.txt (22,240 bytes, 2026-11-15). The generator checks it, never fetches it.
CC_BY_SA_SHA256 = "3f941b3b89cf7b8370ceb83cc76d2120d471b58735d8ca60238a751a48d7f72f"
# The other hand-added texts: the `share/<port>/copyright` files of the vcpkg ports whose libraries the Windows build links,
# copied byte for byte from the CI runner's installed tree (vcpkg commit cdf98aaa). Pinned by hash, never fetched. CI's
# "vcpkg licence texts are the shipped ones" step compares them with the files the runner installs.
VCPKG_TEXTS = {
    "vcpkg-jasper-copyright.txt": "4ad1bb42aff888c4403d792e6e2c5f1716d6c279fea70b296333c9d577d30b81",
    "vcpkg-lcms-copyright.txt": "46d4e05af3bce75332a12d01f19bcce7e32b6bac544be28a64043d0231291e31",
    "vcpkg-libjpeg-turbo-copyright.txt": "e548c96c0c09397c94b5c5531836c02ff2c403f1f2b130cad8e3ec9439b5a7cc",
    "vcpkg-zlib-copyright.txt": "845efc77857d485d91fb3e0b884aaa929368c717ae8186b66fe1ed2495753243",
}
# The BSD notices of LibRaw parts that the vendored build compiles: (source file, first line, last line), copied verbatim.
# The DCB header also covers FBDD, which lives in the same file. LibRaw's COPYRIGHT names both authors.
LIBRAW_BSD_NOTICES = (
    ("src/demosaic/dcb_demosaic.cpp", "/*", "// Luis Sanz Rodr"),
    ("src/x3f/x3f_utils_patched.cpp", "/* Library for accessing X3F Files", "*/"),
)
# IJG licence clause 2, as libjpeg's README states it; a crate whose licence names IJG ships it in IJG-notice.txt.
IJG_SENTENCE = "This software is based in part on the work of the Independent JPEG Group."

INTRO = """# Third-party libraries

The code libraries that Emulsion is built from, and the licence each one declares. The AI models that ship with the
app are covered separately in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Everything below is open source. The
licence texts that carry conditions beyond "keep the notice" are in the `licenses/` folder installed beside this file, and
`licenses/README.txt` says which file is which. `licenses/rust-crates.txt` holds each Rust crate's own licence files,
with their copyright lines. The MIT, Apache-2.0 and BSD texts that most libraries use are also reproduced in
THIRD_PARTY_NOTICES.md.

This file is generated by `scripts/gen_third_party_libraries.py`; do not edit the lists by hand.

## Components with conditions beyond "keep the notice"

| Component | What it is here | Licence | Elected licence | Where its terms are |
| --- | --- | --- | --- | --- |
"""
# One row per component: (the crate whose presence puts the row in the list, or None for always, the row's cells).
CONDITIONS = (
    (None, "LibRaw 0.21.3", "Decodes RAW files; compiled into the app as a static library (`app/src-tauri/vendor/rsraw-sys`, patched, see its `PATCH.md`; on Windows a vcpkg build of the same library)",
     "LGPL-2.1 **or** CDDL-1.0, at the user's choice; copyright LibRaw LLC",
     "CDDL-1.0. Emulsion uses LibRaw 0.21.3 under the CDDL-1.0, as LibRaw LLC permits. Its source is at {url}.",
     "`licenses/LibRaw-LICENSE.CDDL` (the operative text), `LibRaw-COPYRIGHT`, `LibRaw-LICENSE.LGPL` (copies of the files in `app/src-tauri/vendor/rsraw-sys/LibRaw/`); the BSD notices of its DCB, FBDD and X3F parts in `LibRaw-third-party-BSD.txt`"),
    (None, "jasper 4.2.1, lcms 2.14, libjpeg-turbo 3.0.4, zlib 1.3.1", "**Windows installer only.** Static libraries that the vcpkg build of LibRaw pulls in (JPEG 2000, colour management, JPEG and deflate support for LibRaw). The Windows build installs them from vcpkg and passes them to the linker. The linker output contains only LibRaw and zlib's inflate side from vcpkg (measured in a linker map of a Windows release build, not taken from a packaged installer); the texts ship because the build asks for these libraries. The macOS build does not use them. Their versions are pinned by the vcpkg commit in `ci.yml` and checked against `scripts/vcpkg_expected_versions.json`",
     "JasPer License 2.0 (jasper); MIT (lcms); BSD-3-Clause (libjpeg-turbo, as its port declares; its copyright file names the IJG licence too); Zlib (zlib), as declared by their vcpkg ports",
     "As declared",
     "`licenses/vcpkg-jasper-copyright.txt`, `vcpkg-lcms-copyright.txt`, `vcpkg-libjpeg-turbo-copyright.txt`, `vcpkg-zlib-copyright.txt` (the `share/<port>/copyright` file each vcpkg package installs, copied unchanged)"),
    ("lensfun", "`lensfun` crate", "Lens distortion, vignetting and chromatic-aberration correction; a pure-Rust port of Lensfun, compiled into the app",
     "LGPL-3.0-or-later; copyright the Lensfun contributors and David Veszelovszki", "LGPL-3.0-or-later",
     "`licenses/lensfun-LICENSE-LGPL-3.0`, `lensfun-LICENSE-GPL-3.0`, `lensfun-NOTICE` (the crate's own files)"),
    ("lensfun", "Lensfun lens database", "The lens calibration data (XML) the `lensfun` crate embeds and the app reads",
     "CC-BY-SA 3.0 Unported; the Lensfun community", "CC-BY-SA 3.0 Unported",
     "`licenses/CC-BY-SA-3.0.txt`; <https://creativecommons.org/licenses/by-sa/3.0/>"),
    ("cssparser", "`cssparser`, `dtoa-short`, `selectors`", "CSS parsing, pulled in by Tauri's HTML handling (`tauri-utils`, `wry`)",
     "MPL-2.0", "MPL-2.0. Source at <https://crates.io/crates/cssparser/{v[cssparser]}>, <https://crates.io/crates/dtoa-short/{v[dtoa-short]}>, <https://crates.io/crates/selectors/{v[selectors]}> and at {url}",
     "`licenses/MPL-2.0.txt` (`selectors` ships no licence file of its own; its `Cargo.toml` declares MPL-2.0)"),
    ("dyn-eq", "`dyn-eq`", "Small helper used by `tract`, the ONNX inference engine", "MPL-2.0",
     "MPL-2.0. Source at <https://crates.io/crates/dyn-eq/{v[dyn-eq]}> and at {url}", "`licenses/MPL-2.0.txt`"),
    ("option-ext", "`option-ext`", "Small helper used by `dirs`, which finds the app-data folder", "MPL-2.0",
     "MPL-2.0. Source at <https://crates.io/crates/option-ext/{v[option-ext]}> and at {url}", "`licenses/MPL-2.0.txt`"),
    ("webpki-root-certs", "`webpki-root-certs`", "Root certificates `reqwest` uses to verify HTTPS when downloading models",
     "CDLA-Permissive-2.0", "CDLA-Permissive-2.0", "`licenses/CDLA-Permissive-2.0.txt`"),
    ("rsraw", "Vendored `rsraw`, `rsraw-sys`", "Safe wrapper and build glue around LibRaw, patched in this repository",
     "MIT (upstream <https://github.com/hexilee/rsraw>)", "MIT. Source, with the generated `bindings.rs`, at {url}",
     "`app/src-tauri/vendor/*/Cargo.toml`"),
    ("jpeg-encoder", "`jpeg-encoder`", "Writes the JPEG of an upscaled picture one row at a time",
     "(MIT OR Apache-2.0) AND IJG", "MIT, and the IJG terms for the standard tables", "`licenses/IJG-notice.txt` and `licenses/rust-crates.txt`"),
)
OUTRO = """
The MPL-2.0 files are used unmodified. MPL-2.0 is a file-level licence: changes to those files would have to be published under it, and nothing else in Emulsion is affected.

Emulsion's own code is not open source; see the repository's `LICENSE` file. Terms of Emulsion's licence that differ from
the licences above are offered by Emulsion's author alone, not by LibRaw LLC or any other author, and nothing in them
limits your rights under those licences for those components.
"""


def libraries_text(versions, url):
    """The hand-written head of THIRD_PARTY_LIBRARIES.md for the crates a channel links, given as {name: version}."""
    rows = ["| " + " | ".join(cell.format(url=url, v=versions) for cell in cells) + " |" for needs, *cells in CONDITIONS if needs is None or needs in versions]
    return INTRO + "\n".join(rows) + "\n" + OUTRO


def open_source_url(path=OPEN_SOURCE_JS):
    m = re.search(r'^export const OPEN_SOURCE_URL = "(https://[^"]+)";$', path.read_text(), re.M)
    if not m:
        raise SystemExit(f"{path} must hold one line: export const OPEN_SOURCE_URL = \"https://...\";")
    return m.group(1)


def rust_libs(channel="direct"):
    out = subprocess.run(
        ["cargo", "--color", "never", "tree", "-e", "normal", "--target", "all", "--prefix", "none", "--format", "{p}|{l}", *CHANNELS[channel]],
        cwd=ROOT / "app" / "src-tauri", capture_output=True, text=True, check=True, env=NO_COLOR_ENV).stdout
    libs = {}
    for line in out.splitlines():
        line = line.replace(" (*)", "")
        if "(proc-macro)" in line:
            continue
        m = re.match(r"(\S+) v(\S+?)(?: \([^)]*\))?\|(.*)$", line)
        if not m or "emulsion" == m.group(1):
            continue
        name, version, lic = m.groups()
        libs[(name, version)] = lic.strip() or "(none declared)"
    return libs


def npm_libs():
    lock = json.loads((ROOT / "app" / "package-lock.json").read_text())
    libs = {}
    for path, meta in lock.get("packages", {}).items():
        if not path or meta.get("dev") or meta.get("devOptional"):
            continue
        name = path.split("node_modules/")[-1]
        libs[(name, meta.get("version", "?"))] = meta.get("license", "(none declared)")
    return libs


def section(title, libs):
    by_lic = defaultdict(list)
    for (name, version), lic in sorted(libs.items()):
        by_lic[lic].append(f"{name} {version}")
    lines = [f"## {title} ({len(libs)})", ""]
    for lic, items in sorted(by_lic.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        lines += [f"### {lic} ({len(items)})", "", ", ".join(items), ""]
    return "\n".join(lines)


def cargo_packages():
    """{(name, version): package} from `cargo metadata`, with `dir` (the crate's source folder) added."""
    out = subprocess.run(["cargo", "--color", "never", "metadata", "--format-version", "1", "--locked"], cwd=ROOT / "app" / "src-tauri",
                         capture_output=True, text=True, check=True, env=NO_COLOR_ENV).stdout
    return {(p["name"], p["version"]): {**p, "dir": Path(p["manifest_path"]).parent} for p in json.loads(out)["packages"]}


def squash(text):
    """The text without line breaks or runs of spaces, and with http and https alike: one MPL-2.0 file spells the
    Exhibit A address with https and the others with http, and that is the only difference."""
    return " ".join(text.split()).replace("https://", "http://")


def read_text(path):
    """A file's text with LF line ends, so a Windows checkout gives the same output."""
    return path.read_bytes().decode("utf-8", errors="replace").replace("\r\n", "\n")


NOTICE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE)([-._].*)?$", re.I)
# "Copyright" or "(c)" at the start of a line, followed by a year, a symbol or a capitalised name: an owner's notice, not
# the licence body's "copyright notice" or "(c) You must retain", and not Apache's "Copyright [yyyy]" template.
COPYRIGHT_LINE = re.compile(r"^\W*(?:[Cc]opyright|COPYRIGHT)\s*(?:\([cC]\)|©|\d{4}|(?:by\s+)?[A-Z(<])|^\W*(?:\([cC]\)|©)\s*\d{4}", re.M)
APACHE_BODY = "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION"
APACHE_STANDIN = "[The Apache License 2.0. Its full text is in THIRD_PARTY_NOTICES.md. Copyright lines of this file, if any, follow.]"


def crate_notice(name, version, licence, files, authors=(), repository=None):
    """The text rust-crates.txt holds for one crate: its own licence files (a full Apache-2.0 text is replaced by a pointer
    and its copyright lines, because the same text ships in THIRD_PARTY_NOTICES.md), then, when no file has a copyright
    line, the attribution its Cargo.toml gives. `files` is [(file name, text)]."""
    parts = []
    for fname, text in files:
        if APACHE_BODY in text and len(squash(text)) < 12000:
            text = "\n".join([APACHE_STANDIN] + [line.strip() for line in text.splitlines() if COPYRIGHT_LINE.match(line)])
        parts.append(f"--- {fname}\n{text.strip()}\n")
    if not any(COPYRIGHT_LINE.search(text) for _, text in files):
        who = ", ".join(authors) if authors else f"the authors of {name}"
        where = f", {repository}" if repository else ""
        shipped = "carries no copyright line" if files else "ships no licence file"
        parts.append(f"--- Cargo.toml (the crate {shipped})\nCopyright {who}{where}. Licence declared: {licence}.\n")
    return "\n".join(parts)


def rust_crates_text(notices):
    """rust-crates.txt from {(name, version): notice}: crates with the same notice text share one section."""
    groups = defaultdict(list)
    for (name, version), notice in sorted(notices.items()):
        groups[notice].append(f"{name} {version}")
    head = ("Licence files of the Rust crates compiled into Emulsion (generated by scripts/gen_third_party_libraries.py; do not\n"
            "edit by hand). Each section names the crates that ship the same files, then the files. A full Apache License 2.0\n"
            "text is given once, in THIRD_PARTY_NOTICES.md, and only its copyright lines are repeated here. A crate whose files\n"
            "carry no copyright line is followed by the authors its Cargo.toml names.\n")
    body = [f"\n{'=' * 100}\n{', '.join(crates)}\n{'=' * 100}\n{notice}" for notice, crates in sorted(groups.items(), key=lambda kv: kv[1][0])]
    return head + "".join(body)


def libraw_bsd_text(libraw_dir):
    out = ["BSD notices of parts of LibRaw that Emulsion compiles (generated by scripts/gen_third_party_libraries.py; do not\n"
           "edit by hand). Each is copied unchanged from the head of the LibRaw source file named above it.\n"]
    for rel, first, last in LIBRAW_BSD_NOTICES:
        lines = read_text(libraw_dir / rel).splitlines()
        try:
            a = next(i for i, line in enumerate(lines) if line.startswith(first))
            b = next(i for i, line in enumerate(lines) if i > a and line.strip().startswith(last))
        except StopIteration:
            raise SystemExit(f"{libraw_dir / rel}: the BSD notice between {first!r} and {last!r} is gone; update LIBRAW_BSD_NOTICES")
        out.append(f"\n{'=' * 100}\nLibRaw/{rel}\n{'=' * 100}\n" + "\n".join(lines[a:b + 1]) + "\n")
    return "".join(out)


def ijg_text(crates):
    names = ", ".join(f"{n} {v}" for (n, v), lic in sorted(crates.items()) if "IJG" in lic)
    return (f"{IJG_SENTENCE}\n\nThe IJG licence asks binary distributions to state the sentence above. It applies here to {names},\n"
            "whose licence expression names IJG for the standard JPEG tables it uses.\n") if names else None


def readme_text(crate_names, url, has_ijg):
    """licenses/README.txt for the crates a channel links."""
    lensfun = "lensfun" in crate_names
    parts = [
        "Licence texts that ship with Emulsion (generated by scripts/gen_third_party_libraries.py; do not edit by hand).\n"
        "THIRD_PARTY_LIBRARIES.md, installed beside this folder, says which library each one belongs to.\n\n",
        "LibRaw-LICENSE.CDDL, LibRaw-COPYRIGHT, LibRaw-LICENSE.LGPL\n"
        "    LibRaw, the RAW decoder: copyright LibRaw LLC; offered under the LGPL-2.1 or the CDDL-1.0, at the user's choice.\n"
        "    Emulsion uses LibRaw 0.21.3 under the CDDL-1.0, as LibRaw LLC permits, so LibRaw-LICENSE.CDDL is the operative\n"
        f"    text. Its source is at {url}. The COPYRIGHT file names both licences, so both texts ship.\n",
        "LibRaw-third-party-BSD.txt\n"
        "    The BSD notices of LibRaw's DCB demosaic and FBDD denoise (Jacek Gozdz) and X3F parser (Roland Karlsson).\n",
    ]
    if lensfun:
        parts += [
            "lensfun-LICENSE-LGPL-3.0, lensfun-LICENSE-GPL-3.0, lensfun-NOTICE\n"
            "    The `lensfun` crate (lens corrections): LGPL-3.0-or-later, with the GPL-3.0 it refers to, and the crate's own notice.\n",
            "CC-BY-SA-3.0.txt\n"
            "    The Lensfun lens database (the lens calibration data), Creative Commons Attribution-ShareAlike 3.0 Unported,\n"
            "    by the Lensfun community. The database is used unmodified.\n",
        ]
    parts += [
        "MPL-2.0.txt\n"
        "    cssparser, dtoa-short, dyn-eq, option-ext and selectors (Mozilla Public License 2.0). These crates are used\n"
        f"    unmodified. The selectors crate carries no licence file of its own; its Cargo.toml declares MPL-2.0. Their source\n"
        f"    is on crates.io and at {url}.\n",
        "CDLA-Permissive-2.0.txt\n"
        "    webpki-root-certs (the root certificates used to check HTTPS), Community Data License Agreement Permissive 2.0.\n",
        "rust-crates.txt\n"
        "    Each Rust crate's own licence files, with their copyright lines.\n",
    ]
    if has_ijg:
        parts.append("IJG-notice.txt\n    The Independent JPEG Group sentence that the jpeg-encoder crate's licence asks for.\n")
    parts += [
        "vcpkg-jasper-copyright.txt, vcpkg-lcms-copyright.txt, vcpkg-libjpeg-turbo-copyright.txt, vcpkg-zlib-copyright.txt\n"
        "    Windows installer only. The notice each vcpkg package installs as share/<port>/copyright, copied unchanged, for the\n"
        "    static libraries the Windows build installs from vcpkg and passes to the linker besides LibRaw: jasper (JPEG 2000, JasPer License 2.0), lcms (Little CMS,\n"
        "    colour management), libjpeg-turbo (JPEG) and zlib (deflate).\n\n",
        "The MIT, Apache-2.0 and BSD texts that most other libraries use are in THIRD_PARTY_NOTICES.md.\n\n",
        "Emulsion's own licence does not limit your rights under these licences for these components. Where it differs from\n"
        "them, those terms are offered by Emulsion's author alone, not by LibRaw LLC or any other author.\n",
    ]
    return "".join(parts)


COPYLEFT = re.compile(r"GPL|CDDL|EPL|CC-BY-SA|MPL|SSPL|OSL|CPL|EUPL", re.I)
# Crates named in the conditions table (LibRaw and the database are not crates). A crate with one of these
# licences that is not listed here fails the run, so a new copyleft dependency cannot arrive unnoticed.
COPYLEFT_ALLOWED = {"cssparser", "dtoa-short", "dyn-eq", "lensfun", "option-ext", "selectors"}


def copyleft_only(expression):
    """True if every OR alternative of an SPDX-like licence expression names a copyleft licence (so the user cannot
    choose a permissive one). `MIT OR Apache-2.0 OR LGPL-2.1-or-later` is not copyleft-only; `MPL-2.0` and
    `LGPL-3.0-or-later` are. AND terms stay in one alternative, so `MIT AND MPL-2.0` counts as copyleft."""
    alternatives = [a for a in re.split(r"\s+OR\s+|/", expression.replace("(", " ").replace(")", " ")) if a.strip()]
    return bool(alternatives) and all(COPYLEFT.search(a) for a in alternatives)


def copyleft_offenders(libs, allowed=COPYLEFT_ALLOWED):
    return sorted(f"{name} {version} ({lic})" for (name, version), lic in libs.items() if copyleft_only(lic) and name not in allowed)


def licence_outputs(rust, out_root=ROOT, url=None):
    """The files licenses/ must hold for the crates in `rust`, as {path: bytes}. Everything is copied or built from the
    files that are really in the build (the vendored LibRaw, the crates Cargo resolved), so the folder follows a version
    bump; CC-BY-SA-3.0.txt is the one committed by hand and is checked against its pinned hash, as are the four vcpkg
    copyright files."""
    licenses, committed = out_root / "licenses", ROOT / "licenses"
    url = url or open_source_url()
    cc = committed / "CC-BY-SA-3.0.txt"
    if not cc.is_file() or hashlib.sha256(cc.read_bytes()).hexdigest() != CC_BY_SA_SHA256:
        raise SystemExit(f"{cc} is missing or is not the pinned CC-BY-SA 3.0 legal code (see CC_BY_SA_SHA256)")
    pk = cargo_packages()
    by_name = defaultdict(list)
    for (name, _), p in pk.items():
        by_name[name].append(p["dir"])
    one = lambda name: by_name[name][0]  # these crates occur once in the lockfile (checked below for lensfun)
    names = {name for name, _ in rust}
    libraw = ROOT / LIBRAW
    wanted = {
        "LibRaw-COPYRIGHT": libraw / "COPYRIGHT",
        "LibRaw-LICENSE.LGPL": libraw / "LICENSE.LGPL",
        "LibRaw-LICENSE.CDDL": libraw / "LICENSE.CDDL",
        "MPL-2.0.txt": one("cssparser") / "LICENSE",
        "CDLA-Permissive-2.0.txt": one("webpki-root-certs") / "LICENSE",
    }
    if "lensfun" in names:
        assert len(by_name["lensfun"]) == 1, "more than one lensfun in the lockfile"
        wanted.update({"lensfun-LICENSE-LGPL-3.0": one("lensfun") / "LICENSE-LGPL-3.0",
                       "lensfun-LICENSE-GPL-3.0": one("lensfun") / "LICENSE-GPL-3.0", "lensfun-NOTICE": one("lensfun") / "NOTICE"})
    # One MPL-2.0 text stands for all of them: the crates that carry a file must carry the same text.
    for name, fname in (("dtoa-short", "LICENSE"), ("dyn-eq", "LICENSE"), ("option-ext", "LICENSE.txt")):
        if squash((one(name) / fname).read_text()) != squash(wanted["MPL-2.0.txt"].read_text()):
            raise SystemExit(f"{name}'s {fname} differs from cssparser's MPL-2.0 text: ship both")
    out = {}
    for name, digest in VCPKG_TEXTS.items():
        f = committed / name
        if not f.is_file() or hashlib.sha256(f.read_bytes()).hexdigest() != digest:
            raise SystemExit(f"{f} is missing or is not the pinned vcpkg copyright file (see VCPKG_TEXTS)")
        out[licenses / name] = f.read_bytes()
    for name, src in wanted.items():
        if not src.is_file():
            raise SystemExit(f"missing licence source {src}")
        out[licenses / name] = src.read_bytes()
    notices = {}
    for key, lic in rust.items():
        p = pk[key]
        files = [(f.name, read_text(f)) for f in sorted(p["dir"].iterdir(), key=lambda f: f.name) if f.is_file() and NOTICE_FILE.match(f.name)]
        notices[key] = crate_notice(*key, lic, files, p.get("authors") or (), p.get("repository"))
    out[licenses / "rust-crates.txt"] = rust_crates_text(notices).encode()
    out[licenses / "LibRaw-third-party-BSD.txt"] = libraw_bsd_text(libraw).encode()
    ijg = ijg_text(rust)
    if ijg:
        out[licenses / "IJG-notice.txt"] = ijg.encode()
    out[licenses / "README.txt"] = readme_text(names, url, bool(ijg)).encode()
    if "lensfun" in names:
        out[licenses / "CC-BY-SA-3.0.txt"] = cc.read_bytes()
    return out


def first_difference(have, want, limit=24):
    """A short unified diff (what is on disk -> what the build gives) so a CI failure shows the cause, not just the name."""
    diff = list(difflib.unified_diff(have.decode(errors="replace").splitlines(), want.decode(errors="replace").splitlines(),
                                     "on disk", "generated", lineterm="", n=1))
    clipped = [line[:200] for line in diff[:limit]]
    return "\n" + "\n".join("      " + line for line in clipped) + (f"\n      ... ({len(diff) - limit} more lines)" if len(diff) > limit else "")


def drift(outputs, present, root=None):
    """What differs between the generated outputs and the files on disk: names that are missing, changed or unexpected.
    `present` is the set of paths that exist under licenses/ (so a stray file counts)."""
    root = root or ROOT
    problems = []
    for path, want in outputs.items():
        if not path.is_file():
            problems.append(f"missing: {path.relative_to(root)}")
        elif path.read_bytes() != want:
            problems.append(f"out of date: {path.relative_to(root)}" + first_difference(path.read_bytes(), want))
    for path in sorted(present - set(outputs)):
        problems.append(f"unexpected: {path.relative_to(root)}")
    return problems


def main(argv):
    if "--self-test" in argv:
        return 0 if unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SelfTest)).wasSuccessful() else 1
    channel = argv[argv.index("--channel") + 1] if "--channel" in argv else "direct"
    out_root = Path(argv[argv.index("--out") + 1]).resolve() if "--out" in argv else ROOT
    if channel not in CHANNELS:
        print(f"unknown channel {channel}; known: {', '.join(CHANNELS)}", file=sys.stderr)
        return 2
    if channel != "direct" and out_root == ROOT:
        print("the committed files are the direct channel's; write another channel with --out DIR", file=sys.stderr)
        return 2
    rust, npm = rust_libs(channel), npm_libs()
    bad = copyleft_offenders(rust) + copyleft_offenders(npm)
    if bad:
        print("copyleft-only dependencies that THIRD_PARTY_LIBRARIES.md does not name (add them to CONDITIONS "
              "and to COPYLEFT_ALLOWED, with their licence text in licenses/, after a decision about them):\n  " + "\n  ".join(bad), file=sys.stderr)
        return 1
    url = open_source_url()
    if url not in (ROOT / "LICENSE").read_text():
        print(f"LICENSE must name the source address {url} that app/src/lib/openSource.js gives", file=sys.stderr)
        return 1
    text = (libraries_text({name: version for name, version in rust}, url) + "\n" + section("Rust crates", rust) + "\n" + section("JavaScript packages", npm)).rstrip() + "\n"
    out_file, licenses = out_root / "THIRD_PARTY_LIBRARIES.md", out_root / "licenses"
    outputs = {out_file: text.encode(), **licence_outputs(rust, out_root, url)}
    if "--check" in argv:
        problems = drift(outputs, {p for p in licenses.rglob("*") if p.is_file()} if licenses.is_dir() else set(), out_root)
        if problems:
            print("generated licence files are out of date (run python3 scripts/gen_third_party_libraries.py and commit):\n  " + "\n  ".join(problems), file=sys.stderr)
            return 1
        print(f"up to date ({channel}): THIRD_PARTY_LIBRARIES.md and {len(outputs) - 1} files in licenses/")
        return 0
    licenses.mkdir(parents=True, exist_ok=True)
    for path, data in outputs.items():
        path.write_bytes(data)
    stray = sorted(p.name for p in licenses.iterdir() if p not in outputs)
    if stray:
        print(f"licenses/ holds files the generator does not know: {', '.join(stray)}", file=sys.stderr)
        return 1
    print(f"wrote {out_file} ({len(text.splitlines())} lines) and {len(outputs) - 1} files in {licenses} ({channel})")
    return 0


MPL_VERSIONS = {"cssparser": "1", "dtoa-short": "2", "selectors": "3", "dyn-eq": "4", "option-ext": "5"}


class SelfTest(unittest.TestCase):
    def test_copyleft_only_needs_every_alternative_to_be_copyleft(self):
        for expr in ("MPL-2.0", "LGPL-3.0-or-later", "GPL-2.0-only OR LGPL-2.1", "MIT AND MPL-2.0", "CC-BY-SA-3.0", "(MPL-2.0)"):
            self.assertTrue(copyleft_only(expr), expr)
        for expr in ("MIT", "MIT OR Apache-2.0 OR LGPL-2.1-or-later", "Apache-2.0/MIT", "(MIT OR Apache-2.0) AND Unicode-3.0", "MPL-2.0 OR MIT", "(none declared)"):
            self.assertFalse(copyleft_only(expr), expr)

    def test_a_new_copyleft_dependency_is_an_offender_and_a_named_one_is_not(self):
        libs = {("cssparser", "1"): "MPL-2.0", ("newcrate", "1"): "LGPL-3.0-only", ("r-efi", "5"): "MIT OR Apache-2.0 OR LGPL-2.1-or-later", ("serde", "1"): "MIT OR Apache-2.0"}
        self.assertEqual(copyleft_offenders(libs), ["newcrate 1 (LGPL-3.0-only)"])
        self.assertEqual(copyleft_offenders(libs, allowed={"cssparser", "newcrate"}), [])

    def test_drift_names_missing_changed_and_unexpected_files(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "licenses").mkdir()
            same, changed, stray = root / "licenses/a", root / "licenses/b", root / "licenses/stray"
            same.write_bytes(b"x"); changed.write_bytes(b"old"); stray.write_bytes(b"?")
            outputs = {same: b"x", changed: b"new", root / "licenses/c": b"c"}
            found = drift(outputs, {same, changed, stray}, root)
            self.assertEqual([f.split("\n")[0] for f in found], ["out of date: licenses/b", "missing: licenses/c", "unexpected: licenses/stray"])
            self.assertIn("-old", found[0])
            self.assertIn("+new", found[0])
            self.assertEqual(drift({same: b"x"}, {same}, root), [])

    def test_libraw_row_states_the_cddl_election_and_the_source_url(self):
        text = libraries_text(MPL_VERSIONS, "https://example.test/src")
        row = next(line for line in text.splitlines() if line.startswith("| LibRaw"))
        self.assertIn("Emulsion uses LibRaw 0.21.3 under the CDDL-1.0, as LibRaw LLC permits. Its source is at https://example.test/src.", row)
        self.assertIn("`licenses/LibRaw-LICENSE.CDDL` (the operative text)", row)
        readme = readme_text({"lensfun"}, "https://example.test/src", True)
        self.assertIn("LibRaw-LICENSE.CDDL is the operative", readme)
        self.assertIn("https://example.test/src", readme)

    def test_a_channel_without_lensfun_lists_no_lensfun_row_and_no_lens_database(self):
        direct = libraries_text({**MPL_VERSIONS, "lensfun": "1", "rsraw": "1"}, "u")
        store = libraries_text({**MPL_VERSIONS, "rsraw": "1"}, "u")
        self.assertIn("| `lensfun` crate", direct)
        self.assertIn("| Lensfun lens database", direct)
        self.assertNotIn("ensfun", store)
        self.assertNotIn("ensfun", readme_text({"cssparser"}, "u", True))
        self.assertNotIn("CC-BY-SA", readme_text({"cssparser"}, "u", True))

    def test_a_crate_without_a_copyright_line_is_attributed_from_its_cargo_toml(self):
        mit = "Permission is hereby granted, free of charge, to any person obtaining a copy"
        with_line = crate_notice("a", "1", "MIT", [("LICENSE-MIT", "Copyright (c) 2020 Ann\n\n" + mit)], ["Ann <a@x>"])
        self.assertNotIn("Cargo.toml", with_line)
        no_line = crate_notice("b", "1", "MIT", [("LICENSE", mit)], ["Bo <b@x>"], "https://example.test/b")
        self.assertIn("Copyright Bo <b@x>, https://example.test/b. Licence declared: MIT.", no_line)
        self.assertIn("carries no copyright line", no_line)
        no_file = crate_notice("c", "1", "MIT", [], [], None)
        self.assertIn("ships no licence file", no_file)
        self.assertIn("Copyright the authors of c. Licence declared: MIT.", no_file)

    def test_a_full_apache_text_shrinks_to_its_copyright_lines_and_shared_notices_merge(self):
        apache = "Apache License\nVersion 2.0\n" + APACHE_BODY + "\n" + "x " * 3000 + "\nCopyright [yyyy] [name of copyright owner]\n"
        notice = crate_notice("a", "1", "Apache-2.0", [("LICENSE-APACHE", apache + "Copyright 2019 Ann\n")])
        self.assertIn(APACHE_STANDIN, notice)
        self.assertIn("Copyright 2019 Ann", notice)
        self.assertNotIn("[yyyy]", notice)
        self.assertNotIn("x x x", notice)
        for body in ("copyright notice that is included in", "(c) You must retain, in the Source form", "The above copyright notice"):
            self.assertIsNone(COPYRIGHT_LINE.search(body), body)
        for owner in ("Copyright (c) 2016 Ann", "Copyright 2019 Ann", "Copyright The Rust Project Developers", "  Copyright © Bo", "(c) 2020 Cy"):
            self.assertIsNotNone(COPYRIGHT_LINE.search(owner), owner)
        text = rust_crates_text({("a", "1"): "same\n", ("b", "2"): "same\n", ("c", "3"): "other\n"})
        self.assertIn("a 1, b 2\n", text)
        self.assertEqual(text.count("\nsame\n"), 1)

    def test_ijg_sentence_follows_a_crate_that_names_ijg(self):
        self.assertIn(IJG_SENTENCE, ijg_text({("jpeg-encoder", "0.6.1"): "(MIT OR Apache-2.0) AND IJG", ("serde", "1"): "MIT"}))
        self.assertIn("jpeg-encoder 0.6.1", ijg_text({("jpeg-encoder", "0.6.1"): "(MIT OR Apache-2.0) AND IJG"}))
        self.assertIsNone(ijg_text({("serde", "1"): "MIT"}))

    def test_libraw_bsd_notices_are_copied_from_their_source_headers(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "src/demosaic").mkdir(parents=True); (root / "src/x3f").mkdir(parents=True)
            (root / "src/demosaic/dcb_demosaic.cpp").write_bytes(b"/*\r\n * Copyright (C) 2010, Jacek Gozdz\r\n */\r\n// FBDD by\r\n// Luis Sanz Rodriguez\r\ncode();\r\n")
            (root / "src/x3f/x3f_utils_patched.cpp").write_text("#ifdef X\n/* Library for accessing X3F Files\n* Copyright (c) 2010, Roland Karlsson\n*/\ncode();\n")
            text = libraw_bsd_text(root)
            self.assertIn("Jacek Gozdz\n */\n// FBDD by\n// Luis Sanz Rodriguez\n", text)
            self.assertIn("Roland Karlsson\n*/\n", text)
            self.assertNotIn("code()", text)
            self.assertNotIn("\r", text)

    def test_the_open_source_url_is_read_from_the_frontend_module(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            js = Path(d) / "openSource.js"
            js.write_text('// x\nexport const OPEN_SOURCE_URL = "https://example.test/src";\n')
            self.assertEqual(open_source_url(js), "https://example.test/src")
            js.write_text("export const OPEN_SOURCE_URL = 'x';\n")
            with self.assertRaises(SystemExit):
                open_source_url(js)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
