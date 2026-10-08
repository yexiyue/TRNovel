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

### ort 钉版

- `ort` 固定 `2.0.0-rc.13`（原生预编译 ONNX Runtime 1.28），Rust API 显式选择 `api-21`（应用原生库仍为 1.22 或更新）。
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

`ort` 的版本统一固定在根 workspace.dependencies。MOSS Nano 与对齐器仍使用 ORT，不要删除或放宽固定版本；rc.13 支持多版本 API；用户已授权取消 Intel Mac 发布，普通目标使用新的预编译库。

**相关文件**：根 `Cargo.toml`、`crates/novel-tts-backends/Cargo.toml`。

### ARM64 musl release artifact

`local-artifacts-jobs = ["./musl"]` extends cargo-dist without hand-editing the generated workflow. The reusable musl workflow builds both application binaries natively in an ARM64 Alpine 3.23 Rust container and uploads `artifacts-build-musl`; the generated host job includes these files in the GitHub Release. It runs on PRs separately because the main dist workflow normally only plans PR releases.

The pinned ort rc.13 explicitly enables API 21 while shipping ONNX Runtime 1.22 or newer. Its GNU prebuilt libraries are incompatible with musl, so the musl build uses Alpine's system ONNX Runtime with `ORT_LIB_PATH`, `ORT_PREFER_DYNAMIC_LINK=1`, `ORT_SKIP_DOWNLOAD=1`, and `-C target-feature=-crt-static`. This preserves TTS but requires runtime shared libraries. Do not add this target to dist's ordinary matrix until its build and installer dependency handling supports this setup. The custom archive is currently a manual download rather than an installer-selected platform.

**相关文件**：`.github/workflows/musl.yml`、`.github/scripts/build-musl.sh`、`.github/scripts/smoke-musl.sh`、`Cargo.toml`。

### npm Trusted Publishing

npm publishes through the reusable `publish-npm.yml` workflow with Node 24, npm 11 and `id-token: write`; no `NPM_TOKEN` is passed. Configure two npm GitHub trusted publishers for `yexiyue/TRNovel`: `trnovel-release.yml` (the caller identity used for normal releases) and `publish-npm.yml` (manual recovery from existing release assets). Enable direct `npm publish`; dist-tag management is unnecessary. The manual workflow only publishes on main and checks the package name, repository and version against the requested tag. PRs validate the existing release archive with a dry run, without publishing.

Change cargo-dist metadata and regenerate `trnovel-release.yml`; do not edit the generated file directly. Rerunning an old failed release uses its original workflow, so recover with `Publish npm with OIDC` workflow_dispatch on main instead.

**相关文件**：`Cargo.toml`、`.github/workflows/publish-npm.yml`、`.github/scripts/publish-npm.sh`。

npm publish-time scanning can delay registry availability by several minutes after a successful upload. Poll the public version before declaring success; do not re-upload while scanning is pending. Validate existing archives with `npm pack --dry-run`, because `npm publish --dry-run` still rejects already-published versions before a retry can skip them.

### 基础阅读版与听书进程的依赖隔离

根 `tts` feature 仅装配听书 UI、JSON Lines 协议及进程客户端，默认启用。基础阅读版用 `cargo build -p trnovel --no-default-features`，配套程序用 `cargo build -p novel-tts`。两种阅读器的 package-specific dependency tree 均没有 novel-tts-core、kokoro-tts、ort、rodio；不要以 workspace all-features 的依赖集合代替这项证明。

`novel-tts-core` 不包含模型 feature；独立程序默认 moss feature，固定原生推理依赖由 novel-tts-backends 承担。核心拥有通用 rodio 播放器。此变更的发布安装渠道与跨平台试听仍由 OpenSpec 的未完成任务跟踪，不能把本机 check 或假进程测试写成平台发布验收。

**相关文件**：`Cargo.toml`、`crates/novel-tts/Cargo.toml`、`openspec/changes/decouple-tts-process/tasks.md`。


### 双变体 cargo-dist 发布

固定 cargo-dist 0.32.0 配置位于 `dist-workspace.toml`，`dist.toml` 描述泛用构建：默认听书包保留 `trnovel-v*` 标签及旧安装器名称，包含三个二进制。基础版由 custom local job 生成，仅包含两个阅读器；global extra-artifacts 校验基础压缩包并生成独立 shell/PowerShell/npm/Homebrew 安装器。

`release.sh` 在升级版本后执行 `sync-dist-version.py` 保持 generic manifest 同步。`trnovel-basic.formula` 由自定义发布 job 改名为 `.rb`，避免 cargo-dist 默认 Homebrew job 把多个公式当成一个文件。新 npm 包首次发布前必须在注册表配置相应 Trusted Publisher，现有包的授权不自动覆盖新包。

