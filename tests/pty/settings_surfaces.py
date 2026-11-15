# /// script
# requires-python = ">=3.11"
# dependencies = ["pyte>=0.8.2,<0.9"]
# ///
"""Real PTY settings acceptance with isolated HOME and a protocol fixture worker."""
import json
import sys
sys.dont_write_bytecode = True
import os
import subprocess
import tempfile
from pathlib import Path
from settings_tabs import App, OUT, find, resize, find_on_row, open_settings


def click(a, word):
    a.click(*find(a, word))


def control(a, label, word):
    # Theme preview cards behind the modal also contain labels such as 正文.
    # Restrict hits to the modal's actual horizontal content region.
    left = 0
    for title in ('新建方案', '编辑方案', '阅读设置', '听书设置'):
        if title in a.text():
            left = find(a, title)[0]
            break
    for y in range(a.screen.lines):
        text = ''.join(a.screen.buffer[y][x].data for x in range(left, a.screen.columns))
        if label in text and word in text:
            cells = [(x, a.screen.buffer[y][x].data) for x in range(left, a.screen.columns)]
            content, positions = '', []
            for x, value in cells:
                content += value
                positions.extend([x] * len(value))
            a.click(positions[content.index(word, content.index(label) + len(label))], y)
            return
    raise AssertionError((label, word, a.text()))


def hover(a, word):
    x, y = find(a, word)
    before = [(a.screen.buffer[y][i].fg, a.screen.buffer[y][i].bg, a.screen.buffer[y][i].reverse) for i in range(x, min(x + 2, a.screen.columns))]
    a.move(x, y)
    after = [(a.screen.buffer[y][i].fg, a.screen.buffer[y][i].bg, a.screen.buffer[y][i].reverse) for i in range(x, min(x + 2, a.screen.columns))]
    assert before != after, (word, before, after, a.text())


def reader(a):
    click(a, 'sample.txt')
    a.send('\r', .6)
    assert '交互验收' in a.text(), a.text()


checks = []
with tempfile.TemporaryDirectory(prefix='trnovel-pty-worker-') as directory:
    worker = Path(directory) / 'worker'
    subprocess.run(['rustc', '--edition=2024', 'src/tts/client/fixture.rs', '-o', str(worker)], env=dict(os.environ, FIXTURE_PROTOCOL_VERSION='7'), check=True)
    a = App(f'[tts]\nprogram = {json.dumps(str(worker))}\n', novel=True)
    try:
        reader(a)
        a.send('o')
        assert '阅读设置' in a.text(), a.text()
        hover(a, '翻页')
        assert '每行字数' in a.text()
        a.send('\t')
        assert '阅读设置' in a.text() and '每行字数' in a.text()
        control(a, '每行字数', '+')
        a.save('reader-layout')
        control(a, '边框', '单线')
        assert '边框' in a.text()
        a.send('\x1b')
        assert '阅读设置' in a.text(), a.text()
        click(a, '界面')
        control(a, '显示标题', '开')
        a.save('reader-interface')
        a.send('\x1b')
        assert '阅读设置' not in a.text()
        a.send('o')
        assert '关' in a.text()
        resize(a, 52, 18)
        click(a, '排版')
        a.send('\t\x1b[F')
        assert '目录占宽' in a.text(), a.text()
        a.send('\x1b[H')
        assert '每行字数' in a.text(), a.text()
        a.save('reader-compact')
        checks.append('Reader tabs/controls/hover, nested Esc, reopen and 52x18 focus reveal')
        a.send('\x1b')
        resize(a, 120, 29)
        a.send('t', 1)
        assert '听书设置' in a.text(), a.text()
        a.pump(1)
        hover(a, '声音')
        assert '速度' in a.text()
        control(a, '速度', '+')
        a.save('tts-playback')
        click(a, '声音')
        control(a, '模型', 'MOSS Nano')
        assert '搜索名称或 ID' in a.text(), a.text()
        hover(a, 'Qwen Custom')
        a.send('absent')
        assert '没有匹配结果' in a.text(), a.text()
        a.send('\x7f' * 6 + 'qwen')
        assert 'Qwen Custom' in a.text() and 'MOSS Nano' not in a.text(), a.text()
        click(a, 'Qwen Custom')
        a.pump(.5)
        assert '听书设置' in a.text() and '搜索名称或 ID' not in a.text()
        a.save('tts-sound')
        control(a, '音色', 'C')
        a.send('\x1b')
        assert '听书设置' in a.text(), a.text()
        click(a, '高级')
        resize(a, 52, 18)
        a.send('\t\x1b[F')
        assert '释放资源' in a.text(), a.text()
        # Arrows on an action must neither shut down the worker nor activate it.
        requests = worker.with_suffix('.requests.jsonl').read_text()
        a.send('\x1b[D\x1b[C')
        assert worker.with_suffix('.requests.jsonl').read_text() == requests
        a.save('tts-compact')
        resize(a, 52, 14)
        a.send('\x1b[H')
        assert '合成设备' in a.text(), a.text()
        a.send('\x1b[F')
        assert '释放资源' in a.text(), a.text()
        a.save('tts-short')
        a.send('\x1b')
        checks.append('TTS tabs, bounded controls, live filter/empty state/hover, nested cancel and compact actions')
    finally:
        a.close()

