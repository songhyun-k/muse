#!/usr/bin/env python3
"""Freeze or verify current approved UI inputs. Existing snapshots are never overwritten."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
from demo_fixture import ROOT, load
from ui_cases import PAGES, case

REFERENCE = ROOT / 'tests/reference'


def cases():
    result = []
    for language in ('ko', 'en'):
        for theme in range(5):
            for width, height in ((80, 24), (140, 40)):
                for index, page in enumerate(PAGES):
                    result.append(case(f'{language}-{theme}-{width}-{index}',language,theme,page,width,height))
        for index, settings in enumerate([
            dict(focus='nav'), dict(focus='right'), dict(panel='queue', focus='right'),
            dict(transparent=True), dict(left_open=False), dict(right_open=False),
            dict(left_open=False,right_open=False), dict(help=True), dict(plain_icons=False),
            dict(editing='검색',buffer='Mira',query='Mira'),
        ]):
            result.append(case(f'{language}-state-{index}',language,**settings))
    return result


def render(inputs, animated=False, benchmark=0):
    payload = dict(cases=inputs,cover=load()['cover'],animated=animated,benchmark=benchmark)
    command = ['cargo','run','--quiet','--release','--locked','--manifest-path','frontend/Cargo.toml','--example','snapshot']
    result = subprocess.run(command,cwd=ROOT,input=json.dumps(payload,ensure_ascii=False),text=True,
                            stdout=subprocess.PIPE,check=True)
    return [json.loads(line) for line in result.stdout.splitlines()]


def freeze(path, inputs, animated=False):
    assert not path.exists(), f'Refusing to replace frozen file: {path}'
    frames = render(inputs,animated)
    assert len(inputs) == len(frames)
    reference = [dict(item,cells=frame['cells'],expectedMotion=frame['motion'],expectedPanels=frame['panels'])
                 for item,frame in zip(inputs,frames)]
    path.write_bytes(gzip.compress(json.dumps(reference,ensure_ascii=False,separators=(',',':')).encode(),mtime=0))


def load_reference(path, inputs):
    reference = json.loads(gzip.decompress(path.read_bytes()))
    expected = [{k:v for k,v in row.items() if k not in ('cells','expectedMotion','expectedPanels')} for row in reference]
    assert expected == inputs, f'Frozen presentation inputs drifted: {path.name}'
    return reference


def compare(reference, frames):
    assert len(reference) == len(frames)
    count, differences = 0, []
    for expected,actual in zip(reference,frames):
        assert expected['name'] == actual['name']
        assert len(actual['cells']) == expected['height']
        for y,(before,row) in enumerate(zip(expected['cells'],actual['cells'])):
            assert len(row) == expected['width']
            for x,(old,new) in enumerate(zip(before,row)):
                count += 1
                if old != new and len(differences) < 12:
                    differences.append((expected['name'],x,y,old,new))
        assert len(expected['expectedMotion']) == len(actual['motion'])
        for old,new in zip(expected['expectedMotion'],actual['motion']):
            assert abs(old-new) <= 1e-6, (expected['name'],'motion',old,new)
        before, after = expected['expectedPanels'], actual['panels']
        assert (before is None) == (after is None), (expected['name'], 'panels')
        if before is not None:
            assert len(before) == len(after) == 2
            assert all(abs(a-b) <= 1e-6 for a,b in zip(before, after)), (expected['name'], 'panels')
    assert not differences, differences
    print(f'Exact UI match: {count:,} cells in {len(frames)} frames')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--freeze',action='store_true')
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    path=REFERENCE/'snapshots.json.gz'
    if args.freeze: freeze(path,cases())
    load_reference(path,cases())
    manifest=json.loads((REFERENCE/'provenance.json').read_text())
    for name,digest in manifest['sha256'].items():
        assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest() == digest, f'Changed fixture source: {name}'
    print('Approved bilingual fixture inputs and provenance verified')


if __name__ == '__main__': main()
