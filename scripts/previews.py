#!/usr/bin/env python3
"""Render README images from the real UI with original synthetic fixture data."""
import json
from pathlib import Path
import subprocess
import tempfile
from demo_fixture import ROOT, load
from ui_cases import case


def main():
    cases = [case('overview-dark', theme=1, plain_icons=False),
             case('overview-light', theme=0, plain_icons=False),
             case('overview-ko', theme=0, language='ko', plain_icons=False)]
    payload = dict(cover=load()['cover'], cases=cases)
    result = subprocess.run(['cargo','run','--quiet','--locked','--manifest-path','frontend/Cargo.toml',
                             '--example','snapshot'],cwd=ROOT,input=json.dumps(payload),text=True,
                            stdout=subprocess.PIPE,check=True)
    directory = ROOT / 'docs/assets'
    directory.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / 'frames.jsonl'
        path.write_text(result.stdout)
        subprocess.run(['swift','scripts/render_preview.swift',str(path),str(directory)],cwd=ROOT,check=True)


if __name__ == '__main__':
    main()
