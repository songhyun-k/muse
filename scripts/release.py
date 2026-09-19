#!/usr/bin/env python3
"""Build and inspect a relocatable native executable. No upload or publication."""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

from terminal_test import frames, session
from licenses import write_notices
from public_check import add_public_file
from build import version

ROOT = Path(__file__).resolve().parents[1]


def output(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def embedded_metadata(binary, commands):
    section = re.search(r'sectname __info_plist\s+segname __TEXT\s+addr \S+\s+size (0x[0-9a-fA-F]+)\s+offset (\d+)', commands)
    assert section, 'Missing embedded Info.plist'
    size, offset = int(section[1], 16), int(section[2])
    data = binary.read_bytes()
    assert offset + size <= len(data), 'Invalid embedded Info.plist bounds'
    return plistlib.loads(data[offset:offset + size])


def main():
    subprocess.run([sys.executable, 'scripts/build.py', '--release'], cwd=ROOT, check=True)
    binary = ROOT / 'dist/muse'
    libraries = [line.strip().split(' (')[0] for line in output('otool', '-L', str(binary)).splitlines()[1:]]
    assert libraries and all(path.startswith(('/usr/lib/', '/System/Library/')) for path in libraries), libraries
    commands = output('otool', '-l', str(binary))
    metadata = embedded_metadata(binary, commands)
    assert metadata['CFBundleIdentifier'] == os.environ.get('MUSIC_BUNDLE_ID', 'local.muse.cli')
    assert metadata['NSAppleMusicUsageDescription'].strip()
    assert metadata['CFBundleShortVersionString'] == version()
    assert output(str(binary), '--version').startswith(f'muse {version()} ')
    minimum = re.search(r'cmd LC_BUILD_VERSION.*?\bminos (\S+)', commands, re.S).group(1)
    architecture = output('lipo', '-archs', str(binary))
    assert not re.search(rb'/Users/[^/\x00\s]+/', binary.read_bytes()), 'Release contains personal build paths'
    subprocess.run(['codesign', '--verify', '--strict', str(binary)], check=True)
    signature = subprocess.run(['codesign', '-dv', '--verbose=2', str(binary)],
                               text=True, stderr=subprocess.PIPE, check=True).stderr
    assert f'Identifier={metadata["CFBundleIdentifier"]}' in signature
    with tempfile.TemporaryDirectory() as directory:
        copied = Path(directory) / 'muse'
        shutil.copy2(binary, copied)
        environment = dict(os.environ, PATH='/usr/bin:/bin')
        for args in (['--version'], ['--demo', '--probe'], ['--help']):
            subprocess.run([str(copied), *args], cwd=directory, env=environment,
                           stdout=subprocess.DEVNULL, check=True, timeout=10)
        text = '\n'.join(frames(session([(0.7, b'q')], binary=copied, options=('--reduced-motion',))))
        assert 'Soft Signal' in text and 'MUSIC' in text, 'relocated demo needs external assets'
    archive = ROOT / f'dist/muse-macos-{architecture.replace(" ", "-")}.tar.gz'
    notices = write_notices(ROOT)
    with tarfile.open(archive, 'w:gz') as package:
        for path in (binary, ROOT / 'LICENSE', ROOT / 'NOTICE.md', notices):
            add_public_file(package, path, path.name)
    report = dict(version=version(), commit=output('git', 'rev-parse', 'HEAD'),
                  dirty=bool(output('git', 'status', '--porcelain')), architecture=architecture,
                  minimumMacOS=minimum, bundleIdentifier=metadata['CFBundleIdentifier'],
                  signing='ad-hoc' if 'Signature=adhoc' in signature else 'identity',
                  sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), bytes=binary.stat().st_size,
                  archive=archive.name, archiveSha256=hashlib.sha256(archive.read_bytes()).hexdigest(),
                  dynamicLibraries=libraries, relocatedLaunch=True)
    (ROOT / 'dist/release.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: value for key, value in report.items() if key != 'dynamicLibraries'}, indent=2))


if __name__ == '__main__':
    main()
