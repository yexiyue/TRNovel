# TRNovel 文档演示 · 本地小说按名称搜索(v0.17.0+)
# 录制(从本目录):  vhs local-search.tape
#
# 前置:/tmp/trn-demo-home/books 下需有嵌套书库,其中
#   仙侠/剑来纪.txt、仙侠/沧海问剑.txt   —— 搜「剑」命中两本(同一子目录)
#   科幻/深空来信/信使.txt                —— 搜「信使」命中二级嵌套目录
# 演示要点:搜索词只过滤内存索引(不重新扫盘),命中所在目录会自动展开。

Output "../src/assets/guides/local-search.gif"

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

# 进入本地选书页:目录默认折叠
Type "trn -l /tmp/trn-demo-home/books"  Enter
Sleep 2500ms
Screenshot "/tmp/verify-search-1tree.png"

# 预热输入(首帧按键易被吞)
Down  Sleep 400ms
Up    Sleep 600ms

# / 聚焦搜索框,按书名筛选
Type "/"
Sleep 900ms
Type "剑"
Sleep 900ms
Enter
Sleep 1600ms
Screenshot "/tmp/verify-search-2jian.png"
Sleep 1200ms

# 换一个词:命中埋在二级嵌套目录里,同样自动展开
Type "/"
Sleep 700ms
Backspace 3
Sleep 400ms
Type "信使"
Sleep 900ms
Enter
Sleep 1600ms
Screenshot "/tmp/verify-search-3nested.png"
Sleep 1200ms

# 直接选中命中的书打开
Down  Sleep 400ms
Down  Sleep 400ms
Down  Sleep 600ms
Enter
Sleep 2000ms
Screenshot "/tmp/verify-search-4read.png"
Sleep 600ms
