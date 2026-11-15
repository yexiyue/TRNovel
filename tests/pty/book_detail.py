# /// script
# requires-python = ">=3.11"
# dependencies = ["pyte>=0.8.2,<0.9"]
# ///
"""Mouse-only network resume through a loopback book source and isolated HOME."""
import hashlib
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
sys.dont_write_bytecode = True
from settings_tabs import App, OUT, find, open_settings

ready = threading.Event()
requests = []

class Pages(BaseHTTPRequestHandler):
    def do_GET(self):
        requests.append(self.path)
        if self.path == '/book':
            ready.wait(15)
            text = '<h1>鼠标续读验收</h1><a id="toc" href="/toc">目录</a><p id="intro">本地测试书籍简介</p>'
        elif self.path == '/toc':
            text = '<a class="chapter" href="/first">第一章</a><a class="chapter" href="/second">第二章</a>'
        else:
            text = '<div class="content">' + ('第二章：鼠标成功续读。' if self.path == '/second' else '第一章：不应从头开始。') + '</div>'
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.end_headers()
        self.wfile.write(text.encode())

    def log_message(self, *_):
        pass

server = ThreadingHTTPServer(('127.0.0.1', 0), Pages)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = f'http://127.0.0.1:{server.server_port}'
url = base + '/book'

def css(select, attr=None):
    return dict(via='css', select=select, extract={'attr': attr} if attr else 'text')

def seed(home):
    data = home / '.trnovel/data'
    (data / 'network').mkdir(parents=True)
    source = dict(schema='trnovel-booksource/v2', name='本地验收源', url=base,
                  bookInfo=dict(name=css('h1'), tocUrl=css('#toc', 'href'), intro=css('#intro')),
                  toc=dict(list=dict(via='css', select='a.chapter'), name=css('a'), url=css('a', 'href')),
                  content=dict(value=css('.content')))
    info = dict(name='鼠标续读验收', author='测试', cover='', intro='', kind='',
                last_chapter='第二章', toc_url=base + '/toc', word_count='', book_url=url)
    cache = dict(info, book_source_url=base, book_source_name='本地验收源', current_chapter=1,
                 current_chapter_name='第二章', line_percent=0.0, chapter_percent=50.0)
    (data / 'book_sources.json').write_text(json.dumps({'book_sources': [source]}))
    (data / 'network' / (hashlib.md5(url.encode()).hexdigest() + '.json')).write_text(json.dumps(cache))
    item = dict(type='Network', current_chapter='第二章', last_read_at='2026-11-15T12:00:00+08:00',
                percent=50.0, title='鼠标续读验收', book_source='本地验收源')
    (data / 'history.json').write_text(json.dumps(dict(local_path=None, histories=[[url, item]])))

a = App('[appearance]\nshow_header = false\n', seed=seed)
try:
    a.click(*find(a, '历史记录'))
    a.click(*find(a, '鼠标续读验收'))
    assert '继续阅读' in a.text(), a.text()
    a.click(*find(a, '继续阅读'))
    assert '小说详情' in a.text() and '加载中' in a.text(), a.text()
    ready.set()
    a.pump(1)
    a.move(0, 0)
    x, y = find(a, '继续阅读')
    assert y > 1, a.text()
    before = a.screen.buffer[y][x].reverse
    a.move(x, y)
    assert before != a.screen.buffer[y][x].reverse, a.text()
    a.save('network-detail')
    a.click(x, y)
    a.pump(.8)
    assert '第二章' in a.text(), a.text()
    assert '/second' in requests and '/first' not in requests, requests
    a.save('network-resumed')
    a.send('b')
    a.pump(.5)
    assert '继续阅读' in a.text(), a.text()
    a.send('\r', .8)
    assert '第二章' in a.text(), a.text()
finally:
    ready.set()
    a.close()
    server.shutdown()
    server.server_close()

# Neither header mode should have a second standalone settings title.
for visible in [True, False]:
    a = App(f'[appearance]\nshow_header = {str(visible).lower()}\n')
    try:
        open_settings(a)
        assert not any(row.strip() == '设置' for row in a.rows()), a.text()
        a.save('settings-header-' + str(visible).lower())
    finally:
        a.close()
print('PASS: inline resume, hover, loading guard, keyboard fallback, settings headings in both header modes')
