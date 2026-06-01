## Context

TRNovel 本地 TXT 章节检测当前是 `src/novel/local_novel.rs:113` 的单条正则 `第.+章` + 逐行 `is_match`。数据模型为 `NovelChapters<(String, usize)>`（标题, 字节偏移）的扁平列表，`current_chapter: usize` 作为导航游标，`get_content()` 用「本章偏移 → 下一章偏移」的字节区间读取。目录 UI（`select_chapter.rs`）是扁平 `Select<ChapterName>` + 搜索过滤。

经两本真实样本验证，现状的失效已量化（见 proposal）。根因三条：① 子串匹配无行首锚 → 正文「第三章第二十五条」被误判；② 计数词写死「章」→ 用「节/回」的书全军覆没；③ 无长度上限、无卷概念。

约束：
- Rust 标准 `regex` crate **不支持**前后向断言，而要兼容的 Legado `TxtTocRule` 规则大量依赖断言。
- `parse-book-source` 是独立发布 crate（crates.io v0.2.3），改其对外 API 属破坏性变更，需隔离。
- 内容读取依赖字节偏移，且文件可能是 GBK/UTF-8（已有编码探测）。

## Goals / Non-Goals

**Goals:**
- 本地 TXT 章节检测从「单正则」升级为「规则集 + 逐行启发式引擎」，消除误报/漏报。
- 支持「卷 → 章」单层分组，且**不改动导航核心与字节区间内容提取**。
- 规则集可配置（`~/.novel/toc_rules.json`），内置默认值兜底，呼应 issue #49。
- 目录改为可折叠树（卷 → 章）。
- 两本真值样本（吞噬星空 500/0、蛊真人 6/2334）作为验收基线。

**Non-Goals:**
- 不做「计数词决定层级」的多级目录（章=卷、节=子章）。计数词 `[章节回话]` 合并为单层。
- 本期不改网络小说（Phase 2 再做 `is_volume`）。
- 不做基于 NLP/分类器的章节判定（仅正则 + 启发式）。
- 不引入 OCR、不处理非 TXT 格式。

## Decisions

### D1：正则库用 `fancy-regex`（而非标准 `regex`）
- **理由**：支持 `(?<=)`/`(?!)` 断言，可直接吃 Legado `TxtTocRule` 规则字符串；本项目本就在解析 Legado 书源，用户可直接复用 Legado 社区规则。
- **取舍**：多一个依赖、回溯式引擎略慢于标准 `regex`。检测只在「加载/请求章节」时跑一次（非热路径），可接受。
- **备选**：标准 `regex` + 代码层模拟断言（kaf-cli 路线）——无新依赖但无法直接兼容 Legado 规则，已否决。

### D2：检测引擎走「逐行 + 多重过滤」（kaf-cli 风格），正则只做粗筛
逐行流式（沿用现有 `read_until(b'\n')`）：
1. 解码后 `trim`（同时吃掉全角空格 `　`、`\r`）。
2. 跳过空行。
3. 长度上限：`chars().count() <= MAX`（默认 35）才视为候选标题。
4. **卷优先于章**：先试卷规则，命中即标 `is_volume`；否则试章规则。
5. **排除表优先**：命中排除规则（`第X部门/部队/部分/节课/集合` 等）直接丢弃。
6. （启发式增强）标题行不以 `。！？，` 结尾（正文才以句末标点结尾）。
- **理由**：行首锚 + 长度上限 + 计数词字符类，实测在两本书上即达 100% 精确率/召回率。

### D3：多规则竞争自适应（移植 Legado `getTocRule`）
- 跑所有启用规则，统计各自命中数（仅计「相邻命中间隔足够大」的，避免连续误报刷分），选命中最多且分布均匀者作为该书的主规则。
- **理由**：蛊真人用「节」、吞噬星空用「章」，单一合并字符类已覆盖；但竞争机制能进一步自适配奇葩格式，鲁棒性更强。
- **取舍**：实现稍复杂；可作为引擎内部策略，对上层透明。

### D4：卷作「平行元数据」，导航核心零改动 ★
- 章节仍是扁平 `Vec<(title, offset)>`；新增 `volumes: Vec<VolumeMarker { title, first_chapter_index }>`。
- `current_chapter: usize`、`next/prev/set_chapter`、`get_content()` 的字节区间逻辑**完全不变**。
- 卷标题自身的偏移可记录用于「卷前言」展示，但不进入章节导航序列。
- **理由**：内容读取本就是线性字节区间，分卷纯属展示/分组关注点，元数据化可把改动面降到最低，且网络小说沿用同模型。
- **备选**：嵌套 `Vec<Volume{chapters}>`——模型更干净但牵动 `Novel` trait、network、缓存、history、百分比，否决。

### D5：可配置规则文件 `~/.novel/toc_rules.json`
- 结构对齐 Legado `TxtTocRule`：`{ name, rule, is_volume, example, enable, serial_number }`。
- 加载策略：内置默认规则集（编译进二进制）作兜底；若 `~/.novel/toc_rules.json` 存在则合并/覆盖。复用 `utils::novel_catch_dir()`。
- **理由**：直接回应 issue #49；让用户无需改代码即可适配新格式。

### D6：折叠树目录用 `tui-tree-widget`
- 已是项目依赖（`file_select.rs` 已用 `TreeSelect<T>` + `TreeState`）。卷=可折叠父节点，章=叶节点，叶节点标识携带**扁平章节索引**，选中 → `on_select(flat_index)`，导航语义不变。
- 无卷的书：全部章节平铺在根层（与当前体验一致）。
- 搜索态：塌成扁平过滤列表（沿用现有 `$index` / 子串过滤）；清空搜索回到树。

### D7：缓存向后兼容
- `LocalNovelCache` 新增 `volumes`（`#[serde(default)]`）。旧缓存无该字段时反序列化为空 → 退化为无卷展示，不报错。

### D8：分两期，隔离破坏性变更
- Phase 1：本地 TXT 全部（D1–D7）。
- Phase 2：`parse-book-source` 的 `RuleToc`/`Chapter` 加 `is_volume`（BREAKING，需 crate 版本号 bump），`network_novel.rs` 消费并复用树 UI。

## Risks / Trade-offs

- **缩进容忍 vs 误报**：允许标题前 0–4 空白会在吞噬星空引入 8 个误报（实测 500→508）。→ 缓解：默认 `trim` 后整行匹配 + 长度上限 + 「不以句末标点结尾」过滤；必要时规则可声明严格顶格。
- **fancy-regex 回溯性能 / ReDoS**：用户自定义规则可能写出灾难性回溯。→ 缓解：检测仅在章节加载时一次性运行；可对单行匹配设上限、对超长行直接跳过（已有长度上限）。
- **`章+节` 两级书被压平**：本期把 `[章节回话]` 合并为单层，章=卷、节=子章 的书会被压成一层。→ 缓解：数据结构预留层级字段，列为未来扩展（Non-Goal 已声明）。
- **多实例并发写缓存**：与现状一致的已知问题（单 JSON 文件无锁），本期不引入也不解决。
- **Phase 2 crate 破坏性变更**：`parse-book-source` 对外 API 变动影响下游。→ 缓解：`is_volume` 用 `#[serde(default)]` + 默认 `false`，bump minor 版本，隔离到 Phase 2。
- **真值漂移**：精校版与网络源章节数可能不同。→ 验收以 `test-novels/` 本地样本为准（吞噬星空 500/0、蛊真人 6/2334）。
