#!/usr/bin/env python3
"""Exercise the linked executable in a real PTY, with demo journeys and isolated native UI preferences."""
import fcntl
from functools import partial
import hashlib
import json
import os
import re
from pathlib import Path
import select
import signal
import struct
import subprocess
import termios
import tempfile
import time
import unicodedata

ROOT = Path(__file__).resolve().parents[1]


def frames(output):
    # ponytail: capture only the CUP/clear/SGR text commands emitted by this renderer.
    # Use a full emulator if tests add arbitrary terminal applications or emoji input.
    cells, snapshots, x, y = {}, [], 0, 0
    for part in re.split(r'(\x1b\[[0-?]*[ -/]*[@-~])', output.decode()):
        if part.startswith('\x1b['):
            values, command = part[2:-1], part[-1]
            if command == 'H':
                row, column = (values or '1;1').split(';')
                x, y = int(column) - 1, int(row) - 1
            elif command == 'J' and values == '2':
                cells.clear()
            elif command == 'm' and values in ('', '0') and cells:
                width = max(point[0] for point in cells) + 1
                height = max(point[1] for point in cells) + 1
                snapshots.append('\n'.join(''.join(cells.get((xx, yy), ' ') for xx in range(width))
                                           for yy in range(height)))
            continue
        for character in part:
            if character == '\r':
                x = 0
            elif character == '\n':
                y += 1
            elif character.isprintable():
                width = 2 if unicodedata.east_asian_width(character) in 'WF' else 1
                if cells.get((x, y)) == '':
                    cells[x - 1, y] = ' '
                if unicodedata.east_asian_width(cells.get((x, y), ' ')[:1] or ' ') in 'WF':
                    cells[x + 1, y] = ' '
                cells[x, y] = character
                if width == 2:
                    cells[x + 1, y] = ''
                x += width
    return snapshots


def saved_files(directory):
    return {name: hashlib.sha256((directory / name).read_bytes()).hexdigest()
            if (directory / name).exists() else None for name in ('ui.json', 'library.json')}


