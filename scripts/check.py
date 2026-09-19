#!/usr/bin/env python3
"""Repository invariants; --full also runs all currently available build checks."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(*args, capture=False):
    result = subprocess.run(args, cwd=ROOT, text=True, check=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else None


def require(condition, message):
    if not condition:
        raise ValueError(message)


def current_checklist(content, unit):
    rows = re.findall(r"^- \[([ x])\] (C\d+) —", content, re.M)
    expected = [("x", unit)] + [(" ", f"C{int(unit[1:]) + i:02}") for i in range(1, len(rows))]
    require(rows == expected, f"Checklist must contain current {unit}, then contiguous pending units only")


def commit_gate(files):
    count = int(run("git", "rev-list", "--count", "--ignore-missing", "HEAD", capture=True))
    unit = f"C{count + 1:02}"
    current_checklist(files["docs/PLAN.md"], unit)
    changed = run("git", "diff", "--cached", "--name-only", capture=True).splitlines()
    require({"docs/PLAN.md", "docs/STATE.md"} <= set(changed), "Each commit updates PLAN and STATE")
    require(f"Current unit: {unit} complete" in files["docs/STATE.md"], "STATE must name completed unit")
    if (count + 1) % 5 == 0:
        require(f"Inspection unit: {unit}\n" in files["docs/REVIEWS.md"], f"Missing {unit} simplification inspection")
        require("docs/REVIEWS.md" in changed, "Inspection must be included in this commit")
    entries = run("git", "diff", "--cached", "--numstat", capture=True).splitlines()
    sizes = {name: int(add) + int(delete) for add, delete, name in
             (line.split("\t", 2) for line in entries) if add != "-"}
    if sum(sizes.values()) > 500:
        exception = json.loads(files["docs/commit-exceptions.json"]).get(unit, {})
        require(len(exception.get("reason", "")) >= 40, f"{unit}: >500 lines needs a specific exception")
        paths = set(exception.get("paths", []))
        require(paths <= sizes.keys(), "Exception lists paths absent from the commit")
        require(sum(n for p, n in sizes.items() if p not in paths) <= 500,
                "Handwritten change still exceeds 500 lines; split the logical work")
    print(f"{unit}: {sum(sizes.values())} changed text lines; checklist and cadence OK")


def self_test():
    current_checklist("- [x] C99 — Current\n- [ ] C100 — Next\n", "C99")
    for content in ("", "- [x] C98 — Stale", "- [x] C98 — Old\n- [x] C99 — Current",
                    "- [x] C99 — Current\n- [ ] C102 — Gap"):
        try:
            current_checklist(content, "C99")
        except ValueError:
            continue
        raise AssertionError("Checklist gate accepted stale, historical or missing work")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--staged", action="store_true")
    parser.add_argument("--full", action="store_true")
    args = parser.parse_args()
    names = run("git", "ls-files", *([] if args.staged else ["--cached", "--others", "--exclude-standard"]), capture=True).splitlines()
    files = {}
    for name in names:
        try:
            files[name] = run("git", "show", f":{name}", capture=True) + "\n" if args.staged else (ROOT / name).read_text()
        except (UnicodeDecodeError, FileNotFoundError):
            continue
    self_test()
    run("git", "diff", "--check", *(["--cached"] if args.staged else []))
    if args.staged:
        run("git", "diff", "--quiet")
        commit_gate(files)
    if (ROOT / "tools/Cargo.toml").exists():
        run("cargo", "xtask", "generate", "--check")
    if args.full:
        run("cargo", "xtask", "demo", "--check")
        if (ROOT / "scripts/build.py").exists():
            run(sys.executable, "scripts/build.py")
            run(sys.executable, "scripts/build_publish_test.py")
        if (ROOT / "scripts/live_check_test.py").exists():
            run(sys.executable, "scripts/live_check_test.py")
        if (ROOT / "backend/Package.swift").exists():
            run("swift", "test", "--package-path", "backend")
        if (ROOT / "frontend/Cargo.toml").exists():
            for command in (("fmt", "--check"), ("test", "--locked"), ("clippy", "--locked", "--all-targets", "--", "-D", "warnings")):
                run("cargo", command[0], "--manifest-path", "frontend/Cargo.toml", *command[1:])
        if (ROOT / "scripts/terminal_test.py").exists():
            run(sys.executable, "scripts/terminal_test.py")
        run(sys.executable, "scripts/localization_test.py")
        run(sys.executable, "scripts/public_check.py")
    print("Repository gates passed")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
