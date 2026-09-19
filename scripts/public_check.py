#!/usr/bin/env python3
"""Audit public source and local links; optionally export a clean tree without Git history."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
PATTERNS = {
    'private key': rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
    'GitHub token': rb'gh[pousr]_[A-Za-z0-9]{30,}',
    'JWT': rb'eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}',
    'personal absolute path': rb'/Users/(?!example/|yourname/)[A-Za-z0-9_.-]+/',
}


def source_files():
    names = subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
    return sorted({name for name in names if name and (ROOT/name).is_file()})


def findings(data):
    # Patterns are assembled here so the audit does not flag its own definitions.
    return [name for name, pattern in PATTERNS.items() if re.search(pattern, data)]


def add_public_file(archive, path, name):
    info = archive.gettarinfo(str(path), arcname=name)
    info.uid = info.gid = 0
    info.uname = info.gname = ''
    with path.open('rb') as stream:
        archive.addfile(info, stream)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--history',action='store_true')
    parser.add_argument('--export',action='store_true')
    args=parser.parse_args()
    names=source_files()
    errors=[]
    for name in names:
        path=ROOT/name
        data=path.read_bytes()
        if name != 'scripts/public_check.py':
            errors.extend((name,kind) for kind in findings(data))
        if path.suffix == '.md':
            text=data.decode()
            links=re.findall(r'\]\(([^)]+)\)',text) + re.findall(r'(?:src|srcset)="([^"]+)"',text)
            for link in links:
                link=unquote(link.split('#',1)[0].split('?',1)[0])
                if link and not re.match(r'[a-z]+:',link) and not (path.parent/link).exists():
                    errors.append((name,'broken local link: '+link))
    assert not errors, errors
    assert 'MIT License' in (ROOT/'LICENSE').read_text()
    assert not (ROOT/'tests/reference/odo.rgbz').exists()
    assert not (ROOT/'tests/reference/demo.py.gz').exists()
    print(f'Public tree: {len(names)} files; sensitive-pattern and local-link checks passed')
    if args.history:
        rows=subprocess.check_output(['git','rev-list','--objects','--all'],cwd=ROOT,text=True).splitlines()
        process=subprocess.Popen(['git','cat-file','--batch'],cwd=ROOT,stdin=subprocess.PIPE,stdout=subprocess.PIPE)
        hits=[]
        for row in rows:
            oid,_,name=row.partition(' ')
            process.stdin.write((oid+'\n').encode()); process.stdin.flush()
            header=process.stdout.readline().decode().split()
            size=int(header[2]); data=process.stdout.read(size); process.stdout.read(1)
            if header[1]=='blob': hits.extend((name,kind) for kind in findings(data))
        process.stdin.close(); process.wait()
        assert not hits, ('sensitive-pattern hits in history; values withheld',sorted(set(hits)))
        print(f'Git history: {len(rows)} objects checked; no sensitive-pattern matches')
    if args.export:
        output=ROOT/'dist/muse-source.tar.gz'
        output.parent.mkdir(exist_ok=True)
        with tarfile.open(output,'w:gz') as archive:
            for name in names:
                add_public_file(archive, ROOT/name, 'muse/'+name)
        digest=hashlib.sha256(output.read_bytes()).hexdigest()
        print(json.dumps(dict(sourceArchive=output.name,sha256=digest,containsGitHistory=False)))


if __name__ == '__main__': main()
