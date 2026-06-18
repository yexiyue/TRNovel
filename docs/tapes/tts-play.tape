# TRNovel 文档演示 · 听书:下载模型 + 播放高亮(真机录制)
# 录制(从本目录):  vhs tts-play.tape
#
# 前置:此演示会下载 Kokoro 模型(检查点 + 语音,数百 MB),并需要可用音频设备。
#       沙箱无音频设备、模型体积大,故留给真机。模型缓存于 ~/.novel-tts/kokoro。
#
# 流程:打开小说 → Tab 进阅读模式 → t 打开听书设置 → 下载检查点模型 + 语音数据
#       → Tab 回阅读 → p 播放,正文按句高亮跟随朗读。

Output "../src/assets/guides/tts-play.gif"

Set Width 1280
Set Height 800
Set FontSize 20
Set Theme "Catppuccin Mocha"
Set Padding 24
Set TypingSpeed 60ms

Env TERM "xterm-256color"
Env COLORTERM "truecolor"

# 用一本本地小说(换成你自己的路径),或先 trn -q 续读
Type "trn -l ~/books"  Enter
Sleep 4s
Down  Sleep 400ms  Enter        # 打开第一本
Sleep 2500ms
Tab                              # 进阅读模式
Sleep 1000ms

# t 打开听书设置
Type "t"
Sleep 1500ms

# 选「检查点模型」回车下载(数百 MB,耐心等待;断点续传)
Enter
Sleep 60s

# 下移到「语音数据」回车下载
Down  Sleep 600ms
Enter
Sleep 30s

# Tab 回到阅读模式,p 开始播放,观察按句高亮
Tab
Sleep 1500ms
Type "p"
Sleep 12s
