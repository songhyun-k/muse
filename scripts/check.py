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


def dependencies(files):
    allowed = {"Backend": {"Foundation", "Darwin", "MusicKit", "CoreAudio", "CoreGraphics", "ImageIO", "AppKit", "MusicContract", "Combine"},
               "HostTransport": {"Foundation", "MusicContract"},
               "MusicContract": {"Foundation"}}
    for name, content in files.items():
        for module, imports in allowed.items():
            if name.startswith(f"backend/Sources/{module}/") and name.endswith(".swift"):
                actual = set(re.findall(r"^\s*(?:@\w+(?:\([^)]*\))?\s+)*import\s+(\w+)", content, re.M))
                require(actual <= imports, f"{name}: forbidden imports {actual - imports}")
        if name.startswith("frontend/") and name.endswith((".rs", ".toml")):
            require(not re.search(r"MusicKit|Sources/Backend|backend/|swift_bridge", content),
                    f"{name}: frontend references backend implementation")
            if name.endswith(".rs") and name not in {
                "frontend/src/transport.rs", "frontend/src/lib.rs"
            }:
                require('extern "C"' not in content, f"{name}: FFI outside transport boundary")
        if name.startswith("backend/Sources/Backend/"):
            require(not re.search(r"CMuse|muse_run|frontend/|Ratatui", content),
                    f"{name}: backend references frontend")
            if Path(name).name.startswith("Demo"):
                require(not re.search(r"import (?:MusicKit|CoreAudio|AppKit)|MusicService\(|VolumeService\(|LyricsService\(|URLSession|URLRequest|Process\(", content),
                        f"{name}: demo must not reach native playback, devices or network")
    manifest = files.get("backend/Package.swift", "")
    if manifest:
        # SwiftPM describes the resolved target graph, not just spelling in imports.
        package = json.loads(run("swift", "package", "--package-path", "backend", "dump-package", capture=True))
        for target in package["targets"]:
            if target["name"] in allowed:
                edges = {next(iter(dep.values()))[0] for dep in target["dependencies"]}
                expected = set() if target["name"] == "MusicContract" else {"MusicContract"}
                require(edges <= expected, f"{target['name']}: forbidden target dependencies {edges}")


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
    dependencies({"backend/Sources/Backend/Good.swift": "import Foundation\nimport MusicContract"})
    for files in ({"backend/Sources/Backend/Bad.swift": "import CMuse"},
                  {"frontend/src/state.rs": 'include!("../../backend/Private.rs");'}):
        try:
            dependencies(files)
        except ValueError:
            continue
        raise AssertionError("Dependency gate accepted an illegal edge")


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
    dependencies(files)
    run("git", "diff", "--check", *(["--cached"] if args.staged else []))
    if args.staged:
        run("git", "diff", "--quiet")
        commit_gate(files)
    if (ROOT / "scripts/generate.py").exists():
        run(sys.executable, "scripts/generate.py", "--check")
    if args.full:
        if (ROOT / "scripts/embed_demo.py").exists():
            run(sys.executable, "scripts/embed_demo.py", "--check")
        if (ROOT / "tests/reference/snapshots.json.gz").exists():
            run(sys.executable, "scripts/reference.py", "--check")
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
        if (ROOT / "scripts/snapshots.py").exists():
            run(sys.executable, "scripts/snapshots.py")
        if (ROOT / "scripts/animations.py").exists():
            run(sys.executable, "scripts/animations.py")
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
