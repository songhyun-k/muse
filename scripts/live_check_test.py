#!/usr/bin/env python3
"""Offline mode validation only; never authorize or play through the native provider."""
from pathlib import Path
import subprocess

binary = Path(__file__).resolve().parents[1] / 'dist/muse'
cases = [
    ['--live-check', '--demo'], ['--live-check', '--probe'],
    ['--live-check', '--read-only', '--demo'], ['--live-check', '--read-only', '--probe'],
    ['--live-check', '--read-only', '--read-only'], ['--live-check', '--unknown'],
]
for args in cases:
    result = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=5)
    assert result.returncode == 2 and '검사는 --live-check' in result.stderr, result
print('Native acceptance rejects mixed/invalid modes without live authorization or playback')
