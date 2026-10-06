# 听书进程解耦验收记录

日期：2026-10-06。变更：`decouple-tts-process`。本机 macOS / Apple M4 Pro ARM64，Rust 1.98.1，cargo-dist 固定 0.32.0。本记录描述本次本地工作区，尚未发布。

命名迁移：历史播放、PTY 与压缩包验收是在重命名前执行，本文示例按当前名称更新。当前核心为 novel-tts-core、独立程序为 novel-tts、阅读器模块为 src/tts.rs；本轮重新执行构建检查与发行工作流生成，不把此前制品验收视为新名称制品已重新验收。

## 已执行检查

| 范围 | 真实结果 |
| --- | --- |
| workspace all-features lib/tests/examples | 核心 19、协议 8、程序协议集成 3、解析库 146、阅读器 55、TOC 集成 2 个测试通过；配置并发测试包含独立进程 |
| 基础阅读器 no-default-features | 49 个应用单测及 2 个集成测试通过；旧听书键位覆盖被忽略，原输入表未修改 |
| 最后局部修正 | 状态机 6 个测试、阅读器进程客户端/控制器 7 个测试重新通过 |
| Clippy / fmt / rustdoc | workspace 全特性、基础阅读器 Clippy 通过；格式检查、警告作为错误的 rustdoc 通过 |
| 文档 | `cd docs && pnpm build` 通过；现有 sitemap 缺少 site 的警告不影响构建 |
| 依赖隔离 | package-specific `cargo tree -p trnovel` 及 `--no-default-features` 均不包含 novel-tts-core、Kokoro、ORT 或 rodio；协议 crate 无音频依赖 |
| 原始合成迁移 | 固定模型五段 PCM 与原 Kokoro API 最大差值均为 0，见 `tts-baseline.md` |
| 独立 CLI | 自编 UTF-8 文件完整播放返回 0，stdout 为空；无效文件在资源准备前失败 |
| CLI 终端 | 本机 PTY 自动发送暂停、恢复，再分别 q/s/Ctrl+C；三次均返回 0，退出后 termios 与进入前一致。此为自动控制，不替代人工按键与听感验收 |
| 真实协议播放 | 显式模型路径、本机音频设备；握手/配置修改/倍速/模型准备/start/pause/resume/seek/stop/completed，以及播放中 shutdown/EOF 均通过，每行 stdout 校验为协议 JSON |
| 本机构建与发行包 | 基础版 dist profile 构建成功；固定 cargo-dist 生成默认听书 ARM64 Mac 包；基础包只有 trnovel/trn，听书包额外有 novel-tts，均无模型 |
| 基础安装器 | 本机临时 HTTP 服务托管真实压缩包及 SHA256；Shell 与 npm 安装到含空格的临时目录，两个命令可运行且不含听书入口。Homebrew 公式 Ruby 语法通过 |
| 工作流与计划 | 固定 cargo-dist 重复生成工作流 SHA256 相同；`trnovel-v0.17.2` 计划包含两种四平台制品及独立安装器，并保留默认安装器和标签路由 |
| OpenSpec | `openspec validate decouple-tts-process --strict` 通过 |

真实协议复现（需要模型和可用音频设备）：

```sh
cargo build --locked -p novel-tts
python3 .github/scripts/smoke-listening.py --program target/debug/novel-tts --models target/tts-baseline
```

基础版本机安装复现：

```sh
CARGO_DIST_TARGET=aarch64-apple-darwin TRNOVEL_VARIANT_OUTPUT=target/basic-stage/aarch64-apple-darwin python3 .github/scripts/build-variant.py basic
CARGO_DIST_TARGET=aarch64-apple-darwin python3 .github/scripts/package-basic.py
python3 .github/scripts/generate-basic-installers.py --directory target/basic-distrib --platform aarch64-apple-darwin
python3 .github/scripts/smoke-basic-installers.py --directory target/basic-distrib --target aarch64-apple-darwin
```

构建产物、模型、WAV、日志在 git 忽略的 `target/`。核心及各程序的自动化测试不要求大型小说 fixture。基础与默认发行构建分开执行 Cargo 命令，不能把 workspace all-features 视为依赖隔离证明。

## 尚未通过的验收

- 主观试听与变更前后盲听；所有声明平台的真实 Kokoro 30 分钟持续播放、内存与音频卡顿记录。
- 阅读页与设置面板的人工键位、尺寸变化、开关互斥及截图。已有布局、搜索优先与控制器测试不足以替代完整 TUI 操作证据。
- 使用假后端的独立 CLI 与真实 TUI 完整链路；当前证据分为核心假后端、阅读器假子进程、协议进程及本机真实 CLI，不能称为该完整验收已完成。
- Intel Mac、GNU Linux、Windows MSVC 的真实构建、音频及安装器 smoke；基础 custom job 配置了各平台脚本/npm 本地制品安装 smoke，尚未在 CI 执行。
- PowerShell 安装与 Homebrew 实际安装/升级/切换；本机没有 pwsh，公式语法通过不等于安装通过。
- ARM64 musl readelf/干净容器实际运行。本机 Docker socket 缺失、daemon 未启动；脚本已区分基础运行库与额外音频运行库，尚未执行容器验收。
- 发布注册表/ tap / Release 的端到端结果。新 `@trnovel/trnovel-basic` 包需配置 Trusted Publisher，尚未发布；不能用旧版本安装器验证新变体。