构建程序分别调用 package-specific Cargo 命令，防止 workspace feature unification 给阅读器带入原生音频依赖。ARM64 musl 先在无音频包的干净容器运行基础版，再装 ALSA/ONNX Runtime 检查听书程序。当前本机 Docker daemon 未启动，Windows/GNU Linux 与 musl 实机验收仍依赖对应环境，不能以本机构建代替。

### 听书包与模块命名

`novel-tts-core` 是会话/合成/播放库，`novel-tts-protocol` 是轻量协议库，`novel-tts` 是独立程序 crate 及命令。依赖键用 `tts-core` / `tts-protocol` 显式声明 package 名，Rust 用 `tts_core` / `tts_protocol` 引用。阅读器可选模块为 `src/tts.rs` 与 `src/tts/`。

CLI 接管原 novel-tts 的包名，保持 0.3.0 版本线，后续发布需递增；核心新包同样暂用 0.3.0。模型公共根目录 `.novel-tts` 与配置/检查点路径保持原样。更新包名时同步 crates.io 标签到目录的发布路由、cargo-dist binary 清单与同目录/PATH 程序发现。

### MOSS 后端与验收隔离

新模型实现在 novel-tts-backends；共享原生依赖版本位于 workspace.dependencies。novel-tts 默认 moss，CPU 默认仅 MOSS Nano；Kokoro 和 ZipVoice 已移除。阅读器保持协议依赖隔离。

MOSS ONNX opset 17 原始验收使用 ort rc.10，升级到 rc.13 后需重测加载、生成与编解码。SentencePiece 使用纯 Rust sentencepiece-rs，参考 WAV 用 hound 解码和 rubato sinc 重采样，无 Python 运行依赖。资源固定 revision、尺寸和 SHA-256；音色缓存绑定模型版本。

VHS 验收不同 feature 的阅读器时，把构建出的 basic 二进制复制到独立目录后再录制。随后运行 workspace all-features 测试会重建 target/debug/trn；若继续录制这个共享路径，会误把完整听书版当成基础版。

**相关文件**：`crates/novel-tts-backends/README.md`、`dev-notes/moss-tts-acceptance.md`。

### ORT 加速 feature 与模型校验

固定 ort=2.0.0-rc.13，显式启用 api-21。coreml/ort-cuda 仅由听书后端/程序 feature 启用，阅读器始终不链接它们。ORT 自带下载清单仅提供 CUDA13 的原生分发，Mac 可静态链接 CoreML 框架；跨平台原生运行与 CUDA/cuDNN 依赖必须在对应平台验收。本机 Mac 启用 ort-cuda feature 会下载 CPU 原生包，因此 cargo check 不能证明 CUDA 可用。

开发构建将 sha2 单包 opt-level=3，避免每次准备模型时对 GB 级权重执行慢速 debug 校验；保留每次大小与 SHA-256 校验。性能校准应使用 release 构建。多包 cargo build 配合 --bin trn 只构建名为 trn 的程序；更新 worker 必须单独 cargo build -p novel-tts，不能依据阅读器构建完成判断 worker 已更新。

### 轻量 TOC 常量共享

内置章节数字、中文/英文/特殊标题正则常量在 novel-tts-protocol::headings 共享，protocol 不编译正则、不处理书籍。基础阅读版因此也依赖该轻量 crate，但仍不依赖 TTS core/backends/ORT/rodio。模型诊断 example 明确使用输出目录，真实模型测试通过 TRNOVEL_MOSS_MODEL_DIR 启用。

### 本地 Candle Qwen 与平台设备

Qwen 推理源码位于 `crates/qwen3-tts`，来自 TrevorS/qwen3-tts-rs revision `711ceee07cad92673f86de8997bdf54c30caa49f`（MIT）。仅纳入推理库，移除上游 CLI、Hub 下载、Flash Attention 和自定义 PTX；下载/校验、校准、会话与播放仍由既有层管理。Candle core/nn/transformers 统一钉为 0.9.2。

**正确做法**：
- `qwen` 是 CPU 后端；`qwen-cuda` 同时启用 Qwen 和三套 Candle CUDA 算子；`metal` 启用 Candle core/nn 的 Metal 算子，仅在 macOS 构建。`ort-cuda` 独立用于 MOSS/对齐器，不保留旧 `cuda` 别名。
- GPU feature 经 `tts-candle-platform` 按 target 路由，同一 Candle 0.11.0 同时服务 Qwen 和 Omni。Windows/Linux 的 CUDA 依赖和 macOS Metal 依赖分别生效；Windows all-features 不会编入 Objective-C。CUDA 构建仍需 Toolkit，通用 CI/lefthook 使用 CPU feature 集合，平台 GPU CI 单独启用。
- `device.rs` 统一设备创建；显式 CUDA 使用 `Device::new_cuda`，避免 `cuda_if_available` 静默返回 CPU。Registry 继续按后端提供编译/可用设备，对齐器使用自己的 ORT provider。
- CUDA 编译需要 Toolkit/`nvcc`；驱动提供的 `nvidia-smi` 不代替 Toolkit。RTX 5070 的 CC 为 12.0，使用支持 Blackwell 的 Toolkit；无 GPU 构建机显式设置 `CUDA_COMPUTE_CAP`。
- tokenizers 仅启用推理用 `onig`，不启用训练用 `esaxx_fast`。上游 esaxx-rs 的 `.static_crt(true)` 与 ORT 的 `/MD` 在 Windows 导致 LNK2038/LNK2005；从依赖 feature 源头移除 C++ 加速，不使用 `/NODEFAULTLIB` 或全局 `/MD` workaround。
- 模型/音色/检查点格式不变；校准实现标识更新为 `qwen-local-v1`，使旧性能缓存失效。

