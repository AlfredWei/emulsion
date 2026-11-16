#!/usr/bin/env python3
"""Fails unless the vcpkg libraries CI links on Windows are exactly the versions recorded in scripts/vcpkg_expected_versions.json (RFC-0038 A1).

CI takes the whole vcpkg `ports/` tree from one pinned vcpkg commit, so the five libraries LibRaw pulls in (and their
licences) should not move when the runner image does. This makes that a checked claim: it reads the output of
`vcpkg list` and compares every package installed for the triplet with the recorded version, port-version included.
A package that is installed but not recorded fails too, so a new dependency cannot arrive unnoticed.

    "$VCPKG_INSTALLATION_ROOT/vcpkg" list | python3 scripts/check_vcpkg_versions.py -
    python3 scripts/check_vcpkg_versions.py --self-test

Standard library only."""
import json
import re
import sys
import unittest
from pathlib import Path

EXPECTED = Path(__file__).resolve().parent / "vcpkg_expected_versions.json"
LINE = re.compile(r"^(?P<name>[^\s:\[]+)(?P<features>\[[^\]]*\])?:(?P<triplet>\S+)\s+(?P<version>\S+)")


def installed(list_output, triplet):
    """{port: version} for the lines of `vcpkg list` installed for the triplet. Feature lines are skipped."""
    found = {}
    for line in list_output.splitlines():
        m = LINE.match(line)
        if m and m["triplet"] == triplet and not m["features"]:
            found[m["name"]] = m["version"]
    return found


def check(list_output, expected):
    """Returns (ok, message) for the parsed expected file."""
    got = installed(list_output, expected["triplet"])
    want = expected["versions"]
    problems = [f"{p}: expected {v}, installed {got.get(p, 'nothing')}" for p, v in sorted(want.items()) if got.get(p) != v]
    problems += [f"{p}: installed {v} but not recorded in {EXPECTED.name}" for p, v in sorted(got.items()) if p not in want]
    if problems:
        return False, "the vcpkg libraries differ from the recorded ones:\n  " + "\n  ".join(problems)
    return True, "vcpkg libraries match the record: " + ", ".join(f"{p} {v}" for p, v in sorted(want.items()))


class SelfTest(unittest.TestCase):
    EXP = {"triplet": "x64-windows-static-md", "versions": {"zlib": "1.3.1", "libraw": "0.21.3#1"}}
    LIST = (
        "libraw:x64-windows-static-md                      0.21.3#1             RAW image decoder\n"
        "zlib:x64-windows-static-md                        1.3.1                A compression library\n"
        "vcpkg-cmake:x64-windows                           2024-04-23           helper\n"
    )

    def test_matching_list_passes_and_other_triplets_are_ignored(self):
        self.assertTrue(check(self.LIST, self.EXP)[0])

    def test_a_changed_version_or_port_version_fails_and_names_the_port(self):
        for old, new in (("1.3.1 ", "1.3.2 "), ("0.21.3#1", "0.21.3#2")):
            ok, msg = check(self.LIST.replace(old, new), self.EXP)
            self.assertFalse(ok)
            self.assertIn("expected", msg)

    def test_a_missing_or_unrecorded_port_fails(self):
        ok, msg = check(self.LIST.replace("zlib:", "other:"), self.EXP)
        self.assertFalse(ok)
        self.assertIn("zlib: expected 1.3.1, installed nothing", msg)
        self.assertIn("other: installed", msg)

    def test_freeglut_returning_through_jasper_default_features_fails(self):
        ok, msg = check(self.LIST + "freeglut:x64-windows-static-md  3.4.0#3  GLUT\n", self.EXP)
        self.assertFalse(ok)
        self.assertIn("freeglut: installed 3.4.0#3 but not recorded", msg)

    def test_feature_lines_are_not_counted(self):
        self.assertTrue(check(self.LIST + "lcms[fastfloat]:x64-windows-static-md  2.14  feature\n", self.EXP)[0])

    def test_the_recorded_file_is_well_formed(self):
        data = json.loads(EXPECTED.read_text())
        self.assertEqual(data["triplet"], "x64-windows-static-md")
        self.assertEqual(data["versions"]["libraw"].split("#")[0], "0.21.3")
        self.assertFalse({"freeglut", "opengl", "opengl-registry", "egl-registry"} & set(data["versions"]))


def main(argv):
    if "--self-test" in argv:
        return 0 if unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SelfTest)).wasSuccessful() else 1
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    text = sys.stdin.read() if argv[0] == "-" else Path(argv[0]).read_text(encoding="utf-8", errors="replace")
    ok, message = check(text, json.loads(EXPECTED.read_text()))
    print(message, file=sys.stderr if not ok else sys.stdout)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
