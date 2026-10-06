Output "/tmp/trn-continuous-demo/optional-alignment.gif"
Set Width 1440
Set Height 1000
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Framerate 10
Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
Env HOME "/tmp/trn-continuous-demo/home"
Type "trn -l /tmp/trn-continuous-demo/home/books"
Enter
Sleep 5s
Down
Enter
Sleep 2s
Type "T"
Sleep 2s
Screenshot "/tmp/trn-continuous-demo/default-off.png"
Enter
Sleep 12s
Type "T"
Type "p"
Sleep 4s
Screenshot "/tmp/trn-continuous-demo/fragment.png"
Type "p"
Sleep 1s
Type "T"
Down 7
Right
Sleep 2s
Screenshot "/tmp/trn-continuous-demo/enabled.png"
Down
Sleep 1s
Screenshot "/tmp/trn-continuous-demo/alignment-device.png"
Up
Left
Sleep 2s
Screenshot "/tmp/trn-continuous-demo/disabled-again.png"
Type "T"
Type "q"
Sleep 1s
