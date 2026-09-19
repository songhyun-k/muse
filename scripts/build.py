#!/usr/bin/env python3
"""Build one macOS executable. No signing identity is required for offline tests."""
import argparse
import os
from pathlib import Path
import plistlib
import re
import shlex
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def publish(binary, destination, bundle_id, identity="-", strip_debug=False):
    """Verify a new inode before replacing the path; running copies stay intact."""
    destination.parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".muse-", dir=destination.parent) as directory:
        staged = Path(directory) / "muse"
        shutil.copy2(binary, staged)
        if strip_debug:
            subprocess.run(["strip", "-S", str(staged)], check=True)
        signing = ["codesign", "--force", "--sign", identity, "--identifier", bundle_id]
        if identity != "-": signing += ["--options", "runtime", "--timestamp"]
        subprocess.run(signing + [str(staged)], check=True)
        subprocess.run(["codesign", "--verify", "--strict", str(staged)], check=True)
        subprocess.run([str(staged), "--probe"], check=True, timeout=10)
        staged.replace(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    profile = "release" if args.release else "debug"
    environment = dict(os.environ, MUSIC_BUILD_PROFILE=profile)
    metadata = {
        "CFBundleName": "muse",
        "CFBundleIdentifier": os.environ.get("MUSIC_BUNDLE_ID", "local.muse.cli"),
        "CFBundleShortVersionString": "0.1.0",
        "NSAppleMusicUsageDescription": "Browse your music library and play Apple Music. 음악 보관함을 탐색하고 음악을 재생합니다."
    }
    bundle_id = metadata["CFBundleIdentifier"]
    if not re.fullmatch(r"[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+", bundle_id):
        parser.error("MUSIC_BUNDLE_ID must be a valid reverse-DNS identifier")
    build = ROOT / "backend/.build"
    build.mkdir(parents=True, exist_ok=True)
    command = ["swift", "build", "--package-path", "backend", "-c", profile]
    if args.release:
        flags = ([flag for flag in environment["CARGO_ENCODED_RUSTFLAGS"].split("\x1f") if flag]
                 if "CARGO_ENCODED_RUSTFLAGS" in environment else shlex.split(environment.get("RUSTFLAGS", "")))
        cargo_cache = Path(environment.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
        flags += [f"--remap-path-prefix={ROOT}=muse", f"--remap-path-prefix={cargo_cache}=cargo"]
        environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
        command += ["-Xswiftc", "-gnone", "-Xswiftc", "-file-prefix-map", "-Xswiftc", f"{ROOT}=/muse"]
    output = subprocess.check_output(command + ["--show-bin-path"], cwd=ROOT, env=environment, text=True).strip()
    info = build / "Info.plist"
    encoded = plistlib.dumps(metadata)
    if not info.exists() or info.read_bytes() != encoded:
        info.write_bytes(encoded)
    subprocess.run(["cargo", "build", "--locked", "--manifest-path", "frontend/Cargo.toml"]
                   + (["--release"] if args.release else []), cwd=ROOT, env=environment, check=True)
    # SwiftPM does not track our external Rust archive or embedded plist inputs.
    # Relink the executable; all Swift and Rust compilation stays incremental.
    (Path(output) / "muse").unlink(missing_ok=True)
    subprocess.run(command, cwd=ROOT, env=environment, check=True)
    destination = ROOT / "dist/muse"
    publish(Path(output) / "muse", destination, bundle_id,
            os.environ.get("MUSIC_SIGN_IDENTITY", "-"), strip_debug=args.release)
    print(destination)


if __name__ == "__main__":
    main()
