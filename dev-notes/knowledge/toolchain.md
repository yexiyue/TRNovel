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
- onnxruntime 等 C 依赖本就**静态链入** trnovel，"零 C 依赖"从来不是本项目约束。

**相关文件**：`crates/novel-tts/Cargo.toml`、根 `Cargo.toml`

## 构建 / 发布

### 平台坑

- **Edition 2024**：需 Rust ≥ 1.85（stable，无需 nightly）。
- **Linux 构建依赖**：`libasound2-dev`（rodio）、`libssl-dev`、`pkg-config`，CI 用 apt 装。
- **Windows**：根 `Cargo.toml` 的 `[workspace.metadata.dist]` 里 `msvc-crt-static = false` **必须保留**——ort/onnxruntime 是动态 CRT，静态链接会 `__imp_tolower` 等 unresolved-symbol LNK 错。

### pre-commit / 发布链

`lefthook.yaml` pre-commit：test → `clippy --fix --allow-dirty`（自动 stage 修复）→ `cargo fmt` → `cargo doc`。发布走 `./release.sh`（cargo-release + git-cliff，tag `<crate>-v<version>`）+ cargo-dist（`trnovel-v*` tag 触发）。

### 改引擎公开 API 后要单独 cargo build 验 Send

子 crate 的 lib test **测不出** `tokio::spawn` 上下文的 `Send` 约束。改了会被主程序 spawn 调用的引擎公开 API（如 `Engine::explore`/`search`）后，CI 四件套之外还要 `cargo build` 主程序 trnovel——曾因 `explore`/`search` 的 Future 变 `!Send`（async closure 参数）导致主程序编译失败、`cargo run` 跑不起来。

**相关文件**：`crates/parse-book-source/src/engine/mod.rs`

<!-- 随开发补充:新 feature 门控约定、CI matrix 变更等 -->
