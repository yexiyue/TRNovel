# 文档演示录制(VHS tapes)

本目录下的 `.tape` 文件用 [charmbracelet/vhs](https://github.com/charmbracelet/vhs) 把 TRNovel 的
终端界面录成 GIF,作为文档资源(输出到 `../src/assets/guides/`)。关键帧截图也在 tape 里用
VHS 内置 `Screenshot` 命令生成,不用再从 GIF/视频里二次抽帧。所有演示素材均由这些 tape 生成,
**改了界面只需重跑对应 tape 即可刷新文档配图与验收截图**。

## 依赖

```bash
brew install vhs
```

Homebrew 会带上 VHS 录制所需的 `ttyd` / `ffmpeg` 运行时依赖;这里不再单独使用
`ffmpeg` 做截图抽帧。

## 录制

```bash
cd docs/tapes
vhs home.tape            # 单个
for t in *.tape; do vhs "$t"; done   # 全部(真机项见下)
```

所有 tape 统一规格:`1280×800 · FontSize 20 · Catppuccin Mocha`。

所有 tape 都显式设置:

```text
Env TERM "xterm-256color"
Env COLORTERM "truecolor"
Env NO_COLOR ""
```

Codex / CI shell 里可能带 `TERM=dumb` 或 `NO_COLOR=1`,会让 ratatui/crossterm 抑制样式码,
导致录出来只有近似黑白的终端画面。重录素材前先用 VHS `Screenshot` 检查关键帧是否有高亮色、
边框色与主题背景。

每个稳定沙箱 tape 会在关键节点执行 `Screenshot "/tmp/verify-*.png"`。这些截图用于录制后快速验收
画面是否进入预期状态。**`Screenshot` 后若紧跟下一个按键而不留 `Sleep`,截到的可能是按键之后的帧**
—— 曾据此误判「值在按右键之前就变了」。断言「操作前」状态的截图,前后都要留 ≥1s。

截图需要保留到仓库或文章素材时,把路径改成 `../src/assets/guides/<name>.png` 即可,
不要再用 `ffmpeg` 从 GIF 抽帧。

## 演示环境(隔离,不污染真实 ~/.novel)

沙箱内录制用 `Env HOME "/tmp/trn-demo-home"` 隔离,避免动到真实阅读历史/书源。准备:

```bash
mkdir -p /tmp/trn-demo-home/books/{仙侠,科幻/深空来信}
# 放入测试小说(原创),当前 tape 依赖这套结构与顺序:
#   星河彼岸.txt(6 章,每章多段落 —— 段落间距的效果全靠它)
#   山中旧事.txt(3 章)
#   仙侠/剑来纪.txt(3 章)、仙侠/沧海问剑.txt(2 章)   —— 搜「剑」命中两本
#   科幻/深空来信/信使.txt(2 章)                      —— 搜「信使」命中二级嵌套目录
# 树的顺序(目录在前、同级按名):仙侠 / 科幻 / 山中旧事.txt / 星河彼岸.txt
#   —— tape 里的 Down 次数按这个顺序数,增删演示书要同步改按键序列。
# 预热历史:用 read.tape 等打开几本并 q 退出,即生成 ~/.novel/history.json
# 网络源:把 fanqie-web.v2.json / test-novels/bilixs.v2.json 拷进去,用 -n 模式 s 导入
```

录制前**先清掉沙箱配置**,否则上一次录制留下的阅读偏好(如已开启的段落间距)会带进新素材:

```bash
rm -rf /tmp/trn-demo-home/.novel
```

二进制:tape 内用 `trn` 命令,需先把它装到 PATH(`cargo install --path .`,或从 release 安装)。
**不想覆盖本机已装的版本**,可以装到隔离目录再临时挂到 PATH 上录制:

```bash
cargo install --path . --root /tmp/trn-demo-bin --locked
PATH="/tmp/trn-demo-bin/bin:$PATH" vhs local-search.tape
```

VHS 会继承当前 shell 的环境变量,所以这样挂 PATH 对 tape 文件零侵入(tape 里仍写 `trn`)。
录之前记得核对 `trn --version` 是不是待发布的那版 —— 用旧版录出来的是旧界面。

## tape 清单

| tape | 资源 | 内容 | 录制环境 |
|------|------|------|----------|
| `home.tape` | home.gif | 主页四入口 + Logo | 沙箱 |
| `local-select.tape` | local-select.gif | 本地选书:文件树、展开子目录、打开 | 沙箱 |
| `local-search.tape` | local-search.gif | 本地选书:`/` 按名称搜索、命中目录自动展开 | 沙箱 |
| `read.tape` | read.gif | 阅读、方向键翻页、Tab 跳章 | 沙箱 |
| `reader-settings.tape` | reader-settings.gif | 阅读设置浮层:翻页重叠 / 显示标题 / 段落间距 | 沙箱 |
| `shortcuts.tape` | shortcuts.gif | i 唤出快捷键浮层 | 沙箱 |
| `theme.tape` | theme.gif | 命名主题切换 + 背景模式 | 沙箱 |
| `history.tape` | history.gif | 阅读历史列表 | 沙箱 |
| `network-source.tape` | network-source.gif | 导入书源(file 路径)+ 浏览源列表 | 沙箱(离线) |
| `network-read.tape` | network-read.gif | 在线书库 → 分类 → 详情 → 阅读 | 沙箱(联网) |
| `login.tape` | login.gif | 表单登录(loginUi)填写 | 沙箱(离线) |
| `tts-settings.tape` | tts-settings.gif | 听书设置页(未下载状态 + 各参数) | 沙箱(离线) |
| `network-search.tape` | network-search.gif | 在线搜索 → 结果 → 阅读 | **真机** |
| `tts-play.tape` | tts-play.gif | 下载 MOSS Nano 模型 + 播放按句高亮 | **真机**(需音频) |
| `login-fanqie.tape` | login-fanqie.gif | 番茄:浏览器登录全流程 | **真机**(需 Chrome) |
| `browser-challenge.tape` | browser-challenge.gif | bilixs/CF 浏览器辅助过挑战 | **真机**(需 Chrome) |

**真机项**:沙箱缺音频设备 / 无法启动 headful 浏览器 / 部分书源搜索接口受限,这 4 个需在本机录制。
tape 头部注释了各自前置条件与可调整处(书源名次序、本地路径等)。

## 基础阅读版 UI 回归

`basic-ui.tape` 检查无 TTS 构建的目录/正文切换、阅读设置、帮助、滚动、正文搜索和翻章。沿用上面的隔离演示书库，直接选 `星河彼岸.txt`，不依赖文件树的排序。输出与截图保存在仓库 `target/`，不覆盖文档演示素材。

```bash
# 从仓库根目录构建；必须使用 package-specific 的无默认 feature 命令。
cargo build --locked -p trnovel --no-default-features --bins
cd docs/tapes
PATH="$(pwd)/../../target/debug:/opt/homebrew/bin:$PATH" vhs basic-ui.tape
```

检查 `verify-basic-read.png`、`verify-basic-settings.png`、`verify-basic-help.png`：正文边框都应保持整个终端的高度，打开浮层只遮罩正文，不能挤出下半屏空白。帮助中不应出现听书操作；`t/p/+/-` 不应打开听书面板。录制给首次启动留 8 秒，目录进入阅读页后先切到正文再截图。

## MOSS 流式听书

`moss-tts.tape` 验证后端切换、预置音色、模型准备、实际播放高亮与暂停。先构建双后端并准备隔离环境：

```sh
cargo build -p trnovel
cargo build --manifest-path vendor/talechime/Cargo.toml -p talechime --features qwen --target-dir target
mkdir -p /tmp/trn-moss-demo/home
cp -R /tmp/trn-demo-home/books /tmp/trn-moss-demo/home/
# ~/.novel-tts 内需已经有完整 moss/tts 和 moss/codec；复用模型，不复制大权重。
ln -s "$HOME/.novel-tts" /tmp/trn-moss-demo/home/.novel-tts
PATH="$PWD/target/debug:$PATH" vhs docs/tapes/moss-tts.tape
```

此录制仅编入 Qwen 的通用 adapter，不启用 Metal；新沙箱默认 MOSS/Weiguo。已有沙箱会保留后端设置，重录前先删除沙箱内 tts_config.json。所有输出写入 /tmp/trn-moss-demo，不覆盖正式指南素材。模型校验和加载需要时间；按机器性能调整准备后的 Sleep。基础版验收请固定独立二进制路径，避免后续 all-features 构建覆盖同一 target/debug/trn。

## 连续朗读与逐句对齐

`continuous-tts.tape` 使用真实 MOSS/Qwen，不使用伪造时间戳。构建 `trn` 和 `novel-tts --features coreml`，把它们放在 PATH 同目录。创建 `/tmp/trn-continuous-demo/home/books/连续朗读.txt`，使用自编的两个章节和至少 12 段三句正文，例如“你好，欢迎使用听书功能。今天我们一起阅读一个故事。夜色落在山间，星河缓缓流动。”。在 home/.novel/tts_config.json 设置 moss/Weiguo、tts_device=cpu、alignment_device=cpu、alignment_enabled=true，并把 home/.novel-tts 链接到已校验模型根目录。运行 vhs；首次完整下载不能按录制中的短等待验收。

录制覆盖轻量连接、准备状态、连续播放、片段转逐句高亮、暂停和执行设备。NO_COLOR 必须清空，否则继承 shell 的 NO_COLOR=1 会使高亮颜色不可见。保留新 HOME，避免旧续读检查点把待高亮正文移出视口。

### 默认片段高亮与可选对齐

`optional-alignment.tape` 使用与 continuous-tts 相同的目录和真实 MOSS 资源，配置 `alignment_enabled=false`。录制默认关闭、播放片段高亮、开启后出现对齐设备、关闭后隐藏设备；切换不自动准备 Qwen。

## Qwen3-TTS Candle

`qwen-tts.tape` 检查后端与音色切换、实际设备、模型就绪、播放与暂停。使用自编书和隔离 HOME：

```sh
cargo build --release --manifest-path vendor/talechime/Cargo.toml -p talechime --target-dir target --features qwen,metal
cargo build -p trnovel --bin trn
mkdir -p /tmp/trn-qwen-demo/home/books
# 放入“试听.txt”，包含两个章节、至少二十段短正文。
# 例如：第一章 山间 / 你好，欢迎收听。山风轻拂，夜色安静。 / 第二章 夜色
ln -s "$HOME/.novel-tts" /tmp/trn-qwen-demo/home/.novel-tts
PATH="$PWD/target/release:$PWD/target/debug:$PATH" vhs docs/tapes/qwen-tts.tape
```

worker 需编译 MOSS + Qwen（录制按后端目录顺序切到 Qwen）。真实 qwen/ 资源须预先校验，auto 应已校准并命中缓存；首次校准约数分钟，不适合用录制内短 Sleep 验收。非 macOS 构建去掉 metal。输出与截图位于 /tmp/trn-qwen-demo；重录使用新的隔离 HOME，避免续读位置影响画面。

Qwen tape 也检查启动“缓冲中”及余量、随后实际播放与片段高亮；截图 `buffering.png` 中不应提前高亮尚未播放的文本。自动缓冲恢复仍服从用户暂停。

## 听书跟随阅读窗口

`follow-tts.tape` 使用真实 Qwen 播放和小窗口，检查跨屏、手动翻页、搜索、`f` 恢复、暂停定位和自动续章。先按上节构建并校验模型、完成 auto 设备校准；使用新的 `/tmp/trn-follow-demo/home`，把 `.novel-tts` 链接到已准备的模型目录。

创建 `home/books/跟随试听.txt`：第一章“山间”，正文每段一行，段间空行，依次为“山风轻拂。月光皎洁。星河流动。夜色安静。晨光初现。鸟鸣清脆。溪水潺潺。树影摇曳。”；第二章“清晨”，正文为“清晨到了。”和“你好，欢迎收听。”。章节标题使用“第一章 山间”和“第二章 清晨”，各自单独一行。

在 `home/.novel/tts_config.json` 写入：

```json
{"backend":"qwen","voice":"uncle_fu","speed":1.0,"volume":0.2,"auto_play":true,"tts_device":"auto","alignment_enabled":false}
```

运行 `PATH="$PWD/target/release:$PWD/target/debug:$PATH" vhs docs/tapes/follow-tts.tape`。录制等待按已校准 M4 Pro 设置；其他机器需调整 Sleep，截图时检查实际 Playing/Paused，不能只凭经过时间判定。首次校准或下载须在录制前完成。检查 `free.png`、`recover.png`、`search.png`、`cross-screen.png`、`paused-small.png`、`next-small.png`，确认高亮锚点可见、自由浏览不被拉回、暂停恢复定位不恢复播放、续章保留状态。无 TTS 设置与帮助另用 `basic-ui.tape` 检查。
