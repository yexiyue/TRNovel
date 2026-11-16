# /// script
# requires-python = ">=3.11"
# dependencies = ["pyte>=0.8.2,<0.9"]
# ///
"""Actual pointer capture, resize cancellation, reflow and persistence in a PTY."""
import re
import sys
sys.dont_write_bytecode = True
from settings_tabs import App, OUT, find, find_on_row, resize

NOVEL = '第一章 拖动验收\n' + ''.join(f'P{i:04d} ' + ('这是验证阅读锚点的长短不一原文。' * (1 + i % 7)) + '\n' for i in range(350))

def open_reader(a):
    a.click(*find(a, 'sample.txt'))
    a.pump(.7)
    assert '第一章' in a.text(), a.text()

def percentage(a):
    return a.config().get('reader', {}).get('toc_width_percent', 50)

def divider(a):
    available = a.screen.columns - 1
    columns = min(available - 30, max(18, int(available * percentage(a) / 100 + .5)))
    return columns + 1, 10

def pointer(a, kind, x, y):
    a.send(f'\x1b[<{kind};{x+1};{y+1}M')

def release(a, x, y):
    a.send(f'\x1b[<0;{x+1};{y+1}m')
    a.pump(.6)

def first_marker(a, x):
    body = '\n'.join(''.join(a.screen.buffer[y][col].data for col in range(x + 2, a.screen.columns)) for y in range(2, a.screen.lines - 2))
    hits = re.findall(r'P(\d{4})', body)
    assert hits, a.text()
    return int(hits[0])

a = App('[appearance]\nshow_header=false\nconfirm_exit=false\n', novel=NOVEL)
try:
    open_reader(a)
    a.save('initial-panes')
    # Read in the middle, then return to the preview without selecting a chapter.
    a.send('\t' + '\x1b[6~' * 4)
    a.send('\t')
    x, y = divider(a)
    assert a.screen.buffer[y][x].data == '│', a.text()
    original = first_marker(a, x)
    config = a.config()
    before = (a.screen.buffer[y][x].fg, a.screen.buffer[y][x].bold)
    a.move(x, y)
    assert before != (a.screen.buffer[y][x].fg, a.screen.buffer[y][x].bold), a.text()
    assert a.config() == config
    assert '拖动调整' in a.text(),a.text()
    a.save('hover')
    pointer(a, 0, x, y)
    for delta in [12, 20, -10, 6, 0]:
        pointer(a, 32, x + delta, y)
        # Look past the 1-row hover hint and keep the same source paragraph nearby.
    release(a, x, y)
    a.move(0, 0)
    assert percentage(a) == 50, a.config()
    assert abs(first_marker(a, x) - original) <= 1, (original, first_marker(a, x), a.text())
    # Capture keeps working far outside the moving handle and clamps both sides.
    pointer(a, 0, x, y)
    pointer(a, 32, 119, y)
    pointer(a, 32, 1, y)
    release(a, 5, 5)  # Release over the directory, not over the handle.
    a.move(0,0)
    assert abs(first_marker(a,divider(a)[0])-original)<=1,a.text()
    assert percentage(a) < 25, a.config()
    saved = percentage(a)
    a.move(90, y)
    a.pump(.6)
    assert percentage(a) == saved
    a.save('dragged')
    # Resize interrupts an active drag without changing the saved preference.
    x, y = divider(a)
    pointer(a, 0, x, y)
    resize(a, 120, 26)  # Height-only changes also invalidate the active drag.
    pointer(a, 32, 80, y)
    release(a, 80, y)
    assert percentage(a) == saved, a.config()
    pointer(a, 0, x, y)
    resize(a, 90, 25)
    pointer(a, 32, 80, y)
    release(a, 80, y)
    assert percentage(a) == saved, a.config()
    resize(a, 40, 18)
    assert not re.search(r'P\d{4}', a.text()), a.text()
    a.save('narrow-directory')
    a.send('\t')
    assert re.search(r'P\d{4}', a.text()), a.text()
    a.save('narrow-body')
    resize(a, 120, 29)
    # The anchor also survives preview/full-body remounts and a hidden preview.
    a.send('\t')
    before_narrow=first_marker(a, divider(a)[0])
    resize(a,40,18)
    resize(a,100,26)
    assert abs(first_marker(a,divider(a)[0])-before_narrow)<=1, a.text()
    a.send('\t')
    full_marker=first_marker(a,-2)
    a.send('\t')
    assert abs(first_marker(a,divider(a)[0])-full_marker)<=1,a.text()
    a.send('\t')
    resize(a,120,29)
    # The numeric setting and explicit default action use the same preference.
    a.send('o')
    label_x, row = find(a, '目录占宽')
    a.click(*find_on_row(a, '[+]', row))
    a.pump(.6)
    assert abs(percentage(a) - saved - 1) < .001, a.config()
    a.click(*find(a, '目录占宽'))
    a.send('\x1b[C')
    a.pump(.6)
    assert abs(percentage(a) - saved - 2) < .001, a.config()
    a.click(*find_on_row(a, '50%', find(a, '重置目录宽度')[1]))
    a.pump(.6)
    assert percentage(a) == 50, a.config()
    a.save('percentage-settings')
    a.send('\x1b')
    assert '阅读设置' not in a.text(), a.text()
    a.send('\t')
    x, y = divider(a)
    # Switching mode also invalidates capture; moving afterwards must not edit.
    assert a.screen.buffer[y][x].data == '│', a.text()
    pointer(a, 0, x, y)
    a.send('\t')
    pointer(a, 32, x + 20, y)
    release(a, x + 20, y)
    assert percentage(a) == 50, a.config()
    a.send('\t')
    pointer(a, 0, x, y)
    pointer(a, 32, x - 10, y)
    release(a, x - 10, y)
    expected = percentage(a)
    a.save('custom-width')
    assert 25 < expected < 50, a.config()
    a.save('custom-width')
    snapshot = (a.root / '.trnovel/config.toml').read_text()
    assert 'toc_ratio' not in snapshot and 'toc_width_percent' in snapshot
    a.send('q', .5)
    assert a.exited(), a.text()
