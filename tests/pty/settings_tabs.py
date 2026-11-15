# /// script
# requires-python = ">=3.11"
# dependencies = ["pyte>=0.8.2,<0.9"]
# ///
"""Real terminal regression; first build trn with --all-features."""
import codecs
import fcntl
import json
import os
import pty
import select
import signal
import struct
import tempfile
import termios
import time
import tomllib
from pathlib import Path
import pyte
BINARY = os.environ.get('TRNOVEL_TEST_BINARY', str(Path(__file__).resolve().parents[2] / 'target/debug/trn'))
OUT = Path(os.environ.get('TRNOVEL_TEST_ARTIFACTS', tempfile.mkdtemp(prefix='trnovel-settings-tabs-')))
OUT.mkdir(exist_ok=True)

class App:

    def __init__(self, config='', novel=False):
        self.temp = tempfile.TemporaryDirectory(prefix='trnovel-settings-pty-')
        self.root = Path(self.temp.name)
        cfg = self.root / '.trnovel'
        cfg.mkdir()
        (cfg / 'config.toml').write_text(config)
        args = [BINARY]
        if novel:
            (self.root / 'sample.txt').write_text('第一章 测试\n' + '这是用于交互验收的文字。\n' * 200)
            args += ['-l', str(self.root)]
        self.screen = pyte.Screen(120, 29)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder('utf8')('replace')
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 29, 120, 0, 0))
            child_env = os.environ.copy()
            child_env.update(HOME=str(self.root), TERM='xterm-256color', COLORTERM='truecolor')
            child_env.pop('NO_COLOR', None)
            os.execve(BINARY, args, child_env)
        self.pump(1.2)
        deadline = time.monotonic() + 12
        while ('sample.txt' if novel else '本地小说') not in self.text() and time.monotonic() < deadline:
            self.pump(0.3)

    def pump(self, seconds=0.25):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            if select.select([self.fd], [], [], max(0, until - time.monotonic()))[0]:
                try:
                    b = os.read(self.fd, 65536)
                except OSError:
                    break
                if not b:
                    break
                if b'\x1b[6n' in b:
                    os.write(self.fd, b'\x1b[1;1R')
                self.stream.feed(self.decoder.decode(b))
                with (OUT / 'terminal.raw').open('ab') as log:
                    log.write(b)

    def send(self, s, seconds=0.3):
        os.write(self.fd, s.encode())
        self.pump(seconds)

    def rows(self):
        return [''.join((self.screen.buffer[y][x].data for x in range(self.screen.columns))) for y in range(self.screen.lines)]

    def text(self):
        return '\n'.join(self.rows())

    def row(self, word):
        rows = [i for i, s in enumerate(self.rows()) if word in s]
        assert rows, (word, self.text())
        return rows[0]

    def move(self, x, y):
        self.send(f'\x1b[<35;{x + 1};{y + 1}M')

    def click(self, x, y):
        self.send(f'\x1b[<0;{x + 1};{y + 1}M\x1b[<0;{x + 1};{y + 1}m')

    def save(self, name):
        (OUT / (name + '.txt')).write_text(self.text())
        cells = [[dict(data=self.screen.buffer[y][x].data, fg=self.screen.buffer[y][x].fg, bg=self.screen.buffer[y][x].bg, reverse=self.screen.buffer[y][x].reverse) for x in range(self.screen.columns)] for y in range(self.screen.lines)]
        (OUT / (name + '.json')).write_text(json.dumps(cells, ensure_ascii=False))

    def config(self):
        return tomllib.loads((self.root / '.trnovel/config.toml').read_text())

    def exited(self):
        pid, status = os.waitpid(self.pid, os.WNOHANG)
        if pid:
            assert os.waitstatus_to_exitcode(status) == 0, self.text()
        return pid != 0

    def close(self):
        try:
            os.kill(self.pid, signal.SIGTERM)
            os.waitpid(self.pid, 0)
        except ProcessLookupError:
            pass
        except ChildProcessError:
            pass
        os.close(self.fd)
        self.temp.cleanup()

