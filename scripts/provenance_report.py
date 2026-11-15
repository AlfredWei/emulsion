#!/usr/bin/env python3
"""Provenance report for RFC-0038 B5.2: what git records about who and what wrote one ref's history.

Read-only: runs only `git rev-parse`, `rev-list`, `log`, `ls-tree` and `show`. Writes one file,
docs/rfc/RFC-0038-appendix/provenance.md, or prints it with --stdout. Deterministic: the output depends on the commit
the ref resolves to, never on the time of the run (one clone-dependent figure is labelled as such).

    python3 scripts/provenance_report.py                      # ref = main
    python3 scripts/provenance_report.py --ref e5fcd3a26 --stdout
    python3 scripts/provenance_report.py --repo PATH          # read another clone (the output file is written there)
    python3 scripts/provenance_report.py --redact-emails      # names only, no e-mail addresses in the tables
    python3 scripts/provenance_report.py --self-test          # throwaway repo in a temp dir; run by CI

It reports what git records; it cannot see who typed what. Engineering facts for counsel, not a legal reading.
Needs Python 3.11+ (tomllib) and git 2.28+."""
import argparse, os, re, shutil, subprocess, sys, tempfile, tomllib, unittest
from collections import Counter, defaultdict
from datetime import datetime
from pathlib import Path

OUT_REL = "docs/rfc/RFC-0038-appendix/provenance.md"
CARGO_REL = "app/src-tauri/Cargo.toml"
TRAILER = re.compile(r"(?im)^[ \t]*co-authored-by[ \t]*:[ \t]*(\S.*?)[ \t]*$")
VALUE = re.compile(r"^(?P<name>[^<]*)(?:<(?P<email>[^>]*)>)?(?P<rest>.*)$")
CLAUDE_NAME = re.compile(r"(?i)^claude(?:[ \t-]+(?P<family>[a-z]+))?(?:[ \t-]+(?P<ver>\d+(?:[.-]\d+)*))?\b")
FAMILIES = {"opus", "sonnet", "haiku", "instant", "fable", "code"}
LICENCE_FILE = re.compile(r"(?i)^(licen[cs]e|copying|copyright|notice|attribution|unlicen[cs]e)([._-].*)?$")
WEIGHTS = (".onnx", ".pth", ".pt", ".safetensors", ".tflite", ".ckpt")
# git guesses text/binary from the first 8 KB; an ONNX file can pass as text (17,028 "lines"), so decide by name.
BINARY = WEIGHTS + (".jpg", ".jpeg", ".png", ".gif", ".webp", ".tif", ".tiff", ".cr2", ".dng", ".nef", ".arw",
                    ".pdf", ".zip", ".gz")
GITHUB = "noreply@github.com"
F, R = "\x1f", "\x1e"
# Settings that would otherwise let a user's git config change the output.
GIT = ["git", "--no-pager", "-c", "core.quotepath=off", "-c", "i18n.logOutputEncoding=UTF-8",
       "-c", "log.showSignature=false", "-c", "log.mailmap=false", "-c", "diff.renames=false"]


def git(repo, *args):
    env = dict(os.environ, GIT_NO_REPLACE_OBJECTS="1", LC_ALL="C")
    return subprocess.run(GIT + ["-C", str(repo), *args], capture_output=True, text=True, encoding="utf-8",
                          errors="replace", check=True, env=env).stdout


def co_authors(body):
    """Every Co-authored-by value in a message as (label, is_claude), once per label per commit. Key case and
    spacing do not matter, text after the e-mail is ignored, a mid-line mention is not a trailer."""
    found = {}
    for raw in TRAILER.findall(body):
        m = VALUE.match(raw)
        name, email = " ".join(m["name"].split()), (m["email"] or "").strip().lower()
        c = CLAUDE_NAME.match(name)
        if c and (email.endswith("anthropic.com") or (c["family"] or "").lower() in FAMILIES):
            label = " ".join(p for p in ("Claude", (c["family"] or "").capitalize(),
                                         (c["ver"] or "").replace("-", ".")) if p)
            found[label] = True
        else:
            found[f"{name} <{email}>" if email else name] = False
    return sorted(found.items())


