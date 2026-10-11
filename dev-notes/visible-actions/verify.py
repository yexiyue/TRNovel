"""Isolated PTY acceptance. Requires pyte; never reads the user's reader home.

Usage: python verify.py /absolute/path/to/trnovel [output-directory]
"""
import codecs
import fcntl
import http.server
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
from urllib.parse import parse_qs, urlsplit

import pyte

BINARY = str(Path(sys.argv[1]).resolve())
OUTPUT = Path(sys.argv[2] if len(sys.argv) > 2 else tempfile.mkdtemp(prefix="trnovel-ui-results-"))
OUTPUT.mkdir(parents=True, exist_ok=True)
ROOT = Path(tempfile.mkdtemp(prefix="trnovel-ui-home-"))
HOME = ROOT / "home"
BOOKS = ROOT / "books"
HOME.mkdir()
BOOKS.mkdir()
TEXT = "演示小说\n" + "".join(
    f"第{i}章 测试章节\n" + "".join(f"这是第{i}章第{j}段正文，用来验证工具栏和翻页。\n" for j in range(80))
    for i in range(1, 5)
)
(BOOKS / "演示小说.txt").write_text(TEXT)


class MockBooks(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlsplit(self.path)
        query = parse_qs(url.query)
        if url.path in ("/list", "/search"):
            page = int(query.get("page", ["1"])[0])
            genre = query.get("genre", ["玄幻"])[0]
            value = {"has_more": page < 2, "books": [{"name": f"{genre}小说", "bookUrl": "/book/1", "author": "测试作者"}]}
        elif url.path == "/book/1":
            value = {"name": "网络演示小说", "author": "测试作者", "intro": "隔离测试用书", "tocUrl": "/toc"}
        elif url.path == "/toc":
            value = {"chapters": [{"name": f"第{i}章 测试章节", "url": f"/chapter/{i}"} for i in range(1, 5)]}
        else:
            value = {"content": TEXT}
        body = json.dumps(value, ensure_ascii=False).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


def rule(field):
    return {"via": "json", "select": f"$.{field}"}


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), MockBooks)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{server.server_port}"
page = {
    "request": {"url": "{{base}}/list?genre={{genre}}&page={{page}}", "hasMore": rule("has_more")},
    "list": {"via": "json", "select": "$.books[*]"},
    "item": {field: rule(field) for field in ("name", "author", "bookUrl")},
}
source = {
    "schema": "trnovel-booksource/v2", "name": "隔离演示书源", "url": base,
    "http": {"fetcher": "reqwest"},
    "search": dict(page, request={"url": "{{base}}/search?key={{key}}&page={{page}}", "hasMore": rule("has_more")}),
    "explore": {"entries": [{"static": [{"title": title, "vars": {"genre": title}} for title in ("玄幻", "科幻")]}], "page": page},
    "bookInfo": {field: rule(field) for field in ("name", "author", "intro", "tocUrl")},
    "toc": {"list": {"via": "json", "select": "$.chapters[*]"}, "name": rule("name"), "url": rule("url")},
    "content": {"value": rule("content")},
}
data = HOME / ".trnovel" / "data"
data.mkdir(parents=True)
(data.parent / "keybindings.toml").write_text('[reader]\ntoggle_info = ["?"]\n')
(data / "book_sources.json").write_text(json.dumps({"book_sources": [source]}, ensure_ascii=False))


