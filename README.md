<div align="center">
  <a href="https://yexiyue.github.io/TRNovel">
    <img src="docs/public/trnovel-wordmark.svg" alt="TRNovel" width="420">
  </a>

  <p><strong>在终端里，优雅地读小说</strong></p>

  <p>
    <a href="https://yexiyue.github.io/TRNovel"><img src="https://img.shields.io/badge/官网-yexiyue.github.io%2FTRNovel-3DDC97" alt="官网"></a>
    <a href="https://www.npmjs.com/package/@trnovel/trnovel"><img src="https://img.shields.io/npm/v/@trnovel/trnovel" alt="NPM Version"></a>
    <img src="https://img.shields.io/npm/d18m/%40trnovel%2Ftrnovel?label=npm%20downloads" alt="NPM Downloads">
    <a href="https://crates.io/crates/trnovel"><img src="https://img.shields.io/crates/v/trnovel" alt="Crates.io Version"></a>
    <img src="https://img.shields.io/crates/d/trnovel?label=crates.io%20downloads" alt="Crates.io Downloads">
  </p>

  <p>
    <a href="https://yexiyue.github.io/TRNovel/guides/intro">使用指南</a>
    ·
    <a href="https://yexiyue.github.io/TRNovel/book-source/intro">书源参考</a>
    ·
    <a href="https://yexiyue.github.io/TRNovel/reference/cli">CLI 参考</a>
    ·
    <a href="https://yexiyue.github.io/TRNovel/reference/anti-scraping">反爬与浏览器辅助</a>
  </p>

  <img src="docs/src/assets/guides/read.gif" alt="TRNovel 阅读演示" width="860">
</div>

## TRNovel 是什么

TRNovel (Terminal Reader for Novel) 是一个 Rust 构建的终端小说阅读器。本地 TXT 与网络书源双修，一个二进制开箱即用。

它和其他阅读器最大的不同：接入新站点不靠手写规则，而是把「逆向、校验、导入」交给 AI 串成闭环。给 Agent 一个小说站 URL，它生成书源，`trn doctor` 体检到全绿，导入后立刻能读。

## 核心能力

| 能力 | 说明 |
| --- | --- |
| 本地阅读 | 自动识别 UTF-8 / GBK，智能切分「卷、章」目录，支持 `~/.novel/toc_rules.json` 自定义规则 |
| 网络书源 | 搜索、分类浏览、详情、目录、正文全链路；取值后端 CSS / XPath / JSONPath / 正则任选 |
| AI 生成书源 | `booksource-generator` skill 自动探站逆向，配合 `trn doctor` 校验到全绿 |
| 加密与签名 | `clean` 流水线内置 AES/DES/3DES、Base64/Hex/URL、MD5/SHA/HMAC、繁简转换等确定性算子，少数动态站点另有 JS 逃生舱 |
| 反爬辅助 | Cloudflare 等挑战页复用系统浏览器解挑战，cookie 回填后继续走快速请求 |
| TTS 听书 | 内置 Kokoro 中文语音合成，播放进度与正文高亮同步 |
| 阅读体验 | 历史记录、断点续读、命名主题与背景模式；Windows / macOS / Linux 单二进制 |

## 安装

macOS / Linux：

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/yexiyue/TRNovel/releases/latest/download/trnovel-installer.sh | sh
```

Windows：

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/yexiyue/TRNovel/releases/latest/download/trnovel-installer.ps1 | iex"
```

或者使用包管理器：

```bash
cargo install trnovel
npm i -g @trnovel/trnovel
brew install yexiyue/tap/trnovel
```

## 使用

```bash
trn              # 打开主界面
trn -l ./novels  # 从本地小说目录进入
trn -n           # 进入网络小说模式
trn -q           # 继续上次阅读
trn -H           # 查看历史记录
```

书源相关：

```bash
trn doctor ./my-source.v2.json   # 全流程体检书源
trn import ./my-source.v2.json   # 导入书源
```

完整命令与快捷键见 [使用文档](https://yexiyue.github.io/TRNovel)。

## 让 AI 帮你做书源

把 [`booksource-generator`](./skills/booksource-generator/SKILL.md) 装进你的 Agent：

```bash
npx skills add https://github.com/yexiyue/TRNovel/tree/main/skills/booksource-generator
```

然后向 Agent 提需求：

```text
请使用 booksource-generator skill，为 https://example.com 生成一个 TRNovel v2 书源。
生成后运行 trn doctor 校验，修到通过后把 JSON 给我。
```

skill 会探测站点、逆向出搜索、目录、正文的选择器，处理字符集与反爬，并用 `trn doctor` 按 ✓/✗ 迭代到全部通过。深入资料：

- [v2 书源 JSON Schema](./skills/booksource-generator/references/book-source.schema.json)
- [真实书源示例](./skills/booksource-generator/references/example-bilixs.v2.json)
- [探站与逆向参考](./skills/booksource-generator/references/reverse-engineering.md)

## 书源格式

TRNovel 使用自有的 `trnovel-booksource/v2` 格式：结构化 JSON，不依赖紧凑字符串 DSL，schema 由 Rust 类型自动生成，对人类维护和 AI 生成都友好。`search` / `explore` / `bookInfo` / `toc` / `content` / `http` / `samples` 各司其职，规则细节见 [规则语法](https://yexiyue.github.io/TRNovel/book-source/rules)。

v2 暂不兼容 [Legado](https://github.com/gedoor/legado) 书源，后续会提供迁移转换的 skill。

## 开发

```bash
cargo run                                       # 运行
cargo test                                      # 测试
cargo run -p parse-book-source --example gen_schema   # 生成书源 schema
```

## 声明

1. 请支持正版。TRNovel 不提供、制作、上传或存储任何小说内容，网络资源均由用户自行配置书源后访问。
2. 本项目仅供个人学习、研究和技术交流使用，禁止用于任何违法或非法商业用途。
3. 用户需自行确认所访问内容和书源的合法性、准确性、完整性与可用性，并自行承担使用结果。
4. TRNovel 会将缓存、历史、外观设置等数据存储在本地 `.novel` 目录，不会上传至任何服务器。
5. TRNovel 不会收集用户个人信息，也不会对用户阅读行为进行监控。

## License

[MIT](./LICENSE)
