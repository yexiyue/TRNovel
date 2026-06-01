# TRNovel

> **TRNovel (Terminal Reader for Novel)** 是一个 Rust + Ratatui 构建的终端小说阅读器，支持本地 TXT、网络小说书源、阅读历史、主题与听书。它不只负责阅读，也提供面向 AI Agent 的书源生成 skill，可以从小说网站快速生成并校验可用书源。

[![NPM Version](https://img.shields.io/npm/v/@trnovel/trnovel)](https://www.npmjs.com/package/@trnovel/trnovel)
![NPM Downloads](https://img.shields.io/npm/d18m/%40trnovel%2Ftrnovel?label=npm%20downloads)
[![Crates.io Version](https://img.shields.io/crates/v/trnovel)](https://crates.io/crates/trnovel)
![Crates.io Total Downloads](https://img.shields.io/crates/d/trnovel?label=crates.io%20downloads)

## 为什么是 TRNovel

TRNovel 适合希望在终端里安静阅读、管理本地小说和自定义网络书源的用户。它的重点不是内置资源，而是把阅读器、书源规则、诊断工具和 AI 书源生成工作流串起来，让新的站点接入更可控。

- **本地 TXT 阅读**：自动识别章、节、回、番外、楔子等章节结构，支持分卷目录与 `~/.novel/toc_rules.json` 自定义规则。
- **网络小说书源**：通过自定义书源接入站点，支持搜索、分类浏览、书籍详情、目录和正文抽取。
- **通过 skill 创建书源**：安装 `booksource-generator` skill 后，给 AI Agent 一个小说站 URL，就能生成 `trnovel-booksource/v2` JSON，并用 `trn doctor` 校验可用性。
- **书源体检**：`trn doctor <书源.json>` 会检查配置、浏览、书详情、目录、正文和搜索，让书源不是“写出来就算”，而是可验证。
- **反爬辅助**：对 Cloudflare 等挑战页，支持系统浏览器解挑战后回填 cookie，普通阅读链路仍尽量走快速请求。
- **阅读体验**：保存阅读历史，支持继续上次进度、主题颜色配置、Kokoro 文本转语音听书。

## 快速安装

推荐使用自动安装脚本：

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/yexiyue/TRNovel/releases/latest/download/trnovel-installer.sh | sh
```

Windows PowerShell：

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/yexiyue/TRNovel/releases/latest/download/trnovel-installer.ps1 | iex"
```

也可以通过包管理器安装：

```bash
cargo install trnovel
npm i -g @trnovel/trnovel
pnpm add -g @trnovel/trnovel
brew install yexiyue/tap/trnovel
```

安装后检查：

```bash
trnovel --help
```

## 基本使用

```bash
trnovel              # 打开主界面
trnovel -l ./novels  # 从本地小说目录进入
trnovel -n           # 进入网络小说模式
trnovel -q           # 继续上次阅读
trnovel -H           # 查看历史记录
trnovel -c           # 清空历史记录和小说缓存
```

书源诊断命令：

```bash
trn doctor ./my-source.v2.json
```

完整文档见 [TRNovel 使用文档](https://yexiyue.github.io/TRNovel)。

## 用 skill 创建书源

先把 [`booksource-generator`](./skills/booksource-generator/SKILL.md) 安装到你的 Agent，再让 Agent 根据目标小说站生成书源。

### 安装 skill

```bash
npx skills add https://github.com/yexiyue/TRNovel/tree/main/skills/booksource-generator
```

skill 位置：[`skills/booksource-generator`](./skills/booksource-generator/SKILL.md)。安装后请重启 Agent 会话，或按你的 Agent 文档重新加载 skills。

### 使用方式

向 Agent 提这样的需求：

```text
请使用 booksource-generator skill，
为 https://example.com 生成一个 TRNovel v2 书源。
生成后运行 trn doctor 校验，修到通过后把 JSON 给我。
```

常用检查命令：

```bash
trn doctor ./example.v2.json
```

参考文件：

- [booksource-generator skill](./skills/booksource-generator/SKILL.md)
- [v2 书源 JSON Schema](./skills/booksource-generator/references/book-source.schema.json)
- [真实书源示例](./skills/booksource-generator/references/example-bilixs.v2.json)
- [探站与逆向参考](./skills/booksource-generator/references/reverse-engineering.md)
- [skill 评测记录](./skill-evals/booksource-generator/iteration-1/benchmark.md)

## 书源格式

TRNovel 当前推荐 `trnovel-booksource/v2`。它是结构化 JSON，不依赖紧凑字符串 DSL，更适合人类维护和 AI 生成。v2 书源可以声明：

- `search`：搜索请求、结果列表和书籍字段抽取。
- `explore`：分类浏览入口和分页列表。
- `bookInfo`：书名、作者、封面、简介、状态、目录地址等信息。
- `toc`：章节列表、分卷识别和章节 URL。
- `content`：正文选择器、分页正文和清理规则。
- `http`：字符集、请求头、超时、重试、浏览器辅助等网络策略。
- `samples`：用于 `trn doctor` 校验的真实样例。

TRNovel v2 书源目前不兼容 [Legado](https://github.com/gedoor/legado) 书源格式。后续会提供一个转换 skill，用来把 Legado 书源迁移为 TRNovel 的 `trnovel-booksource/v2` 格式。

## 开发

```bash
cargo run
cargo run -- doctor ./my-source.v2.json
cargo test
```

生成书源 schema：

```bash
cargo run -p parse-book-source --example gen_schema
```

## 声明

1. 请支持正版。TRNovel 不提供、制作、上传或存储任何小说内容，网络资源均由用户自行配置书源后访问。
2. 本项目仅供个人学习、研究和技术交流使用，禁止用于任何违法或非法商业用途。
3. 用户需自行确认所访问内容和书源的合法性、准确性、完整性与可用性，并自行承担使用结果。
4. TRNovel 会将缓存、历史、主题等数据存储在本地 `.novel` 目录，不会上传至任何服务器。
5. TRNovel 不会收集用户个人信息，也不会对用户阅读行为进行监控。

## License

[MIT](./LICENSE)
