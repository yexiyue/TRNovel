# TRNovel 文档演示 · 阅读与翻页
# 录制(从本目录):  vhs read.tape

Output "../src/assets/guides/read.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env HOME "/tmp/trn-demo-home"

Type "trn -l /tmp/trn-demo-home/books"  Enter
Sleep 1800ms

# 移到列表末尾的「星河彼岸.txt」(三次 Down 末尾 clamp,稳定命中),回车阅读
Down  Sleep 500ms
Down  Sleep 500ms
Down  Sleep 500ms
Enter
Sleep 1300ms
Screenshot "/tmp/verify-read-1.png"

# 方向键翻页
Down  Sleep 800ms
Down  Sleep 800ms
Screenshot "/tmp/verify-read-2.png"

# Tab 切到左侧目录,跳到第三章
Tab   Sleep 500ms
Down  Sleep 500ms
Down  Sleep 600ms
Enter Sleep 1200ms
Screenshot "/tmp/verify-read-3.png"

# q 退出(干净收尾,落历史记录)
Sleep 600ms
Type "q"
Sleep 1000ms
