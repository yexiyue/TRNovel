# TRNovel 文档演示 · 主页
# 录制方式(从本目录):  vhs home.tape
# 演示环境用隔离 HOME=/tmp/trn-demo-home,不污染真实 ~/.novel;
# 测试书库见 docs/tapes/README.md。真机录制可把路径换成自己的目录。

Output "../src/assets/guides/home.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env HOME "/tmp/trn-demo-home"

# 进入测试书库目录后启动,主页「本地书籍」即扫描此处
Hide
Type "cd /tmp/trn-demo-home/books"  Enter
Type "clear"  Enter
Show

Type "trn"  Enter
Sleep 1800ms

# 依次掠过四个入口,展示主菜单
Down  Sleep 600ms
Down  Sleep 600ms
Down  Sleep 600ms
Up    Sleep 500ms
Up    Sleep 500ms
Up    Sleep 800ms

Screenshot "/tmp/verify-home.png"
Sleep 500ms
