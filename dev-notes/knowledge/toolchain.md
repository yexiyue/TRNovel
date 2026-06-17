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

- **Edition 2024 + std 文件锁**：需 Rust ≥ 1.89（stable，无需 nightly）。`parse-book-source` 的 browser-pool 用 `std::fs::File::try_lock` 做跨进程启动临界区锁；该 API 稳定于 1.89。
- **Linux 构建依赖**：`libasound2-dev`（rodio）、`libssl-dev`、`pkg-config`，CI 用 apt 装。
- **Windows**：根 `Cargo.toml` 的 `[workspace.metadata.dist]` 里 `msvc-crt-static = false` **必须保留**——ort/onnxruntime 是动态 CRT，静态链接会 `__imp_tolower` 等 unresolved-symbol LNK 错。

### pre-commit / 发布链

`lefthook.yaml` pre-commit：test → `clippy --fix --allow-dirty`（自动 stage 修复）→ `cargo fmt` → `cargo doc`。发布走 `./release.sh`（cargo-release + git-cliff，tag `<crate>-v<version>`）+ cargo-dist（`trnovel-v*` tag 触发）。

### 改引擎公开 API 后要单独 cargo build 验 Send

子 crate 的 lib test **测不出** `tokio::spawn` 上下文的 `Send` 约束。改了会被主程序 spawn 调用的引擎公开 API（如 `Engine::explore`/`search`）后，CI 四件套之外还要 `cargo build` 主程序 trnovel——曾因 `explore`/`search` 的 Future 变 `!Send`（async closure 参数）导致主程序编译失败、`cargo run` 跑不起来。

**相关文件**：`crates/parse-book-source/src/engine/mod.rs`

### `ort` 是「假死依赖」——版本钉死,代码零引用但不可删

`crates/novel-tts/Cargo.toml` 的 `ort = "=2.0.0-rc.10"` 在 novel-tts src 里**零 `use`/`ort::` 引用**——它不是直接用的,而是**钉死 kokoro-tts 传递依赖 ort 的版本**（kokoro-tts 声明宽松预发布范围,rc.12 砍 Intel Mac/要 glibc 2.38+）。`cargo-machete` 等死依赖工具会把它误报为 unused 并建议删除,**删了会让 ort 解析到坏版本、发布炸**。审计死依赖时这类「纯版本钉死 dep」要人工豁免。

**相关文件**：`crates/novel-tts/Cargo.toml`（注释已说明）

### 死依赖清理（change upgrade-ratatui-kit-07 随手）

手动审计（grep 每个直接依赖的下划线名在对应 src）清掉:trnovel 的 `tui-scrollview`（框架自带 ScrollView,项目从未直接 use）、`rodio`/`tokio-util`（仅 novel-tts 用,trnovel 自己声明的是死的）;novel-tts 的 `futures`（零引用）。`tui-scrollview`/`tui-big-text` 同时升到最新（0.6.7/0.8.7);**`tui-big-text` 0.8.7 补了 `&BigText: Widget`**,免去为 `widget(big_txt)` 自写 WidgetRef 适配层。

### ratatui-kit 0.7.1 的两处自修(已发版、已切回 crates.io)

迁移期在框架仓修了两个 0.7 的坑并**已发布到 crates.io**(`ratatui-kit-macros` 0.6.1 + `ratatui-kit` 0.7.1,均带 trybuild 测试 `tests/ui/pass/{macro_call_child,widget_by_value}.rs`);TRNovel 依赖已从临时 path 切回 `ratatui-kit = { version = "0.7.1", features = ["full"] }`:
1. **element! 子节点位接纳宏调用**(`ratatui-kit-macros/src/element.rs::parse_children` 加 `is_macro_call`):`element!(...)`/`vec![...]` 在 children 块 / 一等控制流分支体内当 `{expr}` embed 解析,原误报 `expected identifier`(`Comp(..){ element!(..) }` 把 `{}` 当 children 时最坑)。
2. **widget() 收按值 widget**(`adapter/widget.rs` 约束 `for<'a> &'a T: Widget` → `T: Widget + Clone` 按值克隆渲染;`text.rs` 的 `TextParagraph` 同步改按值):`BigText` 等只实现按值 `Widget` 的部件可直接 `widget(...)`。**取舍**:只实现按引用 `impl Widget for &T` 的部件(如 ratatui-widgets 的 `Shadow`)在 0.7.1 起不再被 `widget()` 接纳。

<!-- 随开发补充:新 feature 门控约定、CI matrix 变更等 -->