**相关文件**：`crates/qwen3-tts/README.md`、`crates/novel-tts-backends/src/qwen.rs`、`src/qwen/runtime.rs`、`src/devices/calibration.rs`。

### 品牌主资产与官网导出

终端机器人“小卷”的标志、路径字标、透明 PNG 与品牌规范统一在 `assets/brand/`。官网配置和 Hero 直接引用这个目录的主资产，Astro 可以构建文档根目录以外的静态导入；不要在 `docs/src/assets/` 手动维护另一套标志副本。

矢量源使用 `source/build_vectors.py` 和随附 OFL 字体，字标导出为路径；`source/export_images.mjs` 使用文档站现有 sharp 依赖导出 app icon 与 `docs/public/brand/social-card.png`。favicon 为小尺寸单独简化。更新资源后运行导出脚本与 `pnpm build`，核对深浅主题、窄屏和 GitHub Pages `/TRNovel` 路径。

**相关文件**：`assets/brand/README.md`、`assets/brand/source/`、`docs/astro.config.mjs`、`docs/src/components/landing/Hero.astro`。

### 首页演示的静帧与播放控制

首页演示默认加载 WebP 静帧，点击播放才请求对应 GIF；切换演示或暂停时恢复静帧。静帧由 `node docs/scripts/export-landing-posters.mjs` 从现有 VHS 录屏中选帧导出，更新录屏后需重新选择有完整界面的帧。减少动效偏好切换时停止播放；不把 GIF 交给 Astro Image 优化，否则会丢失动画。自定义 Hero 的主标题保留 `_top` ID，供 Starlight 的跳转内容链接定位。

**相关文件**：`docs/src/components/landing/Gallery.astro`、`docs/scripts/export-landing-posters.mjs`。

### 听书基线 fixture 固定 LF

`baseline-segments.json` 保存 narration.txt 的原始 UTF-8 字节坐标；Windows 的 Git autocrlf 会把 LF 改为 CRLF，导致基线偏移逐行增加。`.gitattributes` 将这个 fixture 固定为 `text eol=lf`；真实 CRLF 坐标行为由单独测试验证，不能通过规范化生产输入来掩盖 fixture 的换行差异。

### Windows CUDA 12.9 初始安装记录

此前 CUDA 12.9 Update 1 从 NVIDIA 官方 redistrib Windows ZIP 组件安装到 `D:\dev-tools\cuda\v12.9`，逐包按官方 manifest 校验 SHA-256，保留现有显示驱动。包含 nvcc、运行时、数学库、头文件、命令行工具与示例；不安装 Nsight GUI 或 Visual Studio 项目集成。当时 `CUDA_PATH` / `CUDA_PATH_V12_9` 指向该目录，系统 PATH 添加 CUDA `bin` 与 VS 2022 的 x64 MSVC 编译器目录；当前默认已升级为下一节的 CUDA 13。

**正确做法**：
- 重开终端或父进程以加载系统环境，CMD/PowerShell 均不需要激活脚本。Rust 自行发现 MSVC 的 linker 不代表 nvcc 可以找到 cl.exe；后者需要编译器目录在 PATH 中，随后 nvcc 自行加载 VS 编译环境。
- 系统 `NVCC_PREPEND_FLAGS=-Xcompiler=/MD` 使 CUDA host C++ 与 Rust/ORT 使用动态 CRT。Candle 0.9.2 的 MOE 静态库默认按 `/MT` 编译，与默认 MOSS 所需的 ORT `/MD` 组合时发生 LNK2038；配置后先 `cargo clean -p candle-kernels` 重建旧对象。此选项影响使用系统环境的所有 nvcc 调用，不使用 `/NODEFAULTLIB` 掩盖冲突。
- 初次编译使用 `$env:RAYON_NUM_THREADS = '2'` 和 `cargo build --locked -j 2 -p novel-tts --no-default-features --features qwen-cuda`；Cargo 并行数不能约束 bindgen_cuda 内部的 Rayon 内核编译线程数。
- nvcc 12.9.86 与本机 MSVC 14.44 配合完成 `sm_120` 内核编译和 RTX 5070 实际执行；官方 deviceQuery / vectorAdd 也通过。该结果证明 Toolkit/驱动工作，不能代替真实 Qwen PCM/EOS 与听感验收。