def bucket(path, roots):
    for r in roots:  # longest first
        if path == r or path.startswith(r + "/"):
            return r + "/"
    return path.split("/", 1)[0] + "/" if "/" in path else "(files at the root)"


def when(iso):
    return datetime.fromisoformat(iso.replace("Z", "+00:00"))


def collect(repo, ref):
    if git(repo, "rev-parse", "--is-shallow-repository").strip() == "true":
        sys.exit("shallow clone: counts would be wrong; fetch full history (actions/checkout fetch-depth: 0)")
    sha = git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}").strip()
    fmt = F.join(["%H", "%P", "%an <%ae>", "%cn <%ce>", "%aI", "%cI",
                  "%(trailers:key=Co-authored-by,valueonly,separator=%x1d)", "%B"]) + R
    commits = []
    for rec in git(repo, "log", sha, f"--format={fmt}").split(R):
        if rec.strip():
            h, p, a, c, ad, cd, gt, b = rec.lstrip("\n").split(F, 7)
            co = co_authors(b)
            commits.append(dict(h=h, merge=len(p.split()) > 1, a=a, c=c, ad=ad, cd=cd, co=co,
                                claude=any(ai for _, ai in co),
                                git_claude=any(ai for v in gt.split("\x1d") if v.strip()
                                               for _, ai in co_authors("Co-authored-by: " + v))))
    tree = {}
    for e in git(repo, "ls-tree", "-r", "-l", "-z", sha).split("\0"):
        if e:
            meta, path = e.split("\t", 1)
            tree[path] = meta.split()[3]
    roots = set()
    if CARGO_REL in tree:
        cargo = tomllib.loads(git(repo, "show", f"{sha}:{CARGO_REL}"))
        for spec in cargo.get("patch", {}).get("crates-io", {}).values():
            if isinstance(spec, dict) and "path" in spec:
                roots.add(os.path.normpath(os.path.join(os.path.dirname(CARGO_REL), spec["path"])))
    roots = sorted(roots, key=lambda r: (-len(r), r))
    claude = {c["h"] for c in commits if c["claude"]}
    lines = defaultdict(lambda: [0, 0, 0, 0])  # lines added, of which in Claude-trailer commits, binary files, commits
    for chunk in git(repo, "log", sha, "--no-merges", "--numstat", "-z", f"--format={R}%H").split(R):
        h, _, rest = chunk.partition("\0")
        touched = set()
        for e in rest.lstrip("\n").split("\0"):
            if e.count("\t") >= 2:
                add, _, path = e.split("\t", 2)
                k = bucket(path, roots)
                touched.add(k)
                if add == "-" or path.lower().endswith(BINARY):
                    lines[k][2] += 1
                else:
                    lines[k][0] += int(add)
                    lines[k][1] += int(add) if h in claude else 0
        for k in touched:
            lines[k][3] += 1
    off_ref = int(git(repo, "rev-list", "--count", "--branches", "--remotes", "--tags", "--not", sha).strip())
    return dict(sha=sha, commits=commits, tree=tree, roots=roots, lines=lines, off_ref=off_ref,
                licence_files=sorted(p for p in tree if "/" in p and LICENCE_FILE.match(p.rsplit("/", 1)[1])),
                cargo={r: tomllib.loads(git(repo, "show", f"{sha}:{r}/Cargo.toml")).get("package", {})
                       for r in roots if f"{r}/Cargo.toml" in tree})


def table(head, rows):
    out = ["| " + " | ".join(head) + " |", "|" + " --- |" * len(head)]
    return out + ["| " + " | ".join(str(x) for x in r) + " |" for r in rows] + [""]


def ranked(counter):
    return sorted(counter.items(), key=lambda kv: (-kv[1], kv[0]))


