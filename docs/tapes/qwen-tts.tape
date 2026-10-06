# Real Qwen models and audio device required; prepare auto calibration before recording.
Output "/tmp/trn-qwen-demo/qwen.gif"
Set Width 1440
Set Height 1000
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Framerate 10
Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
Env HOME "/tmp/trn-qwen-demo/home"
Type "trn --tts-program novel-tts -l /tmp/trn-qwen-demo/home/books"
Enter
Sleep 4s
Down
Enter
Sleep 2s
Enter
Sleep 2s
Type "T"
Sleep 3s
Down
Right
Sleep 2s
Screenshot "/tmp/trn-qwen-demo/backend.png"
Sleep 1s
Down
Right
Sleep 1s
Screenshot "/tmp/trn-qwen-demo/voice.png"
Sleep 1s
Left
Up 2
Enter
Sleep 12s
Screenshot "/tmp/trn-qwen-demo/ready.png"
Sleep 1s
Type "T"
Type "p"
Sleep 3s
Screenshot "/tmp/trn-qwen-demo/buffering.png"
Sleep 4s
Screenshot "/tmp/trn-qwen-demo/playing.png"
Sleep 1s
Type "p"
Sleep 1s
Type "q"