安装清单和哈希：`D:\dev-tools\cuda\v12.9\installation-manifest.json`；本机使用说明：`D:\dev-tools\cuda\README.txt`。

### Windows ORT rc.13 与 CUDA 13（2026-10-07）

同步远程 TTS 分支后，ORT 固定 rc.13 / ONNX Runtime 1.28，发布目标仅保留
Apple Silicon macOS、Windows 和 Linux；不恢复 Intel Mac、Kokoro 或 ZipVoice。
CUDA 13.0 Update 2 安装到 `D:\dev-tools\cuda\v13.0`，cuDNN 9.14 CUDA13
安装到 `D:\dev-tools\cudnn\v9.14-cuda13`。安装仅使用 NVIDIA 官方 redistrib，
按官方 SHA-256 校验；旧 Toolkit 12.9 保留。下载缓存迁移至 C 盘，避免 D 盘
构建空间不足。

CUDA 13 的运行库位于 `bin\x64`，系统 PATH 要同时包含 `bin` 和 `bin\x64`，
以及 cuDNN 的 `bin`。保留 `NVCC_PREPEND_FLAGS=-Xcompiler=/MD`，使 Candle
host 对象与 ORT 共用动态 CRT。ORT 构建显式设置 `ORT_CUDA_VERSION=13`；
旧终端须重新加载环境。Linux Candle CUDA 构建镜像同步为 13.0.2；Windows
构建和真实 provider 检查结果见 `dev-notes/ort-rc13-upgrade.md`，不以 Windows
feature 检查替代 Linux CUDA 或 macOS Metal 验收。

独立 `voxcpm-sys` 对照工具在 CMake 3.31 / CUDA 13 下自动 `native` 架构探测
失败；本机完整 workspace CUDA 检查显式设 `CUDA_COMPUTE_CAP=120`。该值是
RTX 5070 的硬件算力；其他设备不能照搬。生产 Candle 推理不依赖此 C++ 工具。

### 多模型原生 TTS 与 CMake

新增 `crates/voxcpm-sys` 固定 llama.cpp-omni 静态 C ABI，`crates/omnivoice` 只保留标准 Candle 推理。native 依赖只经 worker 引入；阅读器仅使用 protocol。CMake/cc 置于 workspace dependencies。Windows 使用 Ninja 与 cc::windows_registry 得到的 MSVC 环境，无需 CUDA Visual Studio 插件；VS generator 在 ZIP Toolkit 安装中会报 No CUDA toolset。本机 CMake 3.31.6 / Ninja 1.11.1.4 位于 D:/dev-tools/bin。

Omni tokenizer 权重为 BOSON/Higgs/Llama 条款，不能跟生成器一起标为 Apache；`crates/omnivoice/LICENSE.Higgs-Audio` 保存原始许可。

**相关文件**：`crates/tts-candle-platform/`、`crates/voxcpm-sys/build.rs`、`.github/workflows/qwen-cuda.yml`。

### MOSS 统一 Candle 试用

`crates/moss-tts` 与 Qwen/Omni 共用 Candle 0.11.0。worker 使用 `moss-candle-cuda` / `moss-candle-metal`，不把推理依赖引入阅读器。CUDA 大模型 BF16、codec F16；VoiceGenerator F16 的真实权重会产生无效 logits。四组权重按官方固定 revision 和 SHA 放用户缓存，7.1 GB codec 共用一次，不按每个生成模型复制。打包可选 feature 时附带 moss-tts LICENSE/NOTICE。CPU 库检查不等于完整大模型 CPU 验收；当前 worker 新模式仅公开 GPU。

**相关文件**：`crates/moss-tts/`、`crates/novel-tts-backends/src/moss/candle/`、`dev-notes/moss-candle-acceptance.md`。


### macOS VoxCPM2 原生链接

VoxCPM2 的 vendored GGML 在 Apple 平台默认启用 BLAS；Rust 静态链接必须同时包含 `ggml-blas` 和 Accelerate 框架，否则最终链接报 `_ggml_backend_blas_reg` 未定义。仅完成 CMake 构建或 `cargo check` 不能发现该问题，必须构建实际 worker/example。

**相关文件**：`crates/voxcpm-sys/build.rs`。

VoxCPM2 设备探测要匹配 vendored GGML 的注册名 `MTL`（实际 backend 名为 `MTL0`），不是 UI/协议名 `Metal`。显式设备加载后的防 CPU 回退检查也必须使用同一原生名称；只检查 GPU 日志或 Metal feature 编译通过会漏掉这个错误。

