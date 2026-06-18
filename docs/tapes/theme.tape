# TRNovel 文档演示 · 主题设置
# 录制(从本目录):  vhs theme.tape

Output "../src/assets/guides/theme.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env HOME "/tmp/trn-demo-home"

Type "trn"  Enter
Sleep 1800ms

# 主菜单移到「主题设置」(第 4 项)进入
Down  Sleep 500ms
Down  Sleep 500ms
Down  Sleep 500ms
Enter
Sleep 1200ms
Screenshot "/tmp/verify-theme-1.png"

# 选中某个主题色项,回车进入取色
Down  Sleep 600ms
Enter
Sleep 1000ms
Screenshot "/tmp/verify-theme-2.png"

# 在预设色里挑一个,回车确认
Down  Sleep 500ms
Down  Sleep 500ms
Right Sleep 500ms
Enter
Sleep 1200ms
Screenshot "/tmp/verify-theme-3.png"
Sleep 500ms