# Theme editor entered through the same global settings surface.
a = App()
try:
    open_settings(a)
    click(a, '外观')
    click(a, '配色方案')
    a.send('\x1b[C\x1b[D')
    assert '配色方案' in a.text() and '外观与主题' not in a.text()
    control(a, '配色方案', '›')
    assert '外观与主题' in a.text(), a.text()
    before = a.config()
    a.send('n')
    assert '新建方案' in a.text(), a.text()
    a.send('PTY')
    control(a, '基于', '▾')
    assert '选择基础主题' in a.text(), a.text()
    a.send('\x1b')
    assert '新建方案' in a.text()
    # Tab reaches the color fields; a partial hex must remain visible.
    a.send('\t#12')
    assert '输入 #12' in a.text(), a.text()
    a.save('theme-partial-hex')
    # Click the first color value, then hover a named swatch.
    control(a, '正文', '▾')
    hover(a, '纯白')
    a.save('theme-swatches')
    click(a, '纯白')
    assert '#ffffff' in a.text(), a.text()
    resize(a, 52, 18)
    a.send('\x1b[B' * 14)
    assert '占位' in a.text(), a.text()
    a.send('\x1b[<64;30;7M' * 6)
    a.send('\x1b[C')
    assert '占位' in a.text(), a.text()
    a.save('theme-compact')
    click(a, '取消')
    assert '新建方案' not in a.text()
    assert a.config() == before
    resize(a, 120, 29)
    a.send('n')
    a.send('PTY')
    control(a, '基于', '▾')
    click(a, '米黄纸')
    a.send('\t#123456')
    assert any(a.screen.buffer[y][x].data == '█' and a.screen.buffer[y][x].fg == '123456' for y in range(a.screen.lines) for x in range(a.screen.columns)), a.text()
    a.save('theme-custom-hex')
    hover(a, '取消')
    click(a, '保存')
    assert '新建方案' not in a.text()
    saved = next(t for t in a.config()['appearance']['custom_themes'] if t['name'].endswith('PTY'))
    assert saved['colors'] == {'fg': '#123456'}, saved
    assert 'tokyo' not in saved['base'], saved
    checks.append('Theme base picker, swatch hover/click, visible partial hex, compact reveal, save/cancel persistence')
finally:
    a.close()
(OUT / 'surfaces-results.json').write_text(json.dumps(checks, ensure_ascii=False, indent=2))
print(json.dumps(checks, ensure_ascii=False, indent=2))
