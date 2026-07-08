# TRNovel 文档演示 · 快捷键面板
# 录制(从本目录):  vhs shortcuts.tape
# 在任意页面按 i 唤出当前页支持的快捷键浮层,再按 i / Esc 关闭

Output "../src/assets/guides/shortcuts.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
Env HOME "/tmp/trn-demo-home"

Type "trn -l /tmp/trn-demo-home/books"  Enter
Sleep 1800ms

# 打开星河彼岸进入阅读
Down  Sleep 400ms
Down  Sleep 400ms
Down  Sleep 500ms
Enter
Sleep 1300ms

# i 唤出快捷键浮层
Type "i"
Sleep 1300ms
Screenshot "/tmp/verify-shortcuts.png"

# 再按 i 关闭
Type "i"
Sleep 800ms