def find(a, word):
    for y in range(a.screen.lines):
        chars = []
        positions = []
        for x in range(a.screen.columns):
            s = a.screen.buffer[y][x].data
            chars.extend(s)
            positions.extend([x] * len(s))
        line = ''.join(chars)
        if word in line:
            return (positions[line.index(word)], y)
    raise AssertionError((word, a.text()))

def rowclick(a, label, value=None):
    x, y = find(a, label)
    if value is not None:
        x, y = find_on_row(a, value, y)
    a.click(x, y)

def find_on_row(a, word, y):
    chars = []
    positions = []
    for x in range(a.screen.columns):
        s = a.screen.buffer[y][x].data
        chars.extend(s)
        positions.extend([x] * len(s))
    text = ''.join(chars)
    assert word in text, (word, text)
    return (positions[text.index(word)], y)

def resize(a, width, height):
    a.screen.resize(height, width)
    fcntl.ioctl(a.fd, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
    os.kill(a.pid, signal.SIGWINCH)
    a.pump(0.6)

def open_settings(a):
    a.send('\x1b[B' * 3 + '\r', 0.7)
    assert '每行字数' in a.text(), a.text()
if __name__ == '__main__':
    a = App('[appearance]\nshow_header = false\nconfirm_exit = true\n[reader]\nshow_global_hint = false\n')
    checks = []
    try:
        open_settings(a)
        a.save('initial')
        print(a.text(), flush=True)
        x, y = find(a, '外观')
        before = a.screen.buffer[y][x].bg
        a.move(x, y)
        after = a.screen.buffer[y][x].bg
        assert before != after, (before, after, a.text())
        assert '每行字数' in a.text()
        a.save('tabs-hover')
        a.send('\x1b[C')
        assert '配色方案' in a.text(), a.text()
        a.send('\x1b[D')
        assert '每行字数' in a.text()
        checks.append('Tabs hover immediately without activating; arrow navigation follows keyboard focus')
        a.send('\x1b[<65;40;12M' * 3)
        assert '每行字数' not in a.text(), a.text()
        a.send('\t')
        assert '每行字数' in a.text(), a.text()
        a.send('\x1b[<65;40;12M' * 3)
        assert '每行字数' not in a.text(), a.text()
        a.send('\x1b[C')
        a.pump(0.7)
        assert '每行字数' in a.text(), a.text()
        assert a.config()['reader']['lines_per_row'] == 10
        a.send('\x1b[D')
        a.pump(0.7)
        assert a.config()['reader']['lines_per_row'] == 0
        a.send('\t')
        checks.append('Entering the form and resuming keyboard edits reveal the selected row after free scrolling')
        before = a.config()
        a.send('\x1b[<65;40;12M' * 3)
        assert '跟随朗读' in a.text(), a.text()
        assert a.config() == before
        a.send('\x1b[<64;40;12M' * 3)
        assert '每行字数' in a.text(), a.text()
        a.send('\x1b[<0;91;8M\x1b[<32;91;24M\x1b[<0;91;24m')
        assert '跟随朗读' in a.text(), a.text()
        assert a.config() == before
        a.send('\x1b[<64;40;12M' * 5)
        assert '每行字数' in a.text(), a.text()
        checks.append('Wheel and scrollbar drag scroll the viewport without changing focus or values')
        rowclick(a, '每行字数')
        a.pump(0.7)
        assert a.config().get('reader', {}).get('lines_per_row', 0) == 0
        x, y = find_on_row(a, '[+]', find(a, '每行字数')[1])
        a.move(x, y)
        a.save('control-hover')
        a.click(x, y)
        a.pump(0.7)
        assert a.config()['reader']['lines_per_row'] == 10, a.config()
        a.send('\x1b[C')
        a.pump(0.7)
        assert a.config()['reader']['lines_per_row'] == 11, a.config()
        checks.append('Label click only focuses; explicit plus button and focused Right adjust without hidden row halves')
        rowclick(a, '框底提示', '[开]')
        a.pump(.7)
        assert a.config()['reader']['show_hint_bar'] is False
        assert 'Tab 切换焦点' not in a.text(), a.text()
        rowclick(a, '框底提示', '[关]')
        a.pump(.7)
        assert a.config()['reader']['show_hint_bar'] is True
        checks.append('Hint toggle hides and restores the instructions without losing the focused control')

        rowclick(a, '边框', '▾')
        assert '无边框' in a.text(), a.text()
        a.save('enum-picker')
        a.send('\x1b[B')
        a.send('\x1b')
        assert a.config().get('reader', {}).get('frameline', 'single') == 'single'
        rowclick(a, '边框', '▾')
        x, y = find_on_row(a, '[+]', find(a, '每行字数')[1])
        a.click(x, y)
        a.pump(.7)
        assert '无边框' not in a.text(), a.text()
        assert a.config()['reader']['lines_per_row'] == 11, a.config()
        rowclick(a, '边框', '▾')
        x, y = find(a, '无边框')
        a.move(x, y)
        a.click(x, y)
        a.pump(0.7)
        assert a.config()['reader']['frameline'] == 'none', a.config()
        a.save('choice-applied')
        checks.append('Enum picker supports mouse hover/click, keyboard navigation and Esc cancellation')
        a.send('\x1b[F')
        assert '跟随朗读' in a.text(), a.text()
        rowclick(a, '跟随朗读', '[开]')
        a.pump(0.7)
        assert a.config()['reader']['follow_tts'] is False, a.config()
        a.save('scrolled-last')
        a.send('\t')
        a.send('\x1b[C')
        assert '配色方案' in a.text(), a.text()
        rowclick(a, '顶部栏', '[关]')
        a.pump(0.7)
        assert a.config()['appearance']['show_header'] is True
        rowclick(a, '顶部栏', '[开]')
        a.pump(0.7)
        assert a.config()['appearance']['show_header'] is False
        rowclick(a, '退出确认', '[开]')
        assert a.config()['appearance']['confirm_exit'] is False
        rowclick(a, '背景模式', '▾')
        x, y = find(a, '终端背景')
        a.click(x, y)
        assert a.config()['appearance']['background'] == 'terminal'
        checks.append('End reveals last setting; direct toggles and appearance fields persist')
        rowclick(a, '配色方案', '›')
        assert '当前:' in a.text(), a.text()
        a.send('\x1b')
        assert '配色方案' in a.text(), a.text()
        a.send('\x1b')
        a.send('\x1b[B' * 3 + '\r', 0.7)
        assert '配色方案' in a.text(), a.text()
        checks.append('Theme route and page return preserve section and cursor')
        a.click(*find(a, '键位'))
        assert '当前绑定' in a.text(), a.text()
        a.click(*find(a, '数据'))
        assert '本地目录' in a.text(), a.text()
        a.click(*find(a, '关于'))
        assert '许可证' in a.text(), a.text()
        resize(a, 52, 18)
        a.click(*find(a, '阅读'))
        a.send('\t')
        a.send('\x1b[F')
        assert '跟随朗读' in a.text(), a.text()
        a.save('compact-last')
        a.send('\x1b[H')
        assert '每行字数' in a.text(), a.text()
        config = a.config()
        a.click(0, 8)
        a.pump(0.6)
        assert a.config() == config
        a.send('\x1b[C', 0.02)
        a.send('q')
        a.pump(0.6)
        assert a.exited(), 'Q should exit directly after disabling confirmation'
        assert a.config()['reader']['lines_per_row'] == 12, a.config()
        checks.append('All tabs work; 52x18 keyboard scrolling reveals first/last controls; outside click does not mutate')
    finally:
        a.close()
    (OUT / 'results.json').write_text(json.dumps(checks, ensure_ascii=False, indent=2))
    print(json.dumps(checks, ensure_ascii=False, indent=2))