### OmniVoice Metal 的长向量排序限制

Candle 0.9.2 的 Metal `arg_sort_last_dim` 使用单线程组 bitonic sort，线程数为列数的下一个 2 次幂。列数超过 1024 时会超出线程组限制，返回损坏的索引；OmniVoice 的位置选择会残留 mask token，进而生成噪音。8 个 codebook、130 帧已触发；强制 F32 不能修复。

**正确做法**：OmniVoice 在 Metal 且排序列数 >1024 时仅把分数排序放在 CPU，将索引传回原设备；位置选择和 class top-k 共用此保护。模型计算仍在 Metal，CUDA 路径不变。Stage1 普通解码也校验 token 范围，避免 Metal embedding 的越界截断掩盖异常。回归测试覆盖 1040 个位置，以及 1024/1025/4097 列 class top-k。

**相关文件**：`crates/omnivoice/src/stage0_model.rs`、`crates/omnivoice/src/stage1_decoder.rs`；实测见 `dev-notes/metal-tts-efficiency.md`。

### CPU 后端收敛与旧配置迁移

2026-10-07 移除 Kokoro、ZipVoice adapters、features、专用依赖和 ZipVoice 的 vendored eSpeak/CMake helper。保留 MOSS Nano 为 CPU 默认，现有 GPU 模型继续保留。ORT 仍由 Nano 与强制对齐使用；不要随旧后端一起删除。worker 启动时将退休后端及无 backend 的旧配置迁移为 Nano 默认音色、CPU；文件锁内校验并原子保存，保留其他偏好与未知字段，revision 只增加一次。用户缓存不删除。

**相关文件**：`crates/novel-tts-core/src/config.rs`、`crates/novel-tts/src/main.rs`、`crates/novel-tts-backends/src/lib.rs`。

### ORT rc.13 升级与当前平台要求

2026-10-07 用户明确授权升级到最新 rc.13，Windows 配套升级 CUDA 13；不把 CUDA 12 机器上编译通过等同于新版 ORT CUDA 可运行。采用新 `ort::ep::{CoreML, CUDA}`、`session::RunOptions` 和 `Session::outputs()` / `Outlet::name()` API。SessionBuilder 的错误携带非 Send/Sync 的可恢复 builder，跨 anyhow 边界保留错误消息并正常销毁 builder，不通过 `.recover()` 忽略设备初始化错误。

workspace 显式关闭 ORT 默认 feature，选择 `api-21` 关闭 ORT 默认自动 EP policy，设备继续由应用显式管理，同时启用原来使用的 std/ndarray/tracing/下载/复制和 pkg-config。`lax-feature-matching` 仅让通用 all-features 在目标平台选择可用预编译包（例如 Mac 无 CUDA）；运行期仍检查设备可用性且 EP 注册 `error_on_failure`，CUDA 发布还检查实际 provider 库，不能静默打包 CPU 替代物。

Intel Mac 上游已停止提供新包，用户授权取消 Intel Mac 的基础版和听书版发布；不维护旧 ORT 静态包的兼容路径。musl 保持系统库 API 22。新 Windows/Linux 默认预编译 x86-64-v3 至少需要 Haswell/Zen 等对应指令集；Apple Silicon 新原生包需要 macOS 13.4 或更新。依赖升级与 CUDA 13 原生验收是两件事，后者留待 Windows。

**相关文件**：根 `Cargo.toml`、`.github/scripts/build-variant.py`、`.github/workflows/accelerated.yml`。

rc.13 开启 api-22 时，SessionBuilder 默认设置 MaxEfficiency 自动 EP policy，显式注册 CPU 也不能清掉该 policy。应用使用 api-21 关闭这段未需要的自动策略，继续由自身校准选择并显式注册设备。ORT 1.28 的 MLProgram 在 Nano 生成时出现 Shape/Slice 错误；改用 NeuralNetwork 格式完成生成，不能据此声称有加速。CoreML 编译缓存以格式及原生 `ort::info()` SHA 隔离，避免跨原生运行时复用旧编译分区；旧缓存保留。


### 发布 smoke 与协议版本同步

`smoke-variant.py` 与 musl smoke 从 `novel-tts-protocol/src/lib.rs` 读取 `PROTOCOL_VERSION` 构造握手，避免写死的旧协议使新版 worker 误报 incompatible_version。协议升级时无需再重复更新 smoke 常量；实际 worker 的 ready 和 shutdown accepted 仍需验证。

### VoxCPM Candle 候选与数值门槛

`crates/voxcpm` 直接读取现有 Q8_0 BaseLM/F16 Acoustic GGUF，统一 Candle（初始 0.9.2，现升级 0.11.0）。严格 F32 对照未全部通过；用户随后明确接受差异先接入，生产 `voxcpm` 已切换 Candle。吞吐通过及用户接受差异均不代替数值、听感与连续播放验收。

