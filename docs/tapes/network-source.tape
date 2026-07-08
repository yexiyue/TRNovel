# TRNovel 文档演示 · 书源管理(导入 / 选择)
# 录制(从本目录):  vhs network-source.tape
# 本演示离线可录:从本地 file 路径导入书源、浏览源列表;
# 真机上把路径换成书源 URL(https://...)即可演示在线导入。

Output "../src/assets/guides/network-source.gif"

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

# 进入网络小说;书源为空时直接是「导入模式」
Type "trn -n"  Enter
Sleep 1800ms
Screenshot "/tmp/verify-net-1.png"

# s 输入书源地址(支持 http/https/file),这里用本地文件路径离线演示
Type "s"
Sleep 500ms
Type "/tmp/trn-demo-home/bilixs.v2.json"
Sleep 600ms
Enter
Sleep 1300ms
Screenshot "/tmp/verify-net-2.png"

# 方向键移入解析结果列表并高亮,空格勾选,回车导入
Down
Sleep 500ms
Space
Sleep 600ms
Enter
Sleep 800ms
Screenshot "/tmp/verify-net-3.png"

# Tab 切到「仅选择模式」,浏览已导入的书源
Tab
Sleep 1000ms
Screenshot "/tmp/verify-net-4.png"

# q 干净退出(落盘书源缓存)
Sleep 500ms
Type "q"
Sleep 800ms
