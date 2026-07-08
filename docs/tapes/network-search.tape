# TRNovel 文档演示 · 网络小说搜索(真机录制)
# 录制(从本目录):  vhs network-search.tape
#
# 前置:~/.novel 里已导入一个「搜索可用」的书源(本仓库 fanqie-web 搜索接口已失效,
#       书海(bilixs)搜索需浏览器解 CF)。请换成你自己搜索可用的源,或先用 trn import 导入。
# 沙箱环境因书源搜索接口/反爬限制无法稳定录制,故留给真机。
#
# 流程:进入网络模式 → 选书源 → s 输入关键字 → 回车搜索 → 浏览结果 → 进入详情/阅读。

Output "../src/assets/guides/network-search.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""

# 用你真实的 ~/.novel 书源;如需隔离可解除下一行注释指向演示 HOME
# Env HOME "/tmp/trn-demo-home"

Type "trn -n"  Enter
Sleep 4s

# 选中书源进入(若在仅选择模式,直接回车选第一个源)
Enter
Sleep 5s

# s 进入搜索,输入关键字回车
Type "s"
Sleep 800ms
Type "剑来"
Sleep 600ms
Enter
Sleep 6s

# 浏览搜索结果,选一本进入详情,再回车阅读
Down  Sleep 1000ms
Enter
Sleep 5s
Enter
Sleep 6s
Sleep 1s