原始权重对照增加 `Model::load_original`，网络 BF16/F16，AudioVAE F32。官方 `audiovae.pth` 的参数在 `state_dict` 中，须用 Candle `from_pth_with_state`；归一化卷积通过 Rust 合并 `weight_g/weight_v`，沿非第零维求范数。Safetensors 原名与既有计算层名的映射在权重入口集中处理，不复制网络实现。权重固定到 32279effe8c19989596f05d353d1447f51d9e915；比较工具通过显式 precision 选择，生产目录及 Q8 模型选择继续保持。

CFM 只缓存一个步数对应的时间嵌入表，每个 patch 的 cond projection 只计算一次。步数变化重建时间表，取消中断不提交部分表；不可缓存依赖扩散输入的 LocalDiT 深层 KV。真实固定噪声 oracle、步数切换及取消复用通过；GPU Q8 的 11 段 WAV 与修改前逐字节相同。

**正确做法**：
- GGUF `voxcpm.model_version` 是 F32，而非字符串。
- 共享 Linear 在调用 QMatMul 前执行 contiguous 并展为二维，计算后恢复前导维度。LocalDiT 转置后的非连续三维输入曾导致 CFG 分支数值错误。
- AudioVAE 缓存只持有因果卷积 receptive field 和转置卷积 overlap tail；每次生成均重置，不累积解码全历史。
- 模块 profiling 用显式设备同步，独立于吞吐测量。Windows /MD 约定继续适用于 CUDA 与 ORT 共存。

**相关文件**：`crates/voxcpm/`、`crates/novel-tts-backends/examples/voxcpm_candle_probe.rs`、`tools/tts/voxcpm_reference.py`、`dev-notes/voxcpm-candle-acceptance.md`。

### VoxCPM2 Candle 生产接入（2026-10-07）

用户明确接受当前 F32 数值差异先接入；不改容差，不将数值验收记为通过。`voxcpm` worker 现使用本地 Candle crate，沿用 Q8_0/F16 GGUF、模型目录和 feature 名称。`voxcpm-sys` 暂留独立开发基准 crate，适配器和 worker 的依赖及 CUDA/Metal feature 均不含它。发布通知复制 `crates/voxcpm` 的 Apache-2.0 许可证和 SOURCE.md。

**正确做法**：旧音色 WAV/voice.json 保留；`features.json` 不读取。`candle-reference-v1.json` 验证实现、权重清单摘要、模型/revision、WAV 摘要、文字及 F32/F16 精度，不兼容则重建。专用线程只保留一个音色编码，初始化/生成/设计取消后均回收线程。性能校准 revision 带 candle-v1，不能复用 llama.cpp 的校准值。

**相关文件**：`crates/novel-tts-backends/src/voxcpm/`、`crates/novel-tts/src/preparation/synthesis.rs`、`dev-notes/voxcpm-candle-acceptance.md`。



### Windows AMD 核显候选（2026-10-07）

本机是 Ryzen 7 9700X，Windows 枚举到 AMD Radeon(TM) Graphics 和 RTX 5070。WMI 的 AdapterRAM 不是共享 GPU 内存上限，不能据其 512 MiB 值判断模型可用性。Candle 0.9.2 与 0.11.0 的 Device 均只有 CPU/CUDA/Metal；当前 Qwen、VoxCPM、OmniVoice 及 MOSS Candle 后端没有 AMD Windows 计算路径，不将“检测到核显”等同于模型可运行。

候选方案是先验证现有 MOSS Nano ONNX 的 ORT DirectML 路径。rc.13 的 Windows 1.28 分发清单包含 CUDA13+DirectML 联合包，支持复用 ORT；实际 provider 和模型算子覆盖仍须验证。DirectML 需禁用 memory pattern 和 parallel execution；适配器编号来自 DXGI 枚举，不是 CUDA 编号，不写死 0 或 1。显式选择不得静默切换 NVIDIA/CPU；Auto 必须经过真实生成校准，核显不优先于已通过吞吐的 RTX。

后续验收先测逐阶段图分区、CPU fallback、动态 KV 与 int8 算子覆盖、共享内存、首 PCM/RTF、取消和重复生成；只有完整生成有效 PCM 后才新增协议设备及阅读器选项。当前没有发布 DirectML 设备，也没有 AMD 性能验收结果。不要为此维护第二套 Candle 或将 ROCm/Metal 支持误认为 Windows 核显支持。

来源：Candle 0.11.0 `candle-core/src/device.rs`；ORT 官方 https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html；本地 ort-sys rc.13 `build/download/dist.tsv`。

