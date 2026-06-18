# 文档演示录制(VHS tapes)

本目录下的 `.tape` 文件用 [charmbracelet/vhs](https://github.com/charmbracelet/vhs) 把 TRNovel 的
终端界面录成 GIF,作为文档资源(输出到 `../src/assets/guides/`)。所有 GIF 均由这些 tape 生成,
**改了界面只需重跑对应 tape 即可刷新文档配图**。

## 依赖

```bash
brew install vhs ttyd ffmpeg
```

## 录制

```bash
cd docs/tapes
vhs home.tape            # 单个
for t in *.tape; do vhs "$t"; done   # 全部(真机项见下)
```

所有 tape 统一规格:`1280×800 · FontSize 20 · Catppuccin Mocha`。

## 演示环境(隔离,不污染真实 ~/.novel)

沙箱内录制用 `Env HOME "/tmp/trn-demo-home"` 隔离,避免动到真实阅读历史/书源。准备:

```bash
mkdir -p /tmp/trn-demo-home/books/仙侠
# 放入测试小说(原创):星河彼岸.txt(6 章)/ 山中旧事.txt(3 章)/ 仙侠/剑来纪.txt(3 章)
# 预热历史:用 read.tape 等打开几本并 q 退出,即生成 ~/.novel/history.json
# 网络源:把 fanqie-web.v2.json / test-novels/bilixs.v2.json 拷进去,用 -n 模式 s 导入
```

二进制:tape 内用 `trn` 命令,需先把它装到 PATH(`cargo install --path .`,或从 release 安装)。

## tape 清单

| tape | 资源 | 内容 | 录制环境 |
|------|------|------|----------|
| `home.tape` | home.gif | 主页四入口 + Logo | 沙箱 |
| `local-select.tape` | local-select.gif | 本地选书:文件树、展开子目录、打开 | 沙箱 |
| `read.tape` | read.gif | 阅读、方向键翻页、Tab 跳章 | 沙箱 |
| `shortcuts.tape` | shortcuts.gif | i 唤出快捷键浮层 | 沙箱 |
| `theme.tape` | theme.gif | 主题色设置 + 取色弹窗 | 沙箱 |
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