def span(dates):
    return min(dates, key=when), max(dates, key=when)


def render(d, ref, redact=False):
    # --redact-emails hides addresses in what is printed; every count still uses the raw identity.
    P = (lambda s: re.sub(r"\s*<[^>]*>", "", s)) if redact else (lambda s: s)
    cs, roots = d["commits"], d["roots"]
    merges = [c for c in cs if c["merge"]]
    squash = [c for c in cs if not c["merge"] and c["c"].endswith(f"<{GITHUB}>")]
    claude = [c for c in cs if c["claude"]]
    models = Counter(l for c in cs for l, ai in c["co"] if ai)
    others = Counter(l for c in cs for l, ai in c["co"] if not ai)
    differ = sorted(c["h"][:9] for c in cs if c["git_claude"] != c["claude"])
    head = next(c for c in cs if c["h"] == d["sha"])
    o = ["# RFC-0038 appendix: provenance of the git history", "",
         f"Generated by `scripts/provenance_report.py --ref {ref}{' --redact-emails' if redact else ''}`; do not edit by hand. Pinned to commit "
         f"`{d['sha']}` (committed {head['cd']}). Every figure except the one marked \"this clone\" depends only on "
         "that commit. It records what git says, not who typed what: engineering facts for counsel, not a legal "
         "reading (RFC-0038 section 4, question 8).", "", "## Summary", ""]
    o += table(["Fact", "Value"], [
        ["Commits reachable from the pinned commit", len(cs)],
        ["Distinct author identities", len({c["a"] for c in cs})],
        ["Distinct committer identities", len({c["c"] for c in cs})],
        ["Commits with a `Co-authored-by` trailer naming a Claude model", len(claude)],
        ["Same, by git's own trailer parser (cross-check)", sum(c["git_claude"] for c in cs)],
        ["Merge commits", len(merges)],
        ["Single-parent commits committed by GitHub (squash merges or web edits)", len(squash)],
        ["First / last author date", " / ".join(span([c["ad"] for c in cs]))],
        ["`.mailmap` at this commit (identities below are raw either way)", "yes" if ".mailmap" in d["tree"] else "no"]])
    if differ:
        o += ["The two trailer parsers disagree on: " + ", ".join(f"`{h}`" for h in differ), ""]
    o += ["## Identities", ""]
    o += table(["Author" + (" (name only)" if redact else " (raw `name <e-mail>`)"), "Commits"],
               [(P(k), v) for k, v in ranked(Counter(c["a"] for c in cs))])
    o += table(["Committer", "Commits", "of which merges"],
               [(P(k), v, sum(c["c"] == k for c in merges)) for k, v in ranked(Counter(c["c"] for c in cs))])
    o += ["## Co-author trailers", "",
          "A commit counts once per distinct name, however often the trailer repeats (squash merges repeat it). "
          "A commit naming two models counts in both rows. Dates are author dates, as recorded.", ""]
    o += table(["Claude model named", "Commits", "First", "Last"],
               [(m, k, *(x[:10] for x in span([c["ad"] for c in claude if (m, True) in c["co"]])))
                for m, k in ranked(models)])
    o += table(["Other co-author (not a Claude model)", "Commits"], [(P(k), v) for k, v in ranked(others)] or [("(none)", 0)])
    o += ["## Lines added, by area", "",
          "`git log --numstat` over non-merge commits, rename detection off: lines added per commit, summed. This is "
          "contribution, not survival (`git blame` on every file would take minutes and answer a different question). "
          "A trailer says a commit was AI-assisted; it does not say which lines. Vendored rows count upstream code as "
          "added by the commit that imported it, so their trailer share says nothing about who wrote that code.", ""]
    o += table(["Area", "Lines added", "in Claude-trailer commits", "Share", "Binary files added or changed", "Commits"],
               [(f"`{k}`" + (" (vendored)" if k.rstrip("/") in roots else ""), v[0], v[1],
                 f"{100 * v[1] / v[0]:.0f}%" if v[0] else "n/a", v[2], v[3])
                for k, v in sorted(d["lines"].items(), key=lambda kv: (-kv[1][0], kv[0]))])
    o += ["## Third-party content in the tree", "", "Vendored code (the `[patch.crates-io]` paths in `app/src-tauri/Cargo.toml`):", ""]
    o += table(["Directory", "Declared licence (its Cargo.toml)", "Upstream", "Licence files inside", "Patch note"],
               [(f"`{r}/`", d["cargo"].get(r, {}).get("license", "not declared"),
                 d["cargo"].get(r, {}).get("repository", "not declared"),
                 ", ".join(f"`{p[len(r) + 1:]}`" for p in d["licence_files"] if p.startswith(r + "/")) or "none",
                 "`PATCH.md`" if f"{r}/PATCH.md" in d["tree"] else "none") for r in roots] or [("(none)",) * 5])
    o += ["Licence or attribution files outside the vendored directories:", ""]
    o += [f"- `{p}`" for p in d["licence_files"] if not any(p.startswith(r + "/") for r in roots)] or ["- (none)"]
    o += ["", "Committed model weights (made by others; terms in `THIRD_PARTY_NOTICES.md`):", ""]
    o += [f"- `{p}` ({int(s):,} bytes)" for p, s in sorted(d["tree"].items()) if p.endswith(WEIGHTS)] or ["- (none)"]
    o += ["", "## Not covered", "",
          "- Contributions made only on GitHub (review comments, suggestions applied in the web editor, issue text): "
          "git records the committer, not who proposed a change.",
          f"- Squash merges: the {len(squash)} single-parent GitHub commits replace their branch commits, whose "
          "messages and authors are not on this ref.",
          f"- Merge commits ({len(merges)}) count as commits but add no lines in the table above.",
          f"- This clone: {d['off_ref']} commits on other local or remote branches or tags are not reachable from the "
          "pinned commit (unmerged spikes, squashed-away originals). This figure depends on the clone's refs.",
          "- AI-assisted work committed without a trailer, or a trailer on unassisted work: git cannot tell.",
          "- Who holds rights in any of this: a question for counsel.", ""]
    return "\n".join(o)