2026-10-07 进一步静态评估：本机 Windows build 28000，AMD 驱动 32.0.21042.62，Ryzen 9700X 官方标注 2 个图形核心。缓存中的 Nano 8 个 ONNX 图均为 opset17，权重为 FLOAT（编码器另有 INT64 常量），无 Quantize/MatMulInteger/MatMulNBits；不能把当前资源描述成 INT8。prefill/decode/codec 含动态序列长度。`moss/runtime.rs::run` 仅 CUDA 分支用 I/O Binding 把 present/cache 留在 GPU，其他设备走普通 Run；新增 DirectML 注册本身不能证明缓存不会逐步拷回 CPU，需要同时验证 DirectML 设备分配、I/O Binding 和图分区。现有专用线程符合 DirectML 同一 session 不并发 Run 的约束。

Windows ML 新路线不是只有 DirectML：官方 MIGraphX 插件要求 RDNA3+、指定最低驱动，当前明确未支持 GenAI 场景；WebGPU 插件仍为实验。DirectML 继续支持，但新功能开发转向 WinML。对当前 Rust/ORT worker，DirectML 是范围最小的功能试验；WinML/MIGraphX 留作新硬件后续评估，不为这个基础核显迁移整个运行时。此次未启用 DirectML 或执行 AMD 推理，尚无性能/算子覆盖结论。

后续原生试验已实现：后端可选 `directml-probe` example 使用 DXGI 显式选择、专用 session 线程、禁用 memory pattern/parallel execution，支持 host outputs 与持久缓存 I/O Binding。DML `MemoryInfo` 的 allocation id 0 是所选 session 内的设备分配标识，不是 DXGI adapter0；实际 adapter1 的缓存返回身份验证通过。显卡按 D3D12 能力筛选，AMD 独显与核显均可参与，不因本机核显结果排除 RX 系列。

本机短语料 release 五轮平均 RTF：CPU .4224、AMD 核显 host 1.2694、device cache 1.1145。真实 provider profiles 显示 MatMul/Conv 在 DML 执行，但仍有 CPU shape/control 与 codec 算术节点；内置音色不触发参考 encoder，不能宣称克隆图覆盖。取消和再次生成通过。CPU/DML 输出时长及 ASR 有差异，人工音质、长播放与 AMD 独显实机未验收，未新增产品设备或 Auto 偏好。资源是 FLOAT opset17，当前不测试 INT8 算子。详见 `dev-notes/moss-directml-evaluation.md`。

来源：AMD 9700X 规格 https://www.amd.com/en/products/processors/desktops/ryzen/9000-series/amd-ryzen-7-9700x.html；Windows ML provider 要求 https://learn.microsoft.com/en-us/windows/ai/new-windows-ml/supported-execution-providers；本地 ONNX 图静态检查。


### Candle 0.11.0 统一升级（2026-10-07）

用户授权在 VoxCPM 移植期间升级最新稳定 Candle。core/nn/transformers 精确统一为 crates.io 0.11.0；上游打包源码 revision 为 31f35b147389700ed2a178ee66a91c3cc25cc80d。CPU 四个计算库检查通过，未需要复制网络或私有 fork。CUDA 内核构建由 bindgen_cuda 改为 cudaforge；Metal 改为 objc2-metal，但仍通过 tts-candle-platform 按目标路由。不删除已有 Metal 排序保护，需 macOS 实机再核验。

校准 runtime_info 标识同步为 candle-0.11.0，涵盖所有 Candle 后端；Vox 参考缓存实现身份也带版本，从原 WAV 重建旧编码，不重下权重或修改音色记录。CUDA/CPU 回归及性能见移植验收报告。crate 未声明 rust-version，Cargo 按工作区 1.89 兼容约束解析依赖；当前 stable 构建不等同于已完成 Rust 1.89 实机验证。

### Windows CUDA 升级后的磁盘与链接诊断

Candle 0.11 all-features 测试第一次链接报告 LNK1318 / LIMIT(12)，当时 D 盘仅剩 29 MiB，失败 PDB 约 61 MiB；不能据错误名称推断触及 4 GiB PDB 格式限制。将 20:20 之前的旧 Rust incremental 缓存移到 C 盘后（保留移动清单，未移动模型/源码/试听材料），原参数 `-j1` 重跑 537 passed / 7 ignored。没有修改 PDB 页面参数或压制 CRT 警告。CUDA 模型开发的多次 debug/profile/feature 编译可积累大量 incremental 缓存；先检查实际磁盘空间，再诊断链接限制。

CUDA 13 官方 `cuda.lib` 的静态 driver loader 对象带 `/DEFAULTLIB:LIBCMT`，当前 release 链接仍有 LNK4098；Candle 0.11 MOE 对象已全部为 `/MD`。不通过全局 `/NODEFAULTLIB`、修改 SDK 或压制警告掩盖冲突。独立 sm120 CUDA 程序及 Vox Q8/BF16 实际播放通过，CRT 风险继续记录。

### ORT CUDA 预编译包与 Blackwell 架构覆盖

