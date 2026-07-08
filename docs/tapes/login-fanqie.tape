# TRNovel 文档演示 · 书源登录:番茄(浏览器登录,真机录制)
# 录制(从本目录):  vhs login-fanqie.tape
#
# 前置:~/.novel 已导入「番茄小说(网页)」书源(loginUrl 为普通 URL → 浏览器登录);
#       需系统已装 Chrome/Edge。沙箱无法启动 headful 浏览器,故留给真机。
#
# 流程:网络模式选中番茄书源 → 按 L 进登录页 → 提示「浏览器登录」→ Enter 打开系统
#       浏览器,在真实页面完成登录 → 回到终端 Enter 完成,登录态自动保存复用。

Output "../src/assets/guides/login-fanqie.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""

Type "trn -n"  Enter
Sleep 4s

# 在仅选择模式上下移动到「番茄小说(网页)」(按你的列表顺序调整)
# 这里假设它在列表首位,按 L 进入登录页
Type "L"
Sleep 2500ms

# 登录页提示浏览器登录;Enter 打开系统浏览器
Enter
Sleep 8s

# —— 在弹出的浏览器里手动完成番茄账号登录 ——
# 录制时请在此期间于浏览器内登录,然后回到终端

# 回到终端按 Enter 完成登录(提取 cookie / token 并保存)
Enter
Sleep 4s
