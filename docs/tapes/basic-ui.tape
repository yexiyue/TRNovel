# 基础阅读版 UI 回归：必须先构建 --no-default-features。
# 演示书库准备与录制命令见 README.md。
Output "../../target/basic-ui.gif"
Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 1ms
Set Framerate 10
Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
Env HOME "/tmp/trn-demo-home"
Type "trn -l /tmp/trn-demo-home/books/星河彼岸.txt"
Enter
Sleep 8s
Down
Sleep 1s
Enter
Sleep 3s
Screenshot "../../target/verify-basic-chapters.png"
Sleep 1s
Tab
Sleep 1s
Screenshot "../../target/verify-basic-read.png"
Sleep 1s
Type "o"
Sleep 1s
Screenshot "../../target/verify-basic-settings.png"
Sleep 1s
Right
Sleep 1s
Screenshot "../../target/verify-basic-setting-adjusted.png"
Sleep 1s
Type "o"
Sleep 1s
Type "i"
Sleep 1s
Screenshot "../../target/verify-basic-help.png"
Sleep 1s
Type "i"
Sleep 1s
Down
Sleep 1s
Screenshot "../../target/verify-basic-scroll.png"
Sleep 1s
Type "s"
Sleep 1s
Type "林舟"
Enter
Sleep 1s
Screenshot "../../target/verify-basic-search.png"
Sleep 1s
Escape
Sleep 1s
Type "tp+-"
Sleep 1s
Screenshot "../../target/verify-basic-no-tts.png"
Sleep 1s
Tab
Sleep 1s
Down
Sleep 1s
Enter
Sleep 2s
Screenshot "../../target/verify-basic-next-chapter.png"
Sleep 1s
Type "q"
Sleep 3s
