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
Env NO_COLOR ""
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

# 浏览命名主题并应用一个新主题
Down  Sleep 500ms
Down  Sleep 500ms
Enter
Sleep 1200ms
Screenshot "/tmp/verify-theme-2.png"

# 切换背景模式,展示终端背景 / 主题背景差异
Type "T"
Sleep 1000ms
Screenshot "/tmp/verify-theme-3.png"

# 打开重置确认弹窗,展示新主题系统的恢复默认入口
Type "D"
Sleep 1000ms
Screenshot "/tmp/verify-theme-4.png"
Sleep 500ms
