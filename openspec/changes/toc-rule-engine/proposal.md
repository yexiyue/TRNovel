## Why

本地 TXT 的章节识别目前只有一条硬编码正则 `第.+章`（`src/novel/local_novel.rs:113`，逐行 `is_match`、无行首锚定、无长度上限、计数词写死「章」）。实测两本主流网文均严重失效：

| 文件 | 现状 `第.+章` 命中 | 真实结构 | 结果 |
|---|---|---|---|
| 吞噬星空(1-500章) | 745 | 500 章 / 0 卷 | 500 真 + **245 误报** |
| 蛊真人 | 36 | 6 卷 / 2334 节 | **0 真 + 36 全误报** |

《蛊真人》用「节」作章节单位并带 6 卷分卷，当前实现**整本解析为零章节**——这是功能性损坏，而非单纯的体验问题。同时目录 UI 是扁平列表，无法表达「卷 → 章」层级。

## What Changes

- **替换检测引擎**：用「规则集 + 逐行启发式」取代单条正则。引擎逐行 `trim`、跳空行、长度上限（按字符数）、行首锚定、卷优先于章、排除表过滤（部门/部队/节课/集合 等），并支持「多规则竞争」自动适配「章/节/回」等不同计数词。计数词合并为单层分组（卷 → 章），本期不做多级层级。
- **正则库改用 `fancy-regex`**：支持前后向断言，可直接兼容 Legado `TxtTocRule` 规则字符串。
- **支持分卷**：卷作为「平行元数据」存在——扁平章节列表与导航核心（`current_chapter: usize` 索引、字节区间内容提取）**完全不变**，新增 `volumes` 分组信息。
- **可配置规则文件**：内置默认规则集（硬编码兜底）+ 从 `~/.novel/toc_rules.json` 加载用户自定义规则（结构对齐 Legado `TxtTocRule`：`name/rule/is_volume/example/enable`）。呼应 issue #49「用配置文件配置」的诉求。
- **折叠树目录 UI**：`select_chapter.rs` 从扁平 `Select` 改为基于已有依赖 `tui-tree-widget` 的「卷（可折叠）→ 章（叶节点）」两级树；搜索态塌成扁平过滤列表。
- **缓存向后兼容**：`LocalNovelCache` 新增 `volumes` 字段，`#[serde(default)]`，旧缓存可正常读取。
- **(Phase 2) 网络小说分卷**：给 `parse-book-source` 的 `RuleToc`/`Chapter` 增加 `is_volume`（**BREAKING**：影响该 crate 对外 API），网络小说复用同一套折叠树 UI。

## Capabilities

### New Capabilities
- `local-toc-parsing`: 本地 TXT 章节与分卷的检测引擎——规则集、逐行启发式、卷/章识别、多规则竞争选择、可配置规则文件与内置默认值。
- `toc-tree-navigation`: 目录的展示与导航——「卷 → 章」折叠树、展开/收起、搜索过滤、定位当前阅读章；底层扁平索引导航语义保持不变。
- `network-toc-volumes`: (Phase 2) 网络小说书源目录的分卷支持——在 `RuleToc`/`Chapter` 上引入 `is_volume`，使网络小说也能分组显示。

### Modified Capabilities
<!-- 无既有 spec（openspec/specs/ 为空），全部为新增能力。 -->

## Impact

- **代码**：
  - 新增 `src/novel/toc_rule.rs`（规则集 + 检测引擎 + 启发式 + 排除）。
  - 新增规则配置加载（`~/.novel/toc_rules.json`），复用 `utils::novel_catch_dir()`。
  - `src/novel/local_novel.rs`：`request_chapters` 接入新引擎，产出章节 + 卷。
  - `src/novel/novel_core.rs` / `src/cache/local_novel.rs`：章节条目与缓存新增卷元数据（向后兼容）。
  - `src/pages/read_novel/select_chapter.rs`：扁平列表 → `tui-tree-widget` 折叠树。
  - (Phase 2) `crates/parse-book-source`：`RuleToc`/`Chapter` 加 `is_volume`；`src/novel/network_novel.rs` 消费。
- **依赖**：新增 `fancy-regex`。
- **配置/数据**：新增 `~/.novel/toc_rules.json`（可选，缺失时用内置默认）。
- **验收真值**：吞噬星空 = 500 章 / 0 卷；蛊真人 = 6 卷 / 2334 节（两本均已用候选规则 grep 验证 100% 命中）。
- **关联**：issue #49（配置文件驱动）；`parse-book-source` 是独立发布 crate（v0.2.3，crates.io），故网络分卷拆为 Phase 2 以隔离破坏性变更。