finally:
    a.close()

a = App(snapshot, novel=NOVEL)
try:
    open_reader(a)
    assert percentage(a) == expected
    x, y = divider(a)
    assert a.screen.buffer[y][x].data == '│', a.text()
    a.save('restored-width')
finally:
    a.close()
# Border styles use the existing glyphs, including corners; the outside gutter
# is a second drag target without stealing the scrollbar or body text column.
for style, vertical, corner in [('single', '│', '┌'), ('double', '║', '╔'), ('rounded', '│', '╭'), ('thick', '┃', '┏'), ('none', None, None)]:
    a = App(f'[appearance]\nshow_header=true\nconfirm_exit=false\n[reader]\nframeline="{style}"\n', novel=NOVEL)
    try:
        open_reader(a)
        x, y = divider(a)
        gutter = x - 1
        assert a.screen.buffer[y][gutter].data == ' ', a.text()
        before = a.config()
        if vertical:
            assert a.screen.buffer[y][x].data == vertical, a.text()
            corners = [(row, a.screen.buffer[row][x].data) for row in range(a.screen.lines) if a.screen.buffer[row][x].data == corner]
            assert corners, a.text()
            a.move(x, y)
            assert a.screen.buffer[y][x].bold, a.text()
            assert all(a.screen.buffer[row][x].data == glyph for row, glyph in corners)
        # Hover outside the border highlights it; borderless only paints in the gutter.
        a.move(gutter, y)
        assert a.screen.buffer[y][gutter].data == (' ' if vertical else '│'), a.text()
        assert '拖动调整' in a.text(), a.text()
        assert a.config() == before
        a.save('border-' + style + '-hover')
        a.move(x + 2, y)
        assert '拖动调整' not in a.text(), a.text()
        assert a.screen.buffer[y][gutter].data == ' '
        # Body text and TOC scrollbar must not initiate horizontal resizing.
        for target in [x if vertical is None else x + 1, gutter - 1]:
            pointer(a, 0, target, y)
            pointer(a, 32, target + 8, y)
            release(a, target + 8, y)
            assert percentage(a) == 50, a.config()
        pointer(a, 0, gutter, y)
        pointer(a, 32, gutter + 6, y)
        release(a, gutter + 6, y)
        assert percentage(a) > 50, a.config()
        a.move(0,0)
        a.save('border-' + style + '-rest')
    finally:
        a.close()
print('PASS: border/gutter hover and capture, every frame style, corners, borderless text/scrollbar isolation, reflow, resize cancellation and persistence')
