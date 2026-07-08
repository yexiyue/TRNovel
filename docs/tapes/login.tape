# TRNovel 文档演示 · 书源登录(表单登录)
# 录制(从本目录):  vhs login.tape
# 需要书源缓存里有一个配置了 loginUi 的书源(示例登录书源);
# 番茄等「浏览器登录」书源用法见 login-fanqie.tape。

Output "../src/assets/guides/login.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 70ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
Env HOME "/tmp/trn-demo-home"

Type "trn -n"  Enter
Sleep 1800ms
Screenshot "/tmp/verify-login-1.png"

# 移到「示例登录书源」(列表末项,多按 Down 末尾 clamp 稳定命中),按 L 进入登录页
Down  Sleep 400ms
Down  Sleep 400ms
Down  Sleep 500ms
Type "L"
Sleep 1200ms
Screenshot "/tmp/verify-login-2.png"

# 填写登录表单:账号 / 密码 / 切换字段
Type "reader@demo.com"
Sleep 500ms
Tab
Sleep 500ms
Type "my-secret-pass"
Sleep 500ms
Tab
Sleep 600ms
Screenshot "/tmp/verify-login-3.png"
Sleep 500ms
