# TRNovel 文档演示 · 本地选书(文件树 + 章节目录)
# 录制(从本目录):  vhs local-select.tape

Output "../src/assets/guides/local-select.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env HOME "/tmp/trn-demo-home"

# -l 直接进本地选书页,扫描指定目录下的 .txt
Type "trn -l /tmp/trn-demo-home/books"  Enter
Sleep 1800ms
Screenshot "/tmp/verify-local-1tree.png"

# 预热输入(首帧按键易被吞),再回车展开「仙侠」子目录
Down  Sleep 500ms
Up    Sleep 500ms
Enter  Sleep 800ms
Screenshot "/tmp/verify-local-2expand.png"

# 移到子目录里的「剑来纪.txt」,回车进入阅读 + 章节目录
Down  Sleep 500ms
Enter
Sleep 1300ms
Screenshot "/tmp/verify-local-3read.png"
Sleep 500ms
