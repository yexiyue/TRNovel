# TRNovel 文档演示 · 网络小说在线浏览与阅读
# 录制(从本目录):  vhs network-read.tape
# 需要书源缓存里已有可用书源(此处用「番茄小说(网页)」,reqwest 直读)。

Output "../src/assets/guides/network-read.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env HOME "/tmp/trn-demo-home"

Type "trn -n"  Enter
Sleep 1800ms

# 选「番茄小说(网页)」进入,加载书库入口书籍
Enter
Sleep 5000ms
Screenshot "/tmp/verify-nr-1books.png"

# Tab 打开分类弹窗,选一个分类
Tab
Sleep 1300ms
Screenshot "/tmp/verify-nr-2cats.png"
Down  Sleep 600ms
Down  Sleep 600ms
Enter
Sleep 4500ms
Screenshot "/tmp/verify-nr-3list.png"

# 选一本书进入详情
Down  Sleep 500ms
Enter
Sleep 4500ms
Screenshot "/tmp/verify-nr-4detail.png"

# 进入阅读
Enter
Sleep 5000ms
Screenshot "/tmp/verify-nr-5read.png"

# 翻页
Down  Sleep 1000ms
Down  Sleep 1000ms
Screenshot "/tmp/verify-nr-6read2.png"
Sleep 500ms
