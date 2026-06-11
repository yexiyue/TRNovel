---
name: dev-workflow
description: |
  项目开发工作流技能。在以下场景自动调用：
  (1) 编写或修改任何 src/ 或 crates/*/src/ 下的代码
  (2) 添加新依赖或修改配置文件（Cargo.toml / feature / lefthook / CI）
  (3) 完成一个 feature 或修复一个 bug
  触发关键词：组件开发、bug 修复、重构、新功能、依赖升级、配置变更、书源、TUI、ratatui-kit、TTS
---

# Dev Workflow — TRNovel 项目开发工作流

## 工作流程

### 1. 开发前：加载相关知识

根据当前任务，读取 `dev-notes/knowledge/` 下的相关主题文件：

| 主题文件 | 何时读 |
|---|---|
| `toolchain.md` | 改 Cargo workspace / feature 门控（browser/js/js-host/schema）/ lefthook / CI / 发布（cargo-release/cargo-dist）/ 模块组织（mod.rs 风格） |
| `tui-ratatui-kit.md` | 写/改主程序 UI（`src/pages`、`src/components`、`src/hooks`）—— ratatui-kit 的 hooks/组件/路由/键位约定与坑 |
| `booksource.md` | 改 `crates/parse-book-source`（书源规则 DSL / 反爬 / render-fetcher / 签名 / 番茄）或 `crates/novel-tts` |

**读取方式**：用 Read 工具读对应文件，遵循其中的最佳实践与坑。不确定读哪个时，先 `ls dev-notes/knowledge/` 按文件名判断。

### 2. 开发中：遵循最佳实践

同时参考以下通用 skill（与当前任务相关时自动调用）：

- `/rust-best-practices` — Rust 通用规范（所有权、错误处理、惯用法）
- `/rust-async-patterns` — Tokio、异步 trait、并发/取消、Send 约束

**优先级**：项目知识库 > 通用 skill > Claude 自身知识。项目知识库有明确记录时，以它为准。

### 3. 开发后：更新知识库

完成代码修改后，**检查是否产生了新的项目知识**：

**需要记录**：新依赖的正确用法、配置坑与 workaround、架构决策及原因、与通用做法不同的项目特定约定、不明显 bug 的根因。

**不需要记录**：代码本身能表达的、通用编程知识、临时调试信息、git 能查到的。

**更新方式**：判断属于哪个主题文件 → 追加条目到合适分类下 → 现有主题都不合适才新建 → 已过时的条目更新或删除。

**条目格式**：

```markdown
### 条目标题

简短描述做了什么、为什么这样做。

**正确做法**：
- 具体的代码模式或配置

**不要做**（如果有）：
- 错误的做法及原因

**相关文件**：`path/to/file`
```

### 4. 代码质量检查

开发完成后运行 `/simplify` 检查代码质量。lint/format/typecheck/doc 命令（与 CI/lefthook 一致）：

```bash
cargo test --locked --all-features --workspace --lib --tests --examples
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo fmt --all --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples
# 改了会被 tokio::spawn 调用的引擎公开 API（如 Engine::explore/search）后，必须额外：
cargo build   # 验证主程序 trnovel 的 Future 仍 Send（子 crate 的 lib test 测不出 spawn 上下文）
```