class SelfTest(unittest.TestCase):
    def test_trailer_parsing(self):
        body = ("Subject\n\nmentions Co-authored-by: Claude Opus 9 mid-line\n\n"
                "co-authored-by : claude sonnet 4-5 <noreply@anthropic.com> (pairing)\n"
                "Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>\n"
                "CO-AUTHORED-BY: Claude Monet <claude@example.org>\nCo-authored-by: Bob <Bob@Example.com>\n")
        self.assertEqual(co_authors(body), [("Bob <bob@example.com>", False),
                                            ("Claude Monet <claude@example.org>", False), ("Claude Sonnet 4.5", True)])

    def test_report_on_a_synthetic_repo(self):
        tmp = Path(tempfile.mkdtemp())
        try:
            env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1", HOME=str(tmp))
            day = iter(range(1, 32))

            def run(*a, who="Ann <ann@x.org>", by=None):
                d = f"2020-01-{next(day):02d}T12:00:00+00:00"
                (an, ae), (cn, ce) = who[:-1].split(" <"), (by or who)[:-1].split(" <")
                subprocess.run(["git", "-C", str(tmp), *a], check=True, capture_output=True, env=dict(
                    env, GIT_AUTHOR_NAME=an, GIT_AUTHOR_EMAIL=ae, GIT_COMMITTER_NAME=cn, GIT_COMMITTER_EMAIL=ce,
                    GIT_AUTHOR_DATE=d, GIT_COMMITTER_DATE=d))

            def commit(path, text, msg, **kw):
                (tmp / path).parent.mkdir(parents=True, exist_ok=True)
                (tmp / path).write_text(text)
                subprocess.run(["git", "-C", str(tmp), "add", path], check=True, env=env)
                run("commit", "-q", "-m", msg, **kw)

            subprocess.run(["git", "init", "-q", "-b", "main", str(tmp)], check=True, env=env)
            commit("README", "a\nb\n", "init")
            commit(CARGO_REL, '[patch.crates-io]\nfoo = { path = "vendor/foo" }\n', "cargo")
            commit("app/src-tauri/vendor/foo/LICENSE", "MIT\n",
                   "vendor\n\nCo-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>")
            commit("app/src-tauri/vendor/foo/Cargo.toml", '[package]\nname = "foo"\nlicense = "MIT"\n',
                   "x\n\nco-authored-by: claude opus 4.6 <noreply@anthropic.com> (pairing)")
            commit("src/a.rs", "1\n2\n3\n", "y\n\nCo-authored-by: Claude Haiku 4.5 <noreply@anthropic.com>\n"
                   "Co-authored-by: Bob <bob@x.org>")
            commit("src/b.rs", "1\n", "squash (#7)\n\nCo-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>\n"
                   "Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>", by=f"GitHub <{GITHUB}>")
            commit("src/c.rs", "1\n", "inline Co-authored-by: Claude Sonnet 5 is not a trailer")
            run("checkout", "-q", "-b", "feat")
            commit("m.onnx", "w\n" * 5, "weights")
            run("checkout", "-q", "main")
            run("merge", "-q", "--no-ff", "feat", "-m", "Merge pull request #8 from x/feat", by=f"GitHub <{GITHUB}>")
            run("checkout", "-q", "-b", "spike")
            commit("s", "s\n", "unmerged")
            run("checkout", "-q", "main")
            first, again = render(collect(tmp, "main"), "main"), render(collect(tmp, "main"), "main")
            self.assertEqual(first, again)
            for want in ["| Commits reachable from the pinned commit | 9 |",
                         "| Commits with a `Co-authored-by` trailer naming a Claude model | 4 |",
                         "| Same, by git's own trailer parser (cross-check) | 4 |",
                         "| Claude Sonnet 5 | 2 | 2020-01-03 | 2020-01-06 |", "| Claude Opus 4.6 | 1 |",
                         "| Bob <bob@x.org> | 1 |", "| GitHub <noreply@github.com> | 2 | 1 |",
                         "| Single-parent commits committed by GitHub (squash merges or web edits) | 1 |",
                         "| `app/src-tauri/vendor/foo/` | MIT | not declared | `LICENSE` | none |",
                         "| `src/` | 5 | 4 | 80% | 0 | 3 |", "| `(files at the root)` | 2 | 0 | 0% | 1 | 2 |",
                         "- `m.onnx` (10 bytes)", "This clone: 1 commits"]:
                self.assertIn(want, first)
            hidden = render(collect(tmp, "main"), "main", redact=True)
            self.assertNotIn("@", hidden.replace("noreply@", "").replace("anthropic", ""))  # no address survives
            self.assertIn("| Ann | 9 |", hidden)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--ref", default="main", help="branch, tag or commit to report on (default: main)")
    ap.add_argument("--stdout", action="store_true", help="print instead of writing " + OUT_REL)
    ap.add_argument("--redact-emails", action="store_true", help="names only: no e-mail addresses in the printed tables")
    ap.add_argument("--repo", help="repository to read (default: the one this script lives in)")
    ap.add_argument("--self-test", action="store_true", help="run the built-in tests on a throwaway repo")
    args = ap.parse_args()
    if args.self_test:
        r = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SelfTest))
        return 0 if r.wasSuccessful() else 1
    repo = Path(git(Path(args.repo or Path(__file__).resolve().parent), "rev-parse", "--show-toplevel").strip())
    text = render(collect(repo, args.ref), args.ref, args.redact_emails)
    if args.stdout:
        sys.stdout.write(text)
    else:
        (repo / OUT_REL).write_text(text)
        print(f"wrote {OUT_REL}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