本机 rc.13 Runtime 1.28 CUDA13 provider 中只有 sm75/sm80/sm90a cubin，且没有 PTX；RTX 5070 sm120 的 Nano `/Cast` 实际生成报 `cudaErrorNoKernelImageForDevice`。CUDA 13 安装、驱动可用、EP 注册成功都不能证明模型算子覆盖。用 `cuobjdump --list-elf` 和 `--list-ptx` 检查实际打包 DLL，并保存真实生成错误；不静默降级显式 CUDA，不以升级 Toolkit 修复缺失内核。后续验证匹配的含 Blackwell Runtime/provider 分发或可重复原生构建，不能只替换不匹配的 provider DLL。详见 `dev-notes/ort-rc13-upgrade.md`。


### Talechime 独立仓库（2026-10-08）

听书已迁至 https://github.com/yexiyue/talechime，固定版本检出在 `vendor/talechime` Git 子模块。
TRNovel workspace 只保留阅读器和 parse-book-source；协议通过子模块的 talechime-protocol path/version 依赖，Rust 别名 tts_protocol 不变。

**正确做法**：
- 首次克隆/CI 使用 recursive submodules；旧检出运行 git submodule update --init --recursive。
- 听书编译使用 --manifest-path vendor/talechime/Cargo.toml -p talechime，与阅读器分开构建；发行脚本共用 target 输出并继续附带 novel-tts 兼容入口。
- JSON Lines v5、用户配置、模型音色及检查点目录保持不变。新品牌入口为 talechime。
- 模型库、TTS 工具与历史验收资料由 Talechime 拥有；旧知识条目保留为历史记录，其 crates/novel-tts* 路径应映射到 vendor/talechime/crates/talechime*。
- 下次 crates.io 发布 TRNovel 前先发布 talechime-protocol 对应版本；path 依赖在 Cargo 打包后会使用 version。

**不要做**：把 TTS workspace 重新加入阅读器 workspace，或仅因改名迁移 ~/.novel 和 ~/.novel-tts 用户数据。

**相关文件**：Cargo.toml、.gitmodules、.github/scripts/build-variant.py、vendor/talechime/SOURCE.md。

## 2026-10-08 独立发行与数据归属（当前实现）

本节取代上文的旧路径、子模块及双变体发行指导；历史验收记录保留。TRNovel 只分发 trnovel/trn，默认 tts，使用 crates.io talechime-protocol 0.1.0，JSON Lines v5，不要求应用同版本，不递归检出。worker 顺序：CLI --tts-program → config.toml 的 [tts].program → PATH talechime；无效显式路径不回退，缺 worker 不阻断普通阅读。

src/paths.rs 统一 ~/.trnovel 路径：config.toml 的 appearance/reader/browser/tts 分节由 ConfigStore 加锁、重新加载并原子替换；keybindings.toml、toc_rules.json 独立；data 保存书源/历史/登录/browser-profile/local/network，cache 保存字体等可重建资源。clear 只删历史、local/network、cache，不触碰配置、书源或登录态。旧目录不读取、不自动搬迁，手工说明在 guides/migration.mdx。

Talechime 独立拥有 ~/.talechime/config.json、checkpoints、resources，AppPaths 位于其 core crate。两个应用固定 cargo-dist 0.32.0、各一份原生 Cargo dist-workspace.toml；旧 generic dist.toml、basic/加速/ARM64 musl 专用脚本已移除。Talechime 标准包 CPU（Windows/Linux），Mac 添加全部 Metal；CUDA 源码与原生编译 CI 保留。三个平台隔离安装检查不下载模型，不视为真实模型试听通过。

发行验证补充：Talechime GNU 构建须用 Ubuntu 24.04（pinned ORT 1.28 静态库依赖新版 glibc/libstdc++）；min-glibc-version = 2.39。Mac 使用 macos-15 / MACOSX_DEPLOYMENT_TARGET=15.0，匹配 Candle 0.11 的 residency set API。设备探测先枚举 Metal GPU，避免无 GPU 的 hosted VM 在 Candle 的 swap_remove(0) 中 panic/abort。Windows 保持动态 CRT。标准包随附源码、Cargo 依赖与 ONNX Runtime 许可证。AppPaths 尊重非空 HOME，Windows 否则使用系统 home；隔离 smoke 不修改真实用户数据。

隔离 smoke 的注意事项：FileSelect 初始没有选中项，需先 J 再 Enter 打开小说；macOS PTY 在退出时也必须持续读取终端输出，避免缓冲区阻塞最终渲染。Windows 捕获中文帮助时显式使用 UTF-8；冷启动 Cargo 元数据解析允许 600 秒，程序启动和握手仍使用短超时。Unix transport 测试在临时目录编译一次不可变 worker，通过同文件系统的硬链接创建不同名称，避免并行复制可执行文件与 fork/exec 竞争产生 ETXTBSY。

