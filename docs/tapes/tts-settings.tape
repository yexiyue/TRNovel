# TRNovel 文档演示 · 听书设置页
# 录制(从本目录):  vhs tts-settings.tape
# 阅读页按 T 打开听书面板，浏览模型目录不会下载或加载模型。
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
Env NO_COLOR ""
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

# 进入声音设置，再打开可搜索模型列表
Down  Sleep 500ms
Down  Sleep 500ms
Down  Sleep 500ms
Down  Sleep 500ms
Down  Sleep 500ms
Enter
Sleep 800ms
Enter
Sleep 800ms
Type "qwen"
Sleep 800ms
Screenshot "/tmp/verify-tts-2.png"
Escape
Sleep 500ms
Down
Enter
Sleep 800ms
Escape
Sleep 500ms
