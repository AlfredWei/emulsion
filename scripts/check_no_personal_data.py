#!/usr/bin/env python3
"""Fails when a tracked file holds a macOS home-directory path or an Apple signing identity.

The repository is public. Evidence pasted from a local run (a probe's JSON, a codesign command, a worktree path) tends to
carry the machine's user name and the developer's certificate name and team id. This check reads every tracked text file
and reports file, line and category. It never prints the matched text, so a failing CI log does not repeat the leak.

Categories:
  home-path          the macOS home-directory prefix followed by anything, or that path with its slashes turned into dashes
                     (write a placeholder such as <home> instead)
  signing-identity   "Apple Development: <name>", "Developer ID Application: <name>" and the other certificate common names,
                     when the name is not a <placeholder>
  team-id            a 10 character upper-case team id in parentheses that mixes letters and digits

    python3 scripts/check_no_personal_data.py
    python3 scripts/check_no_personal_data.py --self-test

There is no allow-list: no tracked file needs these strings. Standard library only."""
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# The home prefix is built from two pieces so this file does not match its own pattern.
HOME_PREFIX = "/" + "Users" + "/"
ENCODED_HOME = "-" + "Users" + r"-[A-Za-z0-9._]+-"  # a home path with each slash turned into a dash, as Claude Code names its project folders
IDENTITY_KINDS = (
    "Apple Development",
    "Apple Distribution",
    "Developer ID Application",
    "Developer ID Installer",
    "3rd Party Mac Developer Application",
    "3rd Party Mac Developer Installer",
    "Mac Developer",
)

PATTERNS = (
    ("home-path", re.compile(re.escape(HOME_PREFIX) + "|" + ENCODED_HOME)),
    ("signing-identity", re.compile(r"(?:" + "|".join(map(re.escape, IDENTITY_KINDS)) + r"): ?(?![<\s])\S")),
    ("team-id", re.compile(r"\((?=[A-Z0-9]{10}\))(?=[A-Z0-9]*[A-Z])(?=[A-Z0-9]*[0-9])[A-Z0-9]{10}\)")),
)


def scan_text(text):
    """[(line number, category)] for every line that matches, one entry per category per line."""
    hits = []
    for number, line in enumerate(text.splitlines(), 1):
        for category, pattern in PATTERNS:
            if pattern.search(line):
                hits.append((number, category))
    return hits


def tracked_files(root):
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], check=True, capture_output=True).stdout
    return [p for p in out.decode("utf-8", errors="surrogateescape").split("\0") if p]


def scan_repo(root=REPO):
    """[(relative path, line number, category)] over the tracked text files. A file with a NUL byte near its start is binary."""
    hits = []
    for rel in tracked_files(root):
        path = Path(root) / rel
        if not path.is_file():
            continue
        data = path.read_bytes()
        if b"\0" in data[:8192]:
            continue
        for number, category in scan_text(data.decode("utf-8", errors="replace")):
            hits.append((rel, number, category))
    return hits


class SelfTest(unittest.TestCase):
    def test_each_category_is_found_and_a_clean_line_is_not(self):
        home = HOME_PREFIX + "someone/Library"
        identity = f"{IDENTITY_KINDS[0]}:{' '}Some Person (AB12CD34{'EF'})"
        self.assertEqual(scan_text(home), [(1, "home-path")])
        self.assertEqual([c for _, c in scan_text(identity)], ["signing-identity", "team-id"])
        self.assertEqual(scan_text("a\nb " + home + "\nc"), [(2, "home-path")])
        self.assertEqual(scan_text("x/tmp/-" + "Users" + "-someone-src-project/y"), [(1, "home-path")])
        for clean in ("the macOS home directory", "<home>/Library/Containers", "plain text", "", "a/Users"):
            self.assertEqual(scan_text(clean), [])

    def test_placeholders_and_prose_do_not_trip_the_identity_pattern(self):
        self.assertEqual(scan_text("Apple Development: <signing identity> (<team id>)"), [])
        self.assertEqual(scan_text("a Developer ID Application certificate and an Apple Development identity"), [])
        self.assertEqual(scan_text(f"{IDENTITY_KINDS[2]}:{' '}"), [])

    def test_team_id_needs_letters_and_digits_and_ten_characters(self):
        for text in ("(1013904223)", "(ABCDEFGHIJ)", "(AB12CD34E)", "(AB12CD34EFG)", "(ab12cd34ef)"):
            self.assertEqual(scan_text(text), [], text)
        self.assertEqual(scan_text("(AB12CD34" + "EF)"), [(1, "team-id")])

    def test_every_other_identity_kind_is_found(self):
        for kind in IDENTITY_KINDS:
            self.assertEqual(scan_text(f"{kind}: Name"), [(1, "signing-identity")], kind)

    def test_scan_repo_reads_tracked_text_files_only(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            git = lambda *a: subprocess.run(["git", "-C", d, "-c", "user.name=t", "-c", "user.email=t@example.invalid", *a], check=True, capture_output=True)
            git("init", "-q")
            (root / "clean.md").write_text("nothing here\n")
            (root / "leak.md").write_text("ok\n" + HOME_PREFIX + "someone/x\n")
            (root / "blob.bin").write_bytes(b"\0" + HOME_PREFIX.encode() + b"someone")
            (root / "untracked.md").write_text(HOME_PREFIX + "someone\n")
            git("add", "clean.md", "leak.md", "blob.bin")
            self.assertEqual(scan_repo(root), [("leak.md", 2, "home-path")])

    def test_the_checked_in_tree_is_clean(self):
        self.assertEqual(scan_repo(REPO), [])


def main(argv):
    if "--self-test" in argv:
        return 0 if unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SelfTest)).wasSuccessful() else 1
    if argv:
        print(__doc__, file=sys.stderr)
        return 2
    hits = scan_repo()
    for rel, number, category in hits:
        print(f"{rel}:{number}: {category}", file=sys.stderr)
    if hits:
        print(f"{len(hits)} line(s) hold personal data. Replace each with a placeholder such as <home>, <signing identity>, <team id> or <user>.", file=sys.stderr)
        return 1
    print("no home-directory paths or signing identities in tracked files")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