class Session:
    def __init__(self, args, width=100, height=30):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        env = dict(os.environ, HOME=str(HOME), TERM="xterm-256color", COLORTERM="truecolor")
        env.pop("NO_COLOR", None)  # Exercise the actual themed hover/selection rendering.
        self.process = subprocess.Popen([BINARY, *args], stdin=slave, stdout=slave, stderr=slave, env=env)
        os.close(slave)
        self.screen = pyte.Screen(width, height)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            self.drain(.25)
            if "‹" in self.text() or "⋯" in self.text():
                break
        self.drain(.5)

    def drain(self, seconds=.6):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if select.select([self.master], [], [], .05)[0]:
                try:
                    chunk = os.read(self.master, 65536)
                except OSError:
                    break
                self.stream.feed(self.decoder.decode(chunk))
        assert self.process.poll() is None, "Reader exited unexpectedly"

    def lines(self):
        return ["".join(self.screen.buffer[y][x].data for x in range(self.screen.columns)) for y in range(self.screen.lines)]

    def text(self):
        return "\n".join(self.lines())

    def save(self, name):
        (OUTPUT / f"{name}.txt").write_text("\n".join(line.rstrip() for line in self.lines()) + "\n")

    def save_ansi(self, name):
        # Replay the actual terminal buffer with its original colors for screenshots.
        colors = {"black": 0, "red": 1, "green": 2, "brown": 3, "blue": 4, "magenta": 5, "cyan": 6, "white": 7}
        def color(value, background=False):
            prefix = 48 if background else 38
            if value == "default":
                return "49" if background else "39"
            if value in colors:
                return str((40 if background else 30) + colors[value])
            if len(value) == 6:
                return f"{prefix};2;" + ";".join(str(int(value[i:i+2], 16)) for i in (0, 2, 4))
            return "49" if background else "39"
        chunks = ["\x1b[2J\x1b[H"]
        for y in range(self.screen.lines):
            for x in range(self.screen.columns):
                cell = self.screen.buffer[y][x]
                if not cell.data:
                    continue
                style = ["0", color(cell.fg), color(cell.bg, True)]
                if cell.bold: style.append("1")
                if cell.reverse: style.append("7")
                if cell.underscore: style.append("4")
                chunks.append(f"\x1b[{y+1};{x+1}H\x1b[{';'.join(style)}m{cell.data}")
        chunks.append("\x1b[0m\x1b[?25l")
        (OUTPUT / f"{name}.ansi").write_text("".join(chunks))

    def contains(self, text):
        deadline = time.monotonic() + 10
        while text not in self.text() and time.monotonic() < deadline:
            self.drain(.25)
        assert text in self.text(), f"Missing {text!r}:\n{self.text()}"

    def locate(self, text):
        for y in range(self.screen.lines):
            chars, columns = [], []
            for x in range(self.screen.columns):
                data = self.screen.buffer[y][x].data
                chars.append(data)
                columns.extend([x] * len(data))
            index = "".join(chars).find(text)
            if index >= 0:
                return columns[index] + 1, y + 1
        raise AssertionError(f"Could not locate {text!r}:\n{self.text()}")

    def send(self, value):
        os.write(self.master, value.encode())
        self.drain()

    def reveal(self, text):
        if text in self.text() and self.locate(text)[1] < self.screen.lines - 1:
            return
        for group in ("阅读 ▾", "听书 ▾", "本地 ▾", "频道 ▾", "书籍 ▾", "书源 ▾", "小说 ▾", "设置 ▾", "更多 ▾"):
            if group not in self.text():
                continue
            x, y = self.locate(group)
            self.send(f"\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m")
            if text in self.text():
                return
            self.send("\x1b")

    def click(self, text):
        self.reveal(text)
        x, y = self.locate(text)
        print(f"Click {text}: {x}, {y}", flush=True)
        self.send(f"\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m")

    def hover(self, text):
        self.reveal(text)
        x, y = self.locate(text)
        self.send(f"\x1b[<35;{x};{y}M")

    def resize(self, width, height):
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        self.screen.resize(height, width)
        self.process.send_signal(signal.SIGWINCH)
        self.drain()

    def quit(self):
        self.reveal("退出")
        x, y = self.locate("退出")
        # Keep the button held long enough to expose an exit-on-press race.
        self.send(f"\x1b[<0;{x};{y}M")
        assert self.process.poll() is None, "Exit ran before consuming the mouse release"
        os.write(self.master, f"\x1b[<0;{x};{y}m".encode())
        deadline = time.monotonic() + 10
        while self.process.poll() is None and time.monotonic() < deadline:
            if select.select([self.master], [], [], .05)[0]:
                try:
                    os.read(self.master, 65536)
                except OSError:
                    break
        assert self.process.wait(timeout=2) == 0, "Visible exit did not shut down cleanly"

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        os.close(self.master)


