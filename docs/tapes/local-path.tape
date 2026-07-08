# TRNovel 文档演示 · 本地按路径扫描
# 录制(从本目录):  vhs local-path.tape
# 本地选书页按 s 可输入任意目录路径,即时切换扫描范围(路径会存为历史)。

Output "../src/assets/guides/local-path.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 55ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
Env HOME "/tmp/trn-demo-home"

Type "trn -l /tmp/trn-demo-home/books"  Enter
Sleep 1800ms
Screenshot "/tmp/verify-lp-1.png"

# s 聚焦路径框(光标在已有路径末尾),追加子目录后回车重新扫描
Type "s"
Sleep 500ms
Type "/仙侠"
Sleep 500ms
Enter
Sleep 1200ms
Screenshot "/tmp/verify-lp-2.png"
Sleep 700ms
