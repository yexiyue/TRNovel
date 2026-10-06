# 工具链 / 工程

## 概览

Cargo workspace 的模块组织、feature 门控、构建/发布、平台坑。`Cargo.toml`(根)、`crates/*/Cargo.toml`、`lefthook.yaml`、`.github/workflows`、`release.sh`。

## 模块组织

### 模块名.rs 与子模块目录

按用户在 2026-10-06 对听书重构的要求，新增和重构模块使用 **`foo.rs`** 定义模块；只有存在子模块时才建立同名 `foo/` 目录，子模块以 `foo/bar.rs` 定义。`lib.rs` 或父模块使用 `mod foo;` 声明，目录本身不再需要 `mod.rs`。

移动文件时同步 `include_str!`/`include_bytes!` 的相对路径，保持外部模块路径和 re-export 符合本次接口设计。仓库中既有 `mod.rs` 布局属于历史代码，按实际重构范围迁移。

**相关文件**：`crates/novel-tts-protocol/src/{codec.rs,message.rs}`、`crates/novel-tts-core/src/{config.rs,checkpoint.rs,storage.rs}`。

### include_str! 路径随文件深度变

把 `source.rs` 移到 `source/mod.rs` 后多了一层目录,`include_str!("../book-source.schema.json")` 要改成 `../../`。移动含 `include_str!`/`include_bytes!` 的文件时记得同步相对路径。

**相关文件**：`crates/parse-book-source/src/source/mod.rs`(schema_sync 测试)

## 依赖钉版（勿随意升级）

### ort / kokoro-tts 钉死，勿升

- `ort` 钉死 `2.0.0-rc.10`（onnxruntime 绑定）。
- `kokoro-tts` 钉死 `0.3.1`——**`rc.12` 砍掉了 Intel Mac 支持,不要升**。
- 普通发布目标使用 ort 预编译库；ARM64 musl 使用 Alpine 系统共享库，详见下方 musl 发布说明。

**相关文件**：`crates/novel-tts-backends/Cargo.toml`、根 `Cargo.toml`

## 构建 / 发布

### 平台坑

- **Edition 2024 + std 文件锁**：需 Rust ≥ 1.89（stable，无需 nightly）。`parse-book-source` 的 browser-pool 用 `std::fs::File::try_lock` 做跨进程启动临界区锁；该 API 稳定于 1.89。
- **Linux 构建依赖**：`libasound2-dev`（rodio）、`libssl-dev`、`pkg-config`，CI 用 apt 装。
- **Windows**：根 `Cargo.toml` 的 `[workspace.metadata.dist]` 里 `msvc-crt-static = false` **必须保留**——ort/onnxruntime 是动态 CRT，静态链接会 `__imp_tolower` 等 unresolved-symbol LNK 错。

### pre-commit / 发布链

`lefthook.yaml` pre-commit：test → `clippy --fix --allow-dirty`（自动 stage 修复）→ `cargo fmt` → `cargo doc`。发布走 `./release.sh`（cargo-release + git-cliff，tag `<crate>-v<version>`）+ cargo-dist（`trnovel-v*` tag 触发）。

### VHS 录制要显式清 NO_COLOR

Codex / CI shell 可能带 `TERM=dumb` 或 `NO_COLOR=1`，会让 ratatui/crossterm 抑制样式码，录出来的 GIF 接近黑白。所有 `docs/tapes/*.tape` 都要在 `Env TERM "xterm-256color"` 与 `Env COLORTERM "truecolor"` 后补 `Env NO_COLOR ""`，并用 VHS 内置 `Screenshot` 验证关键帧颜色。

**相关文件**：`docs/tapes/README.md`、`docs/tapes/*.tape`

### 改引擎公开 API 后要单独 cargo build 验 Send

子 crate 的 lib test **测不出** `tokio::spawn` 上下文的 `Send` 约束。改了会被主程序 spawn 调用的引擎公开 API（如 `Engine::explore`/`search`）后，CI 四件套之外还要 `cargo build` 主程序 trnovel——曾因 `explore`/`search` 的 Future 变 `!Send`（async closure 参数）导致主程序编译失败、`cargo run` 跑不起来。

**相关文件**：`crates/parse-book-source/src/engine/mod.rs`

### ort 固定版本由后端 crate 管理

`ort` 和 `kokoro-tts` 的版本统一固定在根 workspace.dependencies。novel-tts-backends 的 MOSS 实现直接调用 ort，Kokoro 也通过该依赖固定其传递版本。不要删除或放宽固定版本；rc.10 保留当前 Intel Mac 和 GNU Linux 发布兼容性。

**相关文件**：根 `Cargo.toml`、`crates/novel-tts-backends/Cargo.toml`。

### ARM64 musl release artifact

