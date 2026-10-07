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

- `ort` 钉死 `2.0.0-rc.10`（onnxruntime 绑定）。
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

`ort` 的版本统一固定在根 workspace.dependencies。MOSS Nano 与对齐器仍使用 ORT，不要删除或放宽固定版本；rc.10 保留当前 Intel Mac 和 GNU Linux 发布兼容性。

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

`novel-tts-core` 不包含模型 feature；独立程序默认 moss feature，固定原生推理依赖由 novel-tts-backends 承担。核心拥有通用 rodio 播放器。此变更的发布安装渠道与跨平台试听仍由 OpenSpec 的未完成任务跟踪，不能把本机 check 或假进程测试写成平台发布验收。

**相关文件**：`Cargo.toml`、`crates/novel-tts/Cargo.toml`、`openspec/changes/decouple-tts-process/tasks.md`。


### 双变体 cargo-dist 发布

固定 cargo-dist 0.32.0 配置位于 `dist-workspace.toml`，`dist.toml` 描述泛用构建：默认听书包保留 `trnovel-v*` 标签及旧安装器名称，包含三个二进制。基础版由 custom local job 生成，仅包含两个阅读器；global extra-artifacts 校验基础压缩包并生成独立 shell/PowerShell/npm/Homebrew 安装器。

`release.sh` 在升级版本后执行 `sync-dist-version.py` 保持 generic manifest 同步。`trnovel-basic.formula` 由自定义发布 job 改名为 `.rb`，避免 cargo-dist 默认 Homebrew job 把多个公式当成一个文件。新 npm 包首次发布前必须在注册表配置相应 Trusted Publisher，现有包的授权不自动覆盖新包。

构建程序分别调用 package-specific Cargo 命令，防止 workspace feature unification 给阅读器带入原生音频依赖。ARM64 musl 先在无音频包的干净容器运行基础版，再装 ALSA/ONNX Runtime 检查听书程序。当前本机 Docker daemon 未启动，Windows/Intel Mac/GNU Linux 与 musl 实机验收仍依赖对应环境，不能以本机构建代替。

### 听书包与模块命名

`novel-tts-core` 是会话/合成/播放库，`novel-tts-protocol` 是轻量协议库，`novel-tts` 是独立程序 crate 及命令。依赖键用 `tts-core` / `tts-protocol` 显式声明 package 名，Rust 用 `tts_core` / `tts_protocol` 引用。阅读器可选模块为 `src/tts.rs` 与 `src/tts/`。

CLI 接管原 novel-tts 的包名，保持 0.3.0 版本线，后续发布需递增；核心新包同样暂用 0.3.0。模型公共根目录 `.novel-tts` 与配置/检查点路径保持原样。更新包名时同步 crates.io 标签到目录的发布路由、cargo-dist binary 清单与同目录/PATH 程序发现。

### MOSS 后端与验收隔离

新模型实现在 novel-tts-backends；共享原生依赖版本位于 workspace.dependencies。novel-tts 默认 moss，CPU 默认仅 MOSS Nano；Kokoro 和 ZipVoice 已移除。阅读器保持协议依赖隔离。

MOSS ONNX opset 17 的实际加载、生成和编解码已在本机固定 ort rc.10 上验证，不需升级原生运行时。SentencePiece 使用纯 Rust sentencepiece-rs，参考 WAV 用 hound 解码和 rubato sinc 重采样，无 Python 运行依赖。资源固定 revision、尺寸和 SHA-256；音色缓存绑定模型版本。

VHS 验收不同 feature 的阅读器时，把构建出的 basic 二进制复制到独立目录后再录制。随后运行 workspace all-features 测试会重建 target/debug/trn；若继续录制这个共享路径，会误把完整听书版当成基础版。

**相关文件**：`crates/novel-tts-backends/README.md`、`dev-notes/moss-tts-acceptance.md`。

### ORT 加速 feature 与模型校验

保留 ort=2.0.0-rc.10。coreml/ort-cuda 仅由听书后端/程序 feature 启用，阅读器始终不链接它们。ORT 自带下载清单选择 CUDA12 的原生分发，Mac 可静态链接 CoreML 框架；跨平台原生运行与 CUDA/cuDNN 依赖必须在对应平台验收。本机 Mac 启用 ort-cuda feature 会下载 CPU 原生包，因此 cargo check 不能证明 CUDA 可用。

开发构建将 sha2 单包 opt-level=3，避免每次准备模型时对 GB 级权重执行慢速 debug 校验；保留每次大小与 SHA-256 校验。性能校准应使用 release 构建。多包 cargo build 配合 --bin trn 只构建名为 trn 的程序；更新 worker 必须单独 cargo build -p novel-tts，不能依据阅读器构建完成判断 worker 已更新。

### 轻量 TOC 常量共享

内置章节数字、中文/英文/特殊标题正则常量在 novel-tts-protocol::headings 共享，protocol 不编译正则、不处理书籍。基础阅读版因此也依赖该轻量 crate，但仍不依赖 TTS core/backends/ORT/rodio。模型诊断 example 明确使用输出目录，真实模型测试通过 TRNOVEL_MOSS_MODEL_DIR 启用。

### 本地 Candle Qwen 与平台设备

Qwen 推理源码位于 `crates/qwen3-tts`，来自 TrevorS/qwen3-tts-rs revision `711ceee07cad92673f86de8997bdf54c30caa49f`（MIT）。仅纳入推理库，移除上游 CLI、Hub 下载、Flash Attention 和自定义 PTX；下载/校验、校准、会话与播放仍由既有层管理。Candle core/nn/transformers 统一钉为 0.9.2。

