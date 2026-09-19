"""Collect license notices for the Rust dependency graph of the host build."""
import json
from pathlib import Path
import subprocess


def write_notices(root):
    rust = subprocess.check_output(['rustc', '-vV'], text=True)
    host = next(line.split(': ', 1)[1] for line in rust.splitlines() if line.startswith('host: '))
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', host,
        '--manifest-path', 'frontend/Cargo.toml'], cwd=root, text=True))
    nodes = {node['id']: node for node in metadata['resolve']['nodes']}
    seen, pending = set(), [metadata['resolve']['root']]
    while pending:
        identifier = pending.pop()
        if identifier in seen:
            continue
        seen.add(identifier)
        pending.extend(edge['pkg'] for edge in nodes[identifier]['deps'])
    blocks = ['Rust dependency notices for muse\n']
    for package in sorted(metadata['packages'], key=lambda item: (item['name'], item['version'])):
        if package['id'] not in seen or package['name'] == 'music-frontend':
            continue
        directory = Path(package['manifest_path']).parent
        files = sorted(path for path in directory.iterdir() if path.is_file()
                       and path.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE')))
        assert files, f"Missing license text: {package['name']} {package['version']}"
        blocks.append(f"\n## {package['name']} {package['version']} ({package['license']})\n")
        for path in files:
            blocks.append(f'\n{path.name}\n\n{path.read_text()}')
    destination = root / 'dist/THIRD_PARTY_LICENSES.txt'
    destination.write_text('\n'.join(blocks))
    return destination