`local-artifacts-jobs = ["./musl"]` extends cargo-dist without hand-editing the generated workflow. The reusable musl workflow builds both application binaries natively in an ARM64 Alpine 3.23 Rust container and uploads `artifacts-build-musl`; the generated host job includes these files in the GitHub Release. It runs on PRs separately because the main dist workflow normally only plans PR releases.

The pinned ort rc.10 requires ONNX Runtime API 1.22. Its GNU prebuilt libraries are incompatible with musl, so the musl build uses Alpine's system ONNX Runtime with `ORT_LIB_LOCATION`, `ORT_PREFER_DYNAMIC_LINK=1`, `ORT_SKIP_DOWNLOAD=1`, and `-C target-feature=-crt-static`. This preserves TTS but requires runtime shared libraries. Do not add this target to dist's ordinary matrix until its build and installer dependency handling supports this setup. The custom archive is currently a manual download rather than an installer-selected platform.

**相关文件**：`.github/workflows/musl.yml`、`.github/scripts/build-musl.sh`、`.github/scripts/smoke-musl.sh`、`Cargo.toml`。

### npm Trusted Publishing

npm publishes through the reusable `publish-npm.yml` workflow with Node 24, npm 11 and `id-token: write`; no `NPM_TOKEN` is passed. Configure two npm GitHub trusted publishers for `yexiyue/TRNovel`: `trnovel-release.yml` (the caller identity used for normal releases) and `publish-npm.yml` (manual recovery from existing release assets). Enable direct `npm publish`; dist-tag management is unnecessary. The manual workflow only publishes on main and checks the package name, repository and version against the requested tag. PRs validate the existing release archive with a dry run, without publishing.

Change cargo-dist metadata and regenerate `trnovel-release.yml`; do not edit the generated file directly. Rerunning an old failed release uses its original workflow, so recover with `Publish npm with OIDC` workflow_dispatch on main instead.

**相关文件**：`Cargo.toml`、`.github/workflows/publish-npm.yml`、`.github/scripts/publish-npm.sh`。

npm publish-time scanning can delay registry availability by several minutes after a successful upload. Poll the public version before declaring success; do not re-upload while scanning is pending. Validate existing archives with `npm pack --dry-run`, because `npm publish --dry-run` still rejects already-published versions before a retry can skip them.

### 基础阅读版与听书进程的依赖隔离

根 `tts` feature 仅装配听书 UI、JSON Lines 协议及进程客户端，默认启用。基础阅读版用 `cargo build -p trnovel --no-default-features`，配套程序用 `cargo build -p novel-tts`。两种阅读器的 package-specific dependency tree 均没有 novel-tts-core、kokoro-tts、ort、rodio；不要以 workspace all-features 的依赖集合代替这项证明。

`novel-tts-core` 不包含模型 feature；独立程序默认 moss feature，kokoro 可选，固定原生推理依赖由 novel-tts-backends 承担。核心拥有通用 rodio 播放器。此变更的发布安装渠道与跨平台试听仍由 OpenSpec 的未完成任务跟踪，不能把本机 check 或假进程测试写成平台发布验收。

**相关文件**：`Cargo.toml`、`crates/novel-tts/Cargo.toml`、`openspec/changes/decouple-tts-process/tasks.md`。


### 双变体 cargo-dist 发布

固定 cargo-dist 0.32.0 配置位于 `dist-workspace.toml`，`dist.toml` 描述泛用构建：默认听书包保留 `trnovel-v*` 标签及旧安装器名称，包含三个二进制。基础版由 custom local job 生成，仅包含两个阅读器；global extra-artifacts 校验基础压缩包并生成独立 shell/PowerShell/npm/Homebrew 安装器。

`release.sh` 在升级版本后执行 `sync-dist-version.py` 保持 generic manifest 同步。`trnovel-basic.formula` 由自定义发布 job 改名为 `.rb`，避免 cargo-dist 默认 Homebrew job 把多个公式当成一个文件。新 npm 包首次发布前必须在注册表配置相应 Trusted Publisher，现有包的授权不自动覆盖新包。

构建程序分别调用 package-specific Cargo 命令，防止 workspace feature unification 给阅读器带入原生音频依赖。ARM64 musl 先在无音频包的干净容器运行基础版，再装 ALSA/ONNX Runtime 检查听书程序。当前本机 Docker daemon 未启动，Windows/Intel Mac/GNU Linux 与 musl 实机验收仍依赖对应环境，不能以本机构建代替。

### 听书包与模块命名

`novel-tts-core` 是会话/合成/播放库，`novel-tts-protocol` 是轻量协议库，`novel-tts` 是独立程序 crate 及命令。依赖键用 `tts-core` / `tts-protocol` 显式声明 package 名，Rust 用 `tts_core` / `tts_protocol` 引用。阅读器可选模块为 `src/tts.rs` 与 `src/tts/`。

