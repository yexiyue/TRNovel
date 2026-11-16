# /// script
# requires-python = ">=3.11"
# dependencies = ["pyte>=0.8.2,<0.9"]
# ///
"""Tooltip lifecycle and real click/keyboard dispatch to a recording OS launcher (macOS)."""
import json
import os
import sys
import time
from pathlib import Path
sys.dont_write_bytecode = True
from settings_tabs import App, find, find_on_row, open_settings, resize

if sys.platform != 'darwin':
    print('SKIP: launcher recording fixture uses the macOS open interface')
    sys.exit(0)
original_path = os.environ['PATH']
def seed(root):
    for name in ['data', 'cache']:
        (root / '.trnovel' / name).mkdir()
    directory = root / 'bin'
    directory.mkdir()
    launcher = directory / 'open'
    launcher.write_text(f'#!{sys.executable}\n' + '''import json,sys
from pathlib import Path
root=Path(__file__).resolve().parent.parent
with (root/'opened.jsonl').open('a') as f: f.write(json.dumps(sys.argv[1:])+'\\n')
if (root/'fail-open').exists(): sys.exit(1)
''')
    launcher.chmod(0o755)
    os.environ['PATH'] = str(directory) + os.pathsep + original_path

a = App('[appearance]\nconfirm_exit=false\n[reader]\nshow_hint_bar=false\nshow_global_hint=false\n', seed=seed)
def launches():
    path = a.root / 'opened.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
def wait_launch(count):
    deadline = time.monotonic() + 4
    while len(launches()) < count and time.monotonic() < deadline:
        a.pump(.1)
    assert len(launches()) == count, (launches(), a.text())
    a.pump(.2)
def link(label):
    return find_on_row(a, '~/', find(a, label)[1])
try:
    open_settings(a)
    a.click(*find(a, '数据'))
    a.move(0, 0)
    assert '本地文件与目录' in a.text(), a.text()
    assert a.text().count('config.toml') == 1, a.text()
    assert 'Tab 切换焦点' not in a.text(), a.text()
    a.save('settings-links')
    config = a.config()
    a.click(*find(a, '配置文件'))
    assert not launches()
    x,y = link('配置文件')
    assert a.screen.buffer[y][x].underscore, a.text()
    a.send(f'\x1b[<35;{x+1};{y+1}M', .08)
    assert '定位配置文件' not in a.text(), a.text()
    a.pump(.4)
    assert '定位配置文件' in a.text(), a.text()
    assert a.config() == config and not launches()
    a.save('link-hover')
    a.send('\x1b[C')
    assert '定位配置文件' not in a.text(), a.text()
    assert not launches()
    a.move(x,y)
    a.pump(.4)
    assert '定位配置文件' in a.text(), a.text()
    a.move(0,0)
    assert '定位配置文件' not in a.text(), a.text()
    a.click(x,y)
    a.pump(.4)
    wait_launch(1)
    a.save('link-click')
    assert '定位配置文件' not in a.text(), a.text()
    assert launches() == [['-R', '--', str(a.root/'.trnovel/config.toml')]], (launches(), (x,y), a.text())
    a.send('\r')
    wait_launch(2)
    assert len(launches()) == 2
    a.send('\x1b[C') # Adjustment must never launch a link.
    assert len(launches()) == 2
    a.click(*link('数据目录'))
    a.pump(.4)
    assert launches()[-1] == ['--', str(a.root/'.trnovel/data')]
    a.click(*link('缓存目录'))
    a.pump(.4)
    assert launches()[-1] == ['--', str(a.root/'.trnovel/cache')]
    (a.root/'fail-open').touch()
    a.click(*link('配置文件'))
    a.pump(.4)
    assert '打开失败' in a.text(), a.text()
    (a.root/'fail-open').unlink()
    a.click(*link('配置文件'))
    a.pump(.4)
    assert '打开失败' not in a.text(), a.text()
    a.click(*find(a, '关于'))
    a.click(*find_on_row(a,'github',find(a,'GitHub 仓库')[1]))
    a.pump(.4)
    assert launches()[-1] == ['--','https://github.com/yexiyue/TRNovel'], launches()
    a.click(*find_on_row(a,'↗',find(a,'使用文档')[1]))
    a.pump(.4)
    assert launches()[-1] == ['--','https://yexiyue.github.io/TRNovel'], launches()
    a.save('about-links')
    a.move(0,0)
    a.click(*find(a,'阅读'))
    a.move(0,0)
    a.send('\t')
    a.move(*find(a,'每行字数'))
    a.pump(.4)
    assert '跟随窗口' in a.text(), a.text()
    a.save('field-hint')
    a.move(0,0)
    a.click(*find(a,'数据'))
    resize(a,52,18)
    assert 'config.toml' in a.text(), a.text()
    a.move(*link('配置文件'))
    a.pump(.4)
    assert '定位配置文件' in a.text(), a.text()
    a.save('compact-hint')
    # A full menu tutorial must not appear merely by hovering its header.
    resize(a,120,29)
    a.move(*find(a,'全局设置'))
    a.pump(.4)
    assert 'F10 打开菜单' in a.text(),a.text()
    key_x,key_y = find(a,'F10')
    assert a.screen.buffer[key_y][key_x].bold
    assert a.screen.buffer[key_y][key_x].fg != a.screen.buffer[key_y][key_x+4].fg
    assert 'Enter 执行' not in a.text(),a.text()
    a.save('menu-hint')
    a.click(*find(a,'全局设置'))
    assert 'F10 打开菜单' not in a.text(),a.text()
    assert 'Enter 确认' in a.text(),a.text()
    a.save('expanded-menu')
    a.send('\x1b')
finally:
    a.close()
    os.environ['PATH'] = original_path
print('PASS: delayed/dismissed hints, no duplicate footer, click/Enter links, exact launcher arguments, failure recovery and narrow layout')
