# Requires prepared Qwen models, cached device calibration and a fresh isolated HOME.
Output "/tmp/trn-follow-demo/follow-small.gif"
Set Width 1000
Set Height 420
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Framerate 8
Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
Env HOME "/tmp/trn-follow-demo/home"
Type "trn --tts-program novel-tts -l /tmp/trn-follow-demo/home/books"
Enter
Sleep 8s
Down
Sleep 500ms
Enter
Sleep 3s
Tab
Sleep 3s
Screenshot "/tmp/trn-follow-demo/initial.png"
Sleep 1s
Type "p"
Sleep 20s
PageDown
Sleep 2s
Screenshot "/tmp/trn-follow-demo/free.png"
Sleep 1s
Type "f"
Sleep 1s
Screenshot "/tmp/trn-follow-demo/recover.png"
Sleep 1s
Type "s"
Type "树影"
Enter
Sleep 1s
Screenshot "/tmp/trn-follow-demo/search.png"
Sleep 1s
Escape
Type "f"
Sleep 7s
Screenshot "/tmp/trn-follow-demo/cross-screen.png"
Sleep 1s
Type "p"
Sleep 500ms
PageDown
Sleep 500ms
Type "f"
Sleep 1s
Screenshot "/tmp/trn-follow-demo/paused-small.png"
Sleep 2s
Type "p"
Sleep 30s
Screenshot "/tmp/trn-follow-demo/next-small.png"
Sleep 1s
Type "q"