sessions = []
try:
    local = Session(["-l", str(BOOKS)])
    sessions.append(local)
    local.drain(2)
    local.contains("路径")
    more_x, more_y = local.locate("更多 ▾")
    assert more_y == 1 and more_x == local.screen.columns - 6, "More menu is not right aligned"
    local.save_ansi("local-picker")
    local.click("路径")
    local.send("\x15/does-not-exist\r")
    local.contains("路径不存在")
    assert "本地 ▾" not in local.lines()[0], "Search did not replace menus"
    local.click("取消")
    local.contains("本地 ▾")
    local.hover("帮助")
    local.contains("I")
    local.save("local-picker-hover")
    local.save_ansi("local-picker-hover")
    local.send("\x1b")
    local.click("搜索")
    local.send("没有匹配的书\r")
    local.contains("没有匹配的小说")
    local.click("清除筛选")
    local.contains("演示小说.txt")
    local.click("演示小说.txt")
    local.contains("返回")
    local.click("返回正文")
    local.contains("阅读 ▾")
    assert "阅读设置" not in local.lines()[1], "Closed controls still occupy body rows"
    local.send("\x1b[21~")  # F10 opens the first menu.
    local.contains("阅读设置")
    x, y = local.locate("目录")
    selected_cell = local.screen.buffer[y - 1][x - 1]
    assert selected_cell.reverse or selected_cell.bg != "default", "Menu selection has no visible highlight"
    local.save("reader-menu")
    local.save_ansi("reader-menu")
    local.send("\x1b[C")  # Right switches to listening.
    local.contains("听书设置")
    local.hover("更多 ▾")
    local.contains("退出")
    local.hover("阅读 ▾")
    local.contains("阅读设置")
    local.send("\x1b[B\r")  # Down, Enter opens reading settings.
    local.contains("关闭阅读设置")
    local.send("\x1b")
    local.save("reader-before-next-chapter")
    local.click("下一章")
    local.contains("第2章")
    local.click("搜索正文")
    local.send("测试")
    assert "阅读 ▾" not in local.lines()[0], "Reader search left menu visible"
    local.save("reader-search-editing")
    local.save_ansi("reader-search-editing")
    local.click("取消")
    local.contains("阅读 ▾")
    local.contains("第2章")
    local.send("s测试")
    local.click("确定")
    local.reveal("下一个匹配")
    local.contains("下一个匹配")
    local.click("下一个匹配")
    local.click("清除搜索")
    local.send("\x1b[<35;60;10M")
    local.save("reader")
    local.save_ansi("reader")
    local.click("隐藏顶部栏")
    local.drain(4)
    assert "阅读 ▾" not in local.lines()[0], "Hidden header still occupies the first row"
    local.contains("⋯")
    local.save("reader-hidden")
    local.save_ansi("reader-hidden")
    local.send("\x1b[21~")
    local.contains("阅读设置")
    local.send("\x1b")
    assert "阅读 ▾" not in local.lines()[0], "Temporary menu left the toolbar open"
    local.click("⋯")
    local.contains("显示顶部栏")
    local.send("\x1b")
    local.send("s")
    local.contains("取消")
    local.send("测试")
    local.save_ansi("reader-hidden-search")
    local.click("取消")
    assert "取消" not in local.lines()[0], "Hidden search did not close"
    local.click("⋯")
    local.click("显示顶部栏")
    local.contains("阅读 ▾")

    local.hover("帮助")
    local.contains("?")
    local.click("帮助")
    local.contains("关闭帮助")
    local.send("?")
    assert "关闭帮助" not in local.text(), "Rebound help shortcut did not close the modal"
    local.contains("阅读 ▾")
    local.click("听书设置")
    local.contains("关闭听书设置")
    local.click("关闭听书设置")
    local.click("阅读设置")
    local.contains("每页滚动")
    def setting_value(label):
        return next(line for line in local.lines() if label in line)
    local.send("\x1b[B\x1b[D")  # Down selects title; Left disables it.
    assert "关" in setting_value("显示标题"), "Down did not move settings focus"
    local.send("j\x1b[D")
    assert "关" in setting_value("段落间距"), "j did not move settings focus"
    local.send("\x1b[A\x1b[C")
    assert "开" in setting_value("显示标题"), "Up did not move settings focus"
    local.send("j\x1b[Ck")
    assert "开" in setting_value("段落间距"), "Settings adjustment did not restore spacing"
    local.send("\x1b[<65;50;15M\x1b[D")  # Wheel down selects spacing.
    assert "关" in setting_value("段落间距"), "Wheel did not move settings focus"
    local.send("\x1b[C\x1b[<64;50;15M\x1b[D")
    assert "关" in setting_value("显示标题"), "Wheel up did not move settings focus"
    local.send("\x1b[C")
    local.save("reader-settings")
    local.save_ansi("reader-settings")
    local.send("\x1b")
    local.click("帮助")
    local.click("返回")
    local.contains("阅读 ▾")  # The modal's scrim may dismiss, but navigation must stay blocked.
    local.send("\x1b")
    local.click("目录")
    local.reveal("返回正文")
    local.contains("返回正文")
    local.click("返回正文")
    local.resize(40, 30)
    for label in ("阅读设置", "帮助", "搜索正文", "下一章"):
        local.reveal(label)
        local.contains(label)
        local.send("\x1b")
    local.save("reader-narrow")
    local.resize(100, 30)
    local.click("返回")
    local.contains("演示小说.txt")
    local.click("前进")
    local.contains("返回")
    local.send("b")
    local.contains("演示小说.txt")
    local.send("g")
    local.contains("返回")
    local.reveal("返回正文")
    if "返回正文" in local.text():
        local.click("返回正文")
    local.send("\x1b")
    local.send("\x1b[<35;60;10M")
    local.save("reader-after-routing")

    network = Session(["-n"])
    sessions.append(network)
    for label in ("导入书源", "登录书源", "删除书源"):
        network.reveal(label)
        network.contains(label)
        network.send("\x1b")
    network.click("隔离演示书源")
    network.contains("频道 ▾")
    network.contains("玄幻小说")  # Wait for asynchronous source/genre initialization.
    network.hover("频道：玄幻")
    network.contains("Tab")
    network.click("频道：玄幻")
    network.contains("选择分类")
    network.click("科幻")
    network.contains("科幻小说")
    network.click("下一页")
    network.contains("第 2 页")
    network.click("下一页")
    network.contains("第 2 页")  # Disabled last-page action is a no-op.
    network.send("\x1b")
    network.click("搜索")
    network.send("qgb")
    network.click("取消")
    network.contains("科幻小说")
    network.contains("频道 ▾")
    network.click("搜索")
    network.send("测试\r")
    network.reveal("清除搜索")
    network.contains("清除搜索")
    network.click("清除搜索")
    network.contains("科幻小说")
    network.save("network-list")
    network.click("科幻小说")
    network.contains("小说 ▾")
    network.click("开始阅读")
    network.contains("返回")
    network.click("返回正文")
    network.contains("阅读 ▾")
    network.save("network-reader")
    local.click("返回")
    local.click("搜索")
    local.send("qgb")
    local.contains("确定")
    local.click("取消")
    local.contains("演示小说.txt")  # Cancel must keep the current route.
    assert "取消" not in local.lines()[0] and "返回" in local.lines()[0]
    local.click("返回")
    local.contains("更多 ▾")
    local.quit()
    network.quit()
    print(f"PASS: compact header, dropdown menus, local/network click, rebound shortcut, text search, hover, modal isolation, routing, pagination and narrow layout. Evidence: {OUTPUT}")
finally:
    for session in sessions:
        session.close()
    server.shutdown()
