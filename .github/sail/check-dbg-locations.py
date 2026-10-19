#!/usr/bin/env python3
"""Every `dbg!` in upstream's verifier sources must sit at the same file:line
in the fork (SAIL.md, "Divergences from upstream").

`dbg!` prints `[file:line:column]`, and Verus reaches some of them in ordinary
diagnostics (Reporter::report_as prints an unresolvable label span with one),
so a fork edit that shifts such a line changes stderr against stock.

    check-dbg-locations.py [--base REV]

REV defaults to the newest `upstream/*` branch (local, or `origin/upstream/*`)
that is an ancestor of HEAD, the fork's upstream base. Exit 1 lists every moved
or changed `dbg!` line.
"""
import argparse
import subprocess
import sys


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def base():
    refs = git("for-each-ref", "--sort=-committerdate", "--format=%(refname)",
               "refs/heads/upstream/", "refs/remotes/origin/upstream/").split()
    for ref in refs:
        if subprocess.run(["git", "merge-base", "--is-ancestor", ref, "HEAD"]).returncode == 0:
            return ref
    sys.exit("no upstream/* branch is an ancestor of HEAD; pass --base")


def dbg_lines(text):
    return [(n, line) for n, line in enumerate(text.splitlines(), 1)
            if "dbg!(" in line and not line.lstrip().startswith("//")]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base")
    rev = parser.parse_args().base or base()
    files = [f for f in git("ls-tree", "-r", "--name-only", rev, "source").split()
             if f.endswith(".rs") and "/tests/" not in f]
    problems, checked = [], 0
    for path in files:
        expected = dbg_lines(git("show", f"{rev}:{path}"))
        if not expected:
            continue
        try:
            with open(path) as f:
                actual = f.read().splitlines()
        except FileNotFoundError:
            problems.append(f"{path}: missing in the fork")
            continue
        for number, line in expected:
            checked += 1
            if number > len(actual) or actual[number - 1] != line:
                problems.append(f"{path}:{number}: expected {line.strip()!r}")
    for problem in problems:
        print(problem)
    print(f"{checked - len(problems)} of {checked} dbg! lines at upstream's location ({rev})")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
