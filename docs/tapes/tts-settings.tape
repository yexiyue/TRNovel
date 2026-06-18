# TRNovel 文档演示 · 听书设置页
# 录制(从本目录):  vhs tts-settings.tape
# 阅读页按 T 打开听书设置;模型未下载时显示下载入口与各项参数。
# 真机上模型下载 + 播放高亮见 tts-play.tape。

Output "../src/assets/guides/tts-settings.gif"

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

# 打开星河彼岸,Tab 切到阅读模式(TTS 设置仅在阅读模式可开)
Down  Sleep 400ms
Down  Sleep 400ms
Down  Sleep 500ms
Enter
Sleep 1300ms
Tab
Sleep 800ms

# t 打开听书设置
Type "t"
Sleep 1200ms
Screenshot "/tmp/verify-tts-1.png"

# 上下浏览各项参数
Down  Sleep 500ms
Down  Sleep 500ms
Down  Sleep 500ms
Screenshot "/tmp/verify-tts-2.png"
Sleep 500ms
