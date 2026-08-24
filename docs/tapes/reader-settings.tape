# TRNovel 文档演示 · 阅读设置面板(v0.16.0+ 翻页重叠/显示标题,v0.17.0+ 段落间距)
# 录制(从本目录):  vhs reader-settings.tape
#
# 前置:/tmp/trn-demo-home/books/星河彼岸.txt(多段落正文,段落间距的效果才看得出来)
# 演示要点:o 开面板 → ↑/↓ 选条目、←/→ 调整 → 改动即时生效并自动落盘。
# 注意:断言「操作前」状态的 Screenshot 前后都要留 ≥1s —— 紧跟按键会截到后一帧。

Output "../src/assets/guides/reader-settings.gif"

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

# 打开一本多段落的书(树里第一个 .txt 文件的位置见 local-select.tape 的注释)
Type "trn -l /tmp/trn-demo-home/books"  Enter
Sleep 2500ms
Down  Sleep 400ms
Up    Sleep 400ms
Down  Sleep 300ms
Down  Sleep 300ms
Down  Sleep 500ms
Enter
Sleep 2200ms
Screenshot "/tmp/verify-settings-1read.png"

# o 打开阅读设置:三个条目
Type "o"
Sleep 1400ms
Screenshot "/tmp/verify-settings-2panel.png"

# 翻页重叠 2 → 4,同屏的「每页滚动 N 行」跟着变
Right  Sleep 700ms
Right  Sleep 1500ms
Screenshot "/tmp/verify-settings-3overlap.png"
Sleep 1200ms

# ↓↓ 移到「段落间距」,→ 开启
Down  Sleep 600ms
Down  Sleep 1500ms
Right
Sleep 1600ms
Screenshot "/tmp/verify-settings-4spacing.png"
Sleep 1200ms

# 关掉面板看正文:段落之间多了空行,底部总行数随之变化
Type "o"
Sleep 1800ms
Screenshot "/tmp/verify-settings-5content.png"
Sleep 800ms
