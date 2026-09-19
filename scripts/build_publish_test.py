#!/usr/bin/env python3
"""Check the actual publication path without touching the user's executable."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile

from build import ROOT, publish
from release import embedded_metadata

source = ROOT / "dist/muse"
signature = subprocess.run(['codesign', '-dv', '--verbose=2', str(source)],
                           text=True, stderr=subprocess.PIPE, check=True).stderr
identifier = next(line.removeprefix('Identifier=') for line in signature.splitlines()
                  if line.startswith('Identifier='))
with tempfile.TemporaryDirectory() as directory:
    destination = Path(directory) / 'muse'
    shutil.copy2(source, destination)
    original = destination.read_bytes()
    old_inode = destination.stat().st_ino
    with destination.open('rb') as running_copy:
        publish(source, destination, identifier)
        assert destination.stat().st_ino != old_inode
        assert running_copy.read() == original, 'publication overwrote the running inode'
    published = destination.read_bytes()
    published_inode = destination.stat().st_ino
    invalid = Path(directory) / 'invalid'
    invalid.write_text('#!/bin/sh\nexit 23\n')
    invalid.chmod(0o755)
    try:
        publish(invalid, destination, identifier)
    except subprocess.CalledProcessError as error:
        assert error.returncode == 23 and error.cmd[-1] == '--probe'
    else:
        raise AssertionError('invalid executable was published')
    assert destination.stat().st_ino == published_inode
    assert destination.read_bytes() == published, 'failed publication lost the previous executable'
    assert not list(Path(directory).glob('.muse-*'))
    # Section bytes can start/end between otool's displayed 32-bit words.
    metadata = {'CFBundleName': '뮤즈', 'description': 'English / 한국어'}
    encoded = plistlib.dumps(metadata)
    fixture = Path(directory) / 'metadata'
    fixture.write_bytes(b'prefix!' + encoded + b'not plist')
    section = f'sectname __info_plist\nsegname __TEXT\naddr 0x0\nsize {len(encoded):#x}\noffset 7'
    assert embedded_metadata(fixture, section) == metadata
print('Binary publication preserves open inodes and rolls back failed validation')
