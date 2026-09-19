#!/usr/bin/env python3
"""Check catalog coverage, English CLI and language switching through the linked TUI."""
import json
import re
import subprocess
import tempfile
from pathlib import Path
from terminal_test import ROOT, session, frames


def main():
    catalog = json.loads((ROOT / 'frontend/assets/en.json').read_text())
    missing = set()
    for path in (ROOT / 'frontend/src').glob('*.rs'):
        source = path.read_text().split('#[cfg(test)]')[0]
        for literal in re.findall(r'\.text\("([^"\n]*)"\)', source):
            if re.search('[가-힣]', literal) and literal not in catalog:
                missing.add(literal)
    for path in (ROOT / 'backend/Sources/Backend').glob('*.swift'):
        for literal in re.findall(r'message: "([^"\n]*)"', path.read_text()):
            if re.search('[가-힣]', literal) and literal not in catalog:
                missing.add(literal)
    assert not missing, ('missing English translations', sorted(missing))
    assert all(value.strip() and not re.search('[가-힣]', value) for value in catalog.values())
    binary = ROOT / 'dist/muse'
    for language, expected in [('en', 'Usage: muse'), ('ko', '사용법: muse')]:
        result = subprocess.run([binary, '--language', language, '--help'], capture_output=True, text=True, check=True)
        assert expected in result.stdout
        if language == 'en': assert not re.search('[가-힣]', result.stdout)
    for arguments in [('--language', 'xx'), ('--theme', 'invalid', '--language', 'en'),
                      ('--fps', '0', '--language', 'en')]:
        result = subprocess.run([binary, "--demo", *arguments], capture_output=True, text=True)
        assert result.returncode != 0 and result.stderr.strip()
        assert not re.search('[가-힣]', result.stderr), result.stderr
    with tempfile.TemporaryDirectory(prefix='muse-language-') as directory:
        environment = {'MUSE_STATE_DIR': directory}
        options = ('--language', 'en', '--reduced-motion')
        output = session([(0.5, b'I'), (0.9, b'j\r'), (1.3, b'?'), (1.7, b','), (2.1, b'q')], options=options, environment=environment)
        screens = '\n'.join(frames(output))
        for text in ('Library', 'Songs', 'Language / 언어', '보관함', '단축키', ', Settings', ', 설정', '변경 사항이 즉시 적용됩니다'):
            assert text in screens, ('missing language view', text)
        assert not list(Path(directory).iterdir()), 'demo saved language preferences'
        output = session([(0.5, b'I'), (0.9, b'\r'), (1.1, b','), (1.4, b'\x1b[C'),
                          (1.7, b'jj '), (2.0, b'\x1b'), (2.3, b'q')],
                         options=('--language', 'ko', '--reduced-motion'),
                         demo=False, environment=environment)
        assert 'Changes apply immediately' in '\n'.join(frames(output))
        settings = json.loads((Path(directory) / 'ui.json').read_text())
        assert settings['language'] == 'en'
        assert settings['theme'] == 'graphite' and settings['transparent']
        restored = session([(0.7, b'q')], options=('--reduced-motion',), demo=False, environment=environment)
        assert 'Library' in '\n'.join(frames(restored)), 'saved language was not restored'
    print('Locale catalog, English/Korean help, CLI errors, interactive switching and persistence passed')


if __name__ == '__main__':
    main()