def session(actions, options=(), exit_code=0, marks=None, binary=None,
            demo=True, environment=None, size=(140, 40)):
    master, slave = os.openpty()
    original = termios.tcgetattr(slave)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', size[1], size[0], 0, 0))
    pid = os.fork()
    if pid == 0:
        os.close(master)
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        for target in (0, 1, 2):
            os.dup2(slave, target)
        os.close(slave)
        os.environ['TERM'] = 'xterm-256color'
        os.environ['COLORTERM'] = 'truecolor'
        os.environ['TERM_PROGRAM'] = 'muse-test'
        os.environ['LC_ALL'] = 'ko_KR.UTF-8'
        os.environ.pop('NO_COLOR', None)
        executable = binary or ROOT / 'dist/muse'
        os.environ.update(environment or {})
        modes = ['--demo'] if demo else []
        child = subprocess.Popen([executable, *modes, '--plain-icons', *options], cwd=executable.parent)
        os.write(1, f'PTY_CHILD={child.pid}\n'.encode())
        code = child.wait()
        restored = termios.tcgetattr(0) == original
        os.write(1, b'PTY_RESTORED\n' if restored else b'PTY_NOT_RESTORED\n')
        os._exit(code if restored else 99)
    output, index, status = bytearray(), 0, None
    started = time.monotonic()
    try:
        while time.monotonic() - started < 8:
            elapsed = time.monotonic() - started
            if index < len(actions) and elapsed >= actions[index][0]:
                action = actions[index][1]
                if isinstance(action, bytes):
                    os.write(master, action)
                elif action[0] == 'resize':
                    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', action[2], action[1], 0, 0))
                elif action[0] == 'signal':
                    native_pid = int(re.search(rb'PTY_CHILD=(\d+)', output).group(1))
                    os.kill(native_pid, action[1])
                elif action[0] == 'mark':
                    marks[action[1]] = len(output)
                index += 1
            if select.select([master], [], [], .02)[0]:
                chunk = os.read(master, 65536)
                output.extend(chunk)
                if b'\x1b[6n' in chunk:
                    os.write(master, b'\x1b[1;1R')  # Reply to the terminal cursor query.
            exited, result = os.waitpid(pid, os.WNOHANG)
            if exited:
                status = result
                while select.select([master], [], [], 0)[0]:
                    chunk = os.read(master, 65536)
                    if not chunk:
                        break
                    output.extend(chunk)
                break
        assert status is not None and os.waitstatus_to_exitcode(status) == exit_code, ('exit status/timeout', status, bytes(output[-200:]))
        assert b'PTY_RESTORED' in output, 'raw terminal mode was not restored'
        for sequence in (b'\x1b[?1049h', b'\x1b[?1049l', b'\x1b[?25h', b'\x1b[?2004l', b'\x1b[?1006l'):
            assert sequence in output, ('missing terminal setup/cleanup', sequence)
        return bytes(output)
    finally:
        if status is None:
            os.killpg(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        os.close(master)
        os.close(slave)


def preferences_journey():
    # No HOME override, authorization or playback input; both owners use this isolated directory.
    with tempfile.TemporaryDirectory(prefix='muse-preferences-') as directory:
        environment = {'MUSE_STATE_DIR': directory}
        preferences = Path(directory) / 'ui.json'
        options = ('--reduced-motion',)
        session([(0.5, b'2['), (1.0, b'q')], options=options, demo=False, environment=environment)
        saved = preferences.read_bytes()
        settings = json.loads(saved)
        assert settings['theme'] == 'graphite' and not settings['left_open']
        restored = session([(0.8, b'q')], options=options, demo=False, environment=environment)
        assert preferences.read_bytes() == saved, 'relaunch lost UI preferences'
        assert 'MUSIC' not in frames(restored)[-1].splitlines()[1], 'saved collapsed sidebar was not restored'
        session([(0.8, b'q')], options=(*options, '--theme', 'linen', '--show-left'),
                demo=False, environment=environment)
        settings = json.loads(preferences.read_bytes())
        assert settings['theme'] == 'linen' and settings['left_open'], 'CLI override lost'
        corrupt = b'{"version":999}'
        preferences.write_bytes(corrupt)
        warning = session([(0.4, b'T'), (0.9, b'q')], options=options, demo=False, environment=environment)
        assert preferences.read_bytes() == corrupt, 'corrupt preferences overwritten'
        assert '설정을 읽을 수 없어 기본값을 사용합니다' in '\n'.join(frames(warning))
        demo = session([(0.7, b'q')], options=options, environment=environment)
        assert preferences.read_bytes() == corrupt
        assert '설정을 읽을 수 없어' not in '\n'.join(frames(demo)), 'demo read normal preferences'


def journeys(session):
    paste = lambda text: b'\x1b[200~' + text.encode() + b'\x1b[201~'
    output = session([(0.5, b' '), (0.7, b'/'), (0.8, paste('유나')), (1.1, b'\r'),
                      (1.3, b'+'), (1.45, b'p'), (1.6, b'N'), (1.7, paste('확인 목록')),
                      (1.9, b'\r'), (2.4, b'q')])
    text = re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]', b'', output).decode()
    for expected in ('MUSIC', 'Soft Signal', '유나', '확인 목록'):
        assert expected in text, ('missing rendered text', expected)
    mouse = lambda code, x, y, released=False: f'\x1b[<{code};{x};{y}{"m" if released else "M"}'.encode()
    resized = session([(0.5, mouse(0, 19, 2)), (0.7, mouse(0, 3, 2)),
                       (0.9, mouse(0, 124, 38)), (1.0, mouse(32, 140, 38)), (1.1, mouse(0, 140, 38, True)),
                       (1.3, ('resize', 80, 24)), (1.6, ('resize', 60, 20)),
                       (1.9, ('resize', 180, 44)), (2.1, b'Q'), (2.2, mouse(65, 170, 12)), (2.5, b'q')],
                      options=('--transparent', '--reduced-motion'))
    assert '80×24 이상으로 넓혀주세요'.encode() in re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]', b'', resized)
    assert b'\x1b[49m' in resized, 'terminal-owned background was replaced'
    journey = session([(0.5, b' '), (0.7, b'/'), (0.8, paste('Mira')), (1.1, b'\r'), (1.4, b'\r'),
                       (1.7, b'f'), (2.0, b'a'), (2.2, b'\r'), (2.4, paste('테스트 여정')), (2.6, b'\r'),
                       (2.9, b'R'), (3.1, b'\x15' + paste('새 이름')), (3.3, b'\r'),
                       (3.6, b'M'), (3.8, b'\r'), (4.1, b'\r'), (4.4, b'O'), (4.6, b'jj\r'),
                       (4.8, b'\x15' + paste('0.5')), (5.0, b'\r'), (5.3, b':'), (5.5, b'jjj\r'),
                       (5.7, b'j\r'), (6.0, b'A'), (6.2, b'Q'), (6.6, b'q')],
                      options=('--256-color', '--reduced-motion'))
    screens = frames(journey)
    text = '\n'.join(screens)
    for expected in ('즐겨찾기에 추가', '새 이름', '가사 시간 (초)', '플레이리스트 삭제'):
        assert expected in text, ('missing journey feedback', expected)
    assert b'38;5;' in journey and b'38;2;' not in journey and b'48;2;' not in journey
    assert '새 이름' not in screens[-1] and 'Evening' in screens[-1], 'deleted collection remains visible'
    session([(0.6, b'\x03')])
    for requested in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        session([(0.6, ('signal', requested))], exit_code=128 + requested)
    marks = {}
    session([(0.5, b' '), (3.2, ('mark', 'start')), (3.8, ('mark', 'end')), (4.0, b'q')],
            options=('--reduced-motion',), marks=marks)
    assert marks['start'] == marks['end'], 'idle scene keeps writing terminal frames'
    marks = {}
    session([(0.5, b' '), (0.7, b'o'), (1.0, b'Q'),
             (4.2, ('mark', 'start')), (4.8, ('mark', 'end')), (5.0, b'q')],
            marks=marks, size=(140, 80))
    assert marks['start'] == marks['end'], 'normal-motion tall detail/queue view never idles'
    preferences_journey()


def main():
    with tempfile.TemporaryDirectory(prefix='muse-pty-') as directory:
        directory = Path(directory)
        for name in ('ui.json', 'library.json'):
            (directory / name).write_bytes(b'isolated state must remain untouched')
        before = saved_files(directory)
        journeys(partial(session, environment={'MUSE_STATE_DIR': str(directory)}))
        assert saved_files(directory) == before, 'demo changed isolated library/preferences'
    print('Linked PTY: Korean input, mouse/drag/resize, transparency, both idle modes, preferences, Ctrl-C/INT/TERM/HUP and restoration passed')


if __name__ == '__main__':
    main()