CLI 接管原 novel-tts 的包名，保持 0.3.0 版本线，后续发布需递增；核心新包同样暂用 0.3.0。模型目录 `.novel-tts/kokoro` 与配置/检查点格式保持原样。更新包名时同步 crates.io 标签到目录的发布路由、cargo-dist binary 清单与同目录/PATH 程序发现。

### MOSS 后端与验收隔离

新模型实现在 novel-tts-backends；共享原生依赖版本位于 workspace.dependencies。novel-tts 默认 moss，`--features kokoro` 编入两个后端，`--no-default-features --features kokoro` 只编入 Kokoro。阅读器保持协议依赖隔离。

MOSS ONNX opset 17 的实际加载、生成和编解码已在本机固定 ort rc.10 上验证，不需升级原生运行时。SentencePiece 使用纯 Rust sentencepiece-rs，参考 WAV 用 hound 解码和 rubato sinc 重采样，无 Python 运行依赖。资源固定 revision、尺寸和 SHA-256；音色缓存绑定模型版本。

VHS 验收不同 feature 的阅读器时，把构建出的 basic 二进制复制到独立目录后再录制。随后运行 workspace all-features 测试会重建 target/debug/trn；若继续录制这个共享路径，会误把完整听书版当成基础版。

**相关文件**：`crates/novel-tts-backends/README.md`、`dev-notes/moss-tts-acceptance.md`。

### ORT 加速 feature 与模型校验

保留 ort=2.0.0-rc.10。coreml/cuda 仅由听书后端/程序 feature 启用，阅读器始终不链接它们。ORT 自带下载清单选择 CUDA12 的原生分发，Mac 可静态链接 CoreML 框架；跨平台原生运行与 CUDA/cuDNN 依赖必须在对应平台验收。本机 Mac 启用 cuda feature 会下载 CPU 原生包，因此 cargo check 不能证明 CUDA 可用。

开发构建将 sha2 单包 opt-level=3，避免每次准备模型时对 GB 级权重执行慢速 debug 校验；保留每次大小与 SHA-256 校验。性能校准应使用 release 构建。多包 cargo build 配合 --bin trn 只构建名为 trn 的程序；更新 worker 必须单独 cargo build -p novel-tts，不能依据阅读器构建完成判断 worker 已更新。

### 轻量 TOC 常量共享

内置章节数字、中文/英文/特殊标题正则常量在 novel-tts-protocol::headings 共享，protocol 不编译正则、不处理书籍。基础阅读版因此也依赖该轻量 crate，但仍不依赖 TTS core/backends/ORT/rodio。模型诊断 example 明确使用输出目录，真实模型测试通过 TRNOVEL_MOSS_MODEL_DIR 启用。

### Candle Qwen 与平台设备

Qwen TTS 使用 Git 钉版 TrevorS/qwen3-tts-rs，首版 0.6B CustomVoice。qwen-only 构建不引入 ORT；阅读器也不引入 Candle。上游尚未发布对应 crates.io 包，后端 crate 独立发布前需要解决该 Git 依赖分发，不能直接照旧发布。

Metal 通过 macOS target-specific 的 candle-core 别名依赖启用，避免全 workspace/all-features Linux 构建启用 Apple 原生依赖；非 Mac 的 metal feature 不报告可用 Metal。不要直接把 qwen3-tts/metal 的全局 feature 转发到所有平台。设备目录由 Registry 按后端提供，对齐仍使用 ORT provider，不能复用 TTS 的 Metal 候选。校准缓存文件名包含 key 摘要，避免切换模型互相覆盖记录。

**相关文件**：`crates/novel-tts-backends/src/qwen.rs`、`src/qwen/runtime.rs`、`src/devices/calibration.rs`、后端 Cargo.toml。

- Candle Metal 必须同时启用 core/nn/transformers 的 metal feature。只启用 candle-core 能加载 Qwen 权重，但推理会报 `no metal implementation for rms-norm`；验收必须包含真实 PCM 生成。对应依赖仍仅在 macOS target 启用。

### 品牌主资产与官网导出

终端机器人“小卷”的标志、路径字标、透明 PNG 与品牌规范统一在 `assets/brand/`。官网配置和 Hero 直接引用这个目录的主资产，Astro 可以构建文档根目录以外的静态导入；不要在 `docs/src/assets/` 手动维护另一套标志副本。

矢量源使用 `source/build_vectors.py` 和随附 OFL 字体，字标导出为路径；`source/export_images.mjs` 使用文档站现有 sharp 依赖导出 app icon 与 `docs/public/brand/social-card.png`。favicon 为小尺寸单独简化。更新资源后运行导出脚本与 `pnpm build`，核对深浅主题、窄屏和 GitHub Pages `/TRNovel` 路径。

**相关文件**：`assets/brand/README.md`、`assets/brand/source/`、`docs/astro.config.mjs`、`docs/src/components/landing/Hero.astro`。