**正确做法**：
- `qwen` 是 CPU 后端；`qwen-cuda` 同时启用 Qwen 和三套 Candle CUDA 算子；`metal` 启用 Candle core/nn 的 Metal 算子，仅在 macOS 构建。`ort-cuda` 独立用于 MOSS/对齐器，不保留旧 `cuda` 别名。
- GPU feature 经 `tts-candle-platform` 按 target 路由，同一 Candle 0.9.2 同时服务 Qwen 和 Omni。Windows/Linux 的 CUDA 依赖和 macOS Metal 依赖分别生效；Windows all-features 不会编入 Objective-C。CUDA 构建仍需 Toolkit，通用 CI/lefthook 使用 CPU feature 集合，平台 GPU CI 单独启用。
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

### Windows CUDA Toolkit 本地安装

本机 CUDA 12.9 Update 1 从 NVIDIA 官方 redistrib Windows ZIP 组件安装到 `D:\dev-tools\cuda\v12.9`，逐包按官方 manifest 校验 SHA-256，保留现有显示驱动。包含 nvcc、运行时、数学库、头文件、命令行工具与示例；不安装 Nsight GUI 或 Visual Studio 项目集成。系统环境的 `CUDA_PATH` / `CUDA_PATH_V12_9` 指向该目录，系统 PATH 添加 CUDA `bin` 与 VS 2022 的 x64 MSVC 编译器目录。

**正确做法**：
- 重开终端或父进程以加载系统环境，CMD/PowerShell 均不需要激活脚本。Rust 自行发现 MSVC 的 linker 不代表 nvcc 可以找到 cl.exe；后者需要编译器目录在 PATH 中，随后 nvcc 自行加载 VS 编译环境。
- 系统 `NVCC_PREPEND_FLAGS=-Xcompiler=/MD` 使 CUDA host C++ 与 Rust/ORT 使用动态 CRT。Candle 0.9.2 的 MOE 静态库默认按 `/MT` 编译，与默认 MOSS 所需的 ORT `/MD` 组合时发生 LNK2038；配置后先 `cargo clean -p candle-kernels` 重建旧对象。此选项影响使用系统环境的所有 nvcc 调用，不使用 `/NODEFAULTLIB` 掩盖冲突。
- 初次编译使用 `$env:RAYON_NUM_THREADS = '2'` 和 `cargo build --locked -j 2 -p novel-tts --no-default-features --features qwen-cuda`；Cargo 并行数不能约束 bindgen_cuda 内部的 Rayon 内核编译线程数。
- nvcc 12.9.86 与本机 MSVC 14.44 配合完成 `sm_120` 内核编译和 RTX 5070 实际执行；官方 deviceQuery / vectorAdd 也通过。该结果证明 Toolkit/驱动工作，不能代替真实 Qwen PCM/EOS 与听感验收。

安装清单和哈希：`D:\dev-tools\cuda\v12.9\installation-manifest.json`；本机使用说明：`D:\dev-tools\cuda\README.txt`。

### 多模型原生 TTS 与 CMake

新增 `crates/voxcpm-sys` 固定 llama.cpp-omni 静态 C ABI，`crates/omnivoice` 只保留标准 Candle 推理。native 依赖只经 worker 引入；阅读器仅使用 protocol。CMake/cc 置于 workspace dependencies。Windows 使用 Ninja 与 cc::windows_registry 得到的 MSVC 环境，无需 CUDA Visual Studio 插件；VS generator 在 ZIP Toolkit 安装中会报 No CUDA toolset。本机 CMake 3.31.6 / Ninja 1.11.1.4 位于 D:/dev-tools/bin。

Omni tokenizer 权重为 BOSON/Higgs/Llama 条款，不能跟生成器一起标为 Apache；`crates/omnivoice/LICENSE.Higgs-Audio` 保存原始许可。

**相关文件**：`crates/tts-candle-platform/`、`crates/voxcpm-sys/build.rs`、`.github/workflows/qwen-cuda.yml`。

### MOSS 统一 Candle 试用

`crates/moss-tts` 与 Qwen/Omni 共用 Candle 0.9.2。worker 使用 `moss-candle-cuda` / `moss-candle-metal`，不把推理依赖引入阅读器。CUDA 大模型 BF16、codec F16；VoiceGenerator F16 的真实权重会产生无效 logits。四组权重按官方固定 revision 和 SHA 放用户缓存，7.1 GB codec 共用一次，不按每个生成模型复制。打包可选 feature 时附带 moss-tts LICENSE/NOTICE。CPU 库检查不等于完整大模型 CPU 验收；当前 worker 新模式仅公开 GPU。

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

2026-10-07 移除 Kokoro、ZipVoice adapters、features、专用依赖和 ZipVoice 的 vendored eSpeak/CMake helper。保留 MOSS Nano 为 CPU 默认，现有 GPU 模型继续保留。ORT rc.10 仍由 Nano 与强制对齐使用；不要随旧后端一起删除。worker 启动时将退休后端及无 backend 的旧配置迁移为 Nano 默认音色、CPU；文件锁内校验并原子保存，保留其他偏好与未知字段，revision 只增加一次。用户缓存不删除。

**相关文件**：`crates/novel-tts-core/src/config.rs`、`crates/novel-tts/src/main.rs`、`crates/novel-tts-backends/src/lib.rs`。
