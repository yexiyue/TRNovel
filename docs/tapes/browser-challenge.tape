# TRNovel 文档演示 · 浏览器辅助过反爬挑战(bilixs / Cloudflare,真机录制)
# 录制(从本目录):  vhs browser-challenge.tape
#
# 前置:~/.novel 已导入会触发 Cloudflare 挑战的书源(如 bilixs/书海,http.fetcher=auto);
#       需系统已装 Chrome/Edge。沙箱无法启动 headful 浏览器,故留给真机。
#
# 流程:网络模式进入该书源做一次会撞挑战的操作(搜索/浏览)→ 弹「需要浏览器验证」
#       (Y 本次 / A 总是 / N 拒绝)→ 按 Y → 系统浏览器打开解挑战 → 若出现「确认您是
#       真人」勾选框,在浏览器里点一下 → 自动拿到 cf_clearance 后继续快速抓取。
#
# 提示:也可在书源管理页按 B 预先开启「总是允许浏览器辅助验证」,跳过每次弹窗。

Output "../src/assets/guides/browser-challenge.gif"

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

# 选中会撞挑战的书源进入(按你的列表顺序调整)
Enter
Sleep 5s

# 触发一次会撞 CF 的操作:搜索
Type "s"
Sleep 800ms
Type "都市"
Sleep 600ms
Enter
Sleep 3s

# —— 此时弹「需要浏览器验证」,按 Y 本次允许 ——
Type "Y"
Sleep 6s

# —— 若浏览器出现 Turnstile 勾选框,请在浏览器里点「确认您是真人」 ——
# 解挑战完成后自动继续,搜索结果返回
Sleep 8s
