# 工具链 / 工程

## 概览

Cargo workspace 的模块组织、feature 门控、构建/发布、平台坑。`Cargo.toml`(根)、`crates/*/Cargo.toml`、`lefthook.yaml`、`.github/workflows`、`release.sh`。

## 模块组织

### 统一用 mod.rs 风格

全 workspace（含主程序 `src/` 与子 crate）统一用 **`foo/mod.rs` 目录风格**,不用 `foo.rs` + `foo/` 并列风格。重构 `parse-book-source` 时把扁平的 16 个文件按功能域收进目录:`source/`、`eval/`、`fetch/`(含 `browser/`)、`host/`、`engine/`,每个目录一个 `mod.rs`。

**保持外部路径稳定**：`lib.rs` 用 re-export 把内部新路径映射回旧的对外路径,例如 `pub use fetch::cookie;`、`pub use host::state;`——外部 crate（主程序）的 `use parse_book_source::cookie::...` 不受目录重构影响。

**相关文件**：`crates/parse-book-source/src/lib.rs`、各域 `mod.rs`

### include_str! 路径随文件深度变

把 `source.rs` 移到 `source/mod.rs` 后多了一层目录,`include_str!("../book-source.schema.json")` 要改成 `../../`。移动含 `include_str!`/`include_bytes!` 的文件时记得同步相对路径。

**相关文件**：`crates/parse-book-source/src/source/mod.rs`(schema_sync 测试)

## 依赖钉版（勿随意升级）

### ort / kokoro-tts 钉死，勿升

- `ort` 钉死 `2.0.0-rc.10`（onnxruntime 绑定）。
- `kokoro-tts` 钉死 `0.3.1`——**`rc.12` 砍掉了 Intel Mac 支持,不要升**。
- 普通发布目标使用 ort 预编译库；ARM64 musl 使用 Alpine 系统共享库，详见下方 musl 发布说明。

**相关文件**：`crates/novel-tts/Cargo.toml`、根 `Cargo.toml`

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

### `ort` 是「假死依赖」——版本钉死,代码零引用但不可删

`crates/novel-tts/Cargo.toml` 的 `ort = "=2.0.0-rc.10"` 在 novel-tts src 里**零 `use`/`ort::` 引用**——它不是直接用的,而是**钉死 kokoro-tts 传递依赖 ort 的版本**（kokoro-tts 声明宽松预发布范围,rc.12 砍 Intel Mac/要 glibc 2.38+）。`cargo-machete` 等死依赖工具会把它误报为 unused 并建议删除,**删了会让 ort 解析到坏版本、发布炸**。审计死依赖时这类「纯版本钉死 dep」要人工豁免。

**相关文件**：`crates/novel-tts/Cargo.toml`（注释已说明）

### ARM64 musl release artifact

`local-artifacts-jobs = ["./musl"]` extends cargo-dist without hand-editing the generated workflow. The reusable musl workflow builds both application binaries natively in an ARM64 Alpine 3.23 Rust container and uploads `artifacts-build-musl`; the generated host job includes these files in the GitHub Release. It runs on PRs separately because the main dist workflow normally only plans PR releases.

The pinned ort rc.10 requires ONNX Runtime API 1.22. Its GNU prebuilt libraries are incompatible with musl, so the musl build uses Alpine's system ONNX Runtime with `ORT_LIB_LOCATION`, `ORT_PREFER_DYNAMIC_LINK=1`, `ORT_SKIP_DOWNLOAD=1`, and `-C target-feature=-crt-static`. This preserves TTS but requires runtime shared libraries. Do not add this target to dist's ordinary matrix until its build and installer dependency handling supports this setup. The custom archive is currently a manual download rather than an installer-selected platform.

**相关文件**：`.github/workflows/musl.yml`、`.github/scripts/build-musl.sh`、`.github/scripts/smoke-musl.sh`、`Cargo.toml`。

### npm Trusted Publishing

npm publishes through the reusable `publish-npm.yml` workflow with Node 24, npm 11 and `id-token: write`; no `NPM_TOKEN` is passed. Configure two npm GitHub trusted publishers for `yexiyue/TRNovel`: `trnovel-release.yml` (the caller identity used for normal releases) and `publish-npm.yml` (manual recovery from existing release assets). Enable direct `npm publish`; dist-tag management is unnecessary. The manual workflow only publishes on main and checks the package name, repository and version against the requested tag. PRs validate the existing release archive with a dry run, without publishing.

Change cargo-dist metadata and regenerate `trnovel-release.yml`; do not edit the generated file directly. Rerunning an old failed release uses its original workflow, so recover with `Publish npm with OIDC` workflow_dispatch on main instead.

**相关文件**：`Cargo.toml`、`.github/workflows/publish-npm.yml`、`.github/scripts/publish-npm.sh`。

npm publish-time scanning can delay registry availability by several minutes after a successful upload. Poll the public version before declaring success; do not re-upload while scanning is pending. Validate existing archives with `npm pack --dry-run`, because `npm publish --dry-run` still rejects already-published versions before a retry can skip them.
