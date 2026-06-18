# TRNovel 文档演示 · 历史记录
# 录制(从本目录):  vhs history.tape
# 依赖:演示 HOME 已有阅读历史(见 README:先跑 read.tape 等预热)

Output "../src/assets/guides/history.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env HOME "/tmp/trn-demo-home"

Type "trn -H"  Enter
Sleep 1800ms
Screenshot "/tmp/verify-history.png"

# 上下浏览历史条目
Down  Sleep 500ms
Down  Sleep 500ms
Up    Sleep 700ms
Sleep 500ms
