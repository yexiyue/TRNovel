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
| `tts-play.tape` | tts-play.gif | 下载 Kokoro 模型 + 播放按句高亮 | **真机**(需音频) |
| `login-fanqie.tape` | login-fanqie.gif | 番茄:浏览器登录全流程 | **真机**(需 Chrome) |
| `browser-challenge.tape` | browser-challenge.gif | bilixs/CF 浏览器辅助过挑战 | **真机**(需 Chrome) |

**真机项**:沙箱缺音频设备 / 无法启动 headful 浏览器 / 部分书源搜索接口受限,这 4 个需在本机录制。
tape 头部注释了各自前置条件与可调整处(书源名次序、本地路径等)。
