#!/usr/bin/env python3
"""Validate a clean release; publish or update the tap only with explicit flags."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile

from build import ROOT, version

REPO = 'songhyun-k/muse'
TAP = 'songhyun-k/homebrew-tap'


def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def formula_text(checksum):
    assert re.fullmatch(r'[0-9a-f]{64}', checksum), 'Invalid archive checksum'
    return (ROOT / 'packaging/muse.rb.in').read_text().replace('@VERSION@', version()).replace('@SHA256@', checksum)


def validate():
    assert re.fullmatch(r'\d+\.\d+\.\d+', version()), 'Expected a stable semantic version'
    assert not output('git', 'status', '--porcelain'), 'Commit changes before publishing'
    head = output('git', 'rev-parse', 'HEAD')
    assert output('git', 'ls-remote', 'origin', 'refs/heads/main').split()[0] == head, 'Publish current remote main only'
    report = json.loads((ROOT / 'dist/release.json').read_text())
    assert report['commit'] == head and not report['dirty'], 'Rebuild this clean commit'
    assert report['version'] == version() and report['architecture'] == 'arm64'
    assert report['relocatedLaunch'] and report['archive'] == 'muse-macos-arm64.tar.gz'
    archive = ROOT / 'dist' / report['archive']
    assert digest(archive) == report['archiveSha256'], 'Archive changed after inspection'
    assert digest(ROOT / 'dist/muse') == report['sha256'], 'Executable changed after inspection'
    with tarfile.open(archive) as package:
        assert hashlib.sha256(package.extractfile('muse').read()).hexdigest() == report['sha256']
    source = ROOT / 'dist/muse-source.tar.gz'
    expected = set(output('git', 'ls-files').splitlines())
    with tarfile.open(source) as package:
        names = set()
        for member in package:
            assert member.isfile() and member.name.startswith('muse/'), 'Invalid source entry'
            name = member.name.removeprefix('muse/')
            assert name in expected and name not in names, 'Unexpected source entry'
            assert package.extractfile(member).read() == (ROOT / name).read_bytes(), 'Source archive is stale'
            names.add(name)
        assert names == expected, 'Source archive is incomplete'
    formula = formula_text(digest(archive))
    (ROOT / 'dist/muse.rb').write_text(formula)
    assets = [archive, source, ROOT / 'dist/release.json', ROOT / 'dist/muse.rb']
    checksums = ROOT / 'dist/SHA256SUMS'
    checksums.write_text(''.join(f'{digest(path)}  {path.name}\n' for path in assets))
    return head, assets + [checksums], formula


def update_tap():
    # Pin the published bytes, never a local rebuild of the same version.
    release = json.loads(output('gh', 'api', f'repos/{REPO}/releases/tags/v{version()}'))
    assert not release['draft'] and not release['prerelease'] and release['immutable']
    archive = next(asset for asset in release['assets'] if asset['name'] == 'muse-macos-arm64.tar.gz')
    formula = formula_text(archive['digest'].removeprefix('sha256:'))
    endpoint = f'repos/{TAP}/contents/Formula/muse.rb'
    current = json.loads(output('gh', 'api', endpoint))
    previous = base64.b64decode(current['content']).decode()
    if previous == formula:
        return
    previous_version = re.search(r'/download/v(\d+\.\d+\.\d+)/', previous)[1]
    assert tuple(map(int, previous_version.split('.'))) <= tuple(map(int, version().split('.'))), 'Refusing to downgrade tap'
    payload = dict(message=f'Release muse {version()}', sha=current['sha'],
                   content=base64.b64encode(formula.encode()).decode(), branch='main')
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json') as file:
        json.dump(payload, file); file.flush()
        subprocess.run(['gh', 'api', '--method', 'PUT', endpoint, '--input', file.name,
                        '--jq', '.commit.html_url'], cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--publish', action='store_true')
    parser.add_argument('--update-tap', action='store_true')
    args = parser.parse_args()
    if args.update_tap and not args.publish:
        update_tap()
        return
    head, assets, formula = validate()
    print(f'Verified muse {version()} at {head}')
    if args.publish:
        notes = f"""Apple Music in your terminal. macOS 14+ on Apple Silicon.

```sh
brew install songhyun-k/tap/muse
muse
```

Sign in to the Music app on your Mac. Press `,` for Settings or `?` for help.

The archive includes the executable and license notices. No extra runtime is required.
`SHA256SUMS` and `release.json` identify the inspected artifacts. Packages use ad-hoc signing.
"""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.md') as file:
            file.write(notes); file.flush()
            subprocess.run(['gh', 'release', 'create', f'v{version()}', *map(str, assets),
                            '--repo', REPO, '--target', head, '--title', f'muse {version()}',
                            '--notes-file', file.name, '--latest'], cwd=ROOT, check=True)
    if args.update_tap:
        update_tap()


if __name__ == '__main__':
    main()
