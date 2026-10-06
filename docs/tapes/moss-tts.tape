# MOSS/Kokoro 双后端真实 UI 验收，准备方式见 README.md。
Output "/tmp/trn-moss-demo/moss.gif"
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
Env HOME "/tmp/trn-moss-demo/home"
Type "trn -l /tmp/trn-moss-demo/home/books"
Enter
Sleep 8s
Down
Enter
Sleep 5s
Type "T"
Sleep 2s
Screenshot "/tmp/trn-moss-demo/moss-settings.png"
Sleep 1s
Down
Right
Sleep 2s
Screenshot "/tmp/trn-moss-demo/kokoro-selected.png"
Sleep 1s
Left
Sleep 1s
Down
Right
Sleep 2s
Screenshot "/tmp/trn-moss-demo/voice-selected.png"
Sleep 1s
Left
Sleep 1s
Up
Up
Enter
Sleep 45s
Screenshot "/tmp/trn-moss-demo/model-ready.png"
Sleep 1s
Type "T"
Type "p"
Sleep 3s
Screenshot "/tmp/trn-moss-demo/playing.png"
Sleep 1s
Type "p"
Sleep 1s
Screenshot "/tmp/trn-moss-demo/paused.png"
Sleep 1s
Type "q"
Sleep 2s