按 openspec-apply-change 的完整验收要求，对应任务继续保持未完成。本次不归档、不发布。

## 规格场景证据索引

| 规格场景 | 当前证据 / 待补证据 |
| --- | --- |
| 基础阅读构建、共享键位、应用依赖隔离 | no-default-features 测试、active_reader_overrides 测试、两种 package-specific tree 与实际压缩包 |
| 自动续章、失败/全书结束、切章迟到事件 | controller 的实例/会话/摘要过滤及 continuation 测试；真实阅读页人工运行仍待验收 |
| 搜索重叠、阅读设置互斥/关闭/正文不滚动 | 原文范围布局及搜索测试、单一 Panel 状态门控；面板交互/截图待补 |
| 默认听书包、基础包、已支持平台 | Mac ARM64 真实制品与基础安装；其他平台及全部发行渠道待补 |
| 独立朗读/控制、无效文件 | 独立 CLI 完整播放、PTY 恢复测试、程序文件错误集成测试；手动按键待补 |
| 协议兼容、不兼容/非法输入、正文含换行 | 协议编解码测试、程序进程测试、真实模型协议 smoke |
| 首次启用、明确路径无效、跨章复用、父进程断开、程序崩溃 | reader client/controller 假子进程测试及真实 EOF/shutdown smoke |
| Kokoro 迁移、暂停背压、已合成未播放、回跳 | 固定模型 PCM 对照；session 假后端预算/释放/单终态/seek 测试、真实协议控制 |
| 末段失败、用户重试、空白正文 | 首中末段注入失败，并创建新 manager 验证恢复起点；空白/UTF8 校验测试 |
| 旧配置、UI 确认修改、损坏/并发配置 | config fixture/修订/原子保存/独立进程争用测试；controller 快照测试 |
| 重启恢复、正文变化、CLI/阅读器来源隔离 | session 与 checkpoint 故障恢复、重分段、摘要/边界/命名空间测试 |
| 首次模型准备 | 下载模拟服务器取消/续传/忽略 Range 测试，查询不创建模型目录进程测试；本机已有固定模型加载通过 |

## 命名迁移后的构建检查

`cargo check --locked --workspace --all-targets --all-features`、基础阅读器 `cargo check --locked -p trnovel --no-default-features --all-targets`、`cargo build --locked -p novel-tts` 及 `cargo fmt --all --check` 均通过。Cargo metadata 显示核心 lib 为 novel_tts_core、程序 bin 为 novel-tts；两种阅读器依赖树均没有核心或原生音频库。固定 cargo-dist 重新生成发布工作流并生成 release plan，OpenSpec 严格校验通过。本轮没有重新运行播放或安装测试。

## 架构审查后检查（2026-10-06）

本次按当前 HEAD 对比全部未提交改动开展 Standards/Spec 双轴审查，结果与修复见 `tts-review.md`。协议拆出 runtime，核心状态和音色切换统一维护，阅读器控制改为可靠的有界生命周期意图。

以下均通过：

- `cargo check --locked --workspace --all-targets --all-features`
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `cargo check --locked -p trnovel --no-default-features --all-targets`
- `cargo build --locked -p novel-tts`
- `cargo fmt --all --check`、`git diff --check`

本轮未新增或运行测试，未重新试听或验收发行包。上述静态检查不替代前文人工和跨平台待办。

## VHS 基础版 UI 验证（2026-10-06）

安装 VHS 0.12.1、ttyd 与 ffmpeg，以隔离 HOME 和原创三章文本录制 1280×800 画面，对比 `--no-default-features` 与默认听书构建。确认无 TTS 面板返回空 Fragment 会挤占正文空间：阅读设置使正文缩到上半屏，帮助使正文进一步缩小；听书构建没有该现象。

修复 `listening_panel` 的禁用分支为宽高均为零的 View。修复后已实际查看正文、设置、帮助、设置数值调整、滚动和下一章截图，正文始终铺满终端，帮助不显示听书操作，`t/p/+/-` 无听书入口。证据保存在 `target/tts-ui-vhs/`：`basic-settings.png`/`basic-help.png` 是修复前，`fixed-settings.png`/`fixed-help.png`/`fixed-next-chapter.png` 是修复后；`fixed.gif` 为操作录制。

新增可重复录制的 `docs/tapes/basic-ui.tape`，运行说明见 tapes README。基础版 49 个单测、带/不带 TTS 的 application all-target Clippy、格式及 diff 检查通过。这是本机基础阅读 UI 验证，不替代跨平台安装和真实音频验收。

正文搜索另录 `search.gif`：输入 `林舟` 命中 35 处，`n` 从第 1 处切到第 2 处并滚动，见 `search-input.png`、`search-results.png`、`search-next.png`。搜索入口实际为小写 `s`，录制使用真实绑定而非帮助中美化的大写显示。

## 提交前检查（2026-10-06）

提交前运行 `lefthook run pre-commit`，工作区 all-features 测试（lib/tests/examples）、Clippy、rustfmt 和 rustdoc 四项全部通过。此结果补充此前架构审查阶段未重跑测试的记录；人工试听及跨平台待办仍保留。
