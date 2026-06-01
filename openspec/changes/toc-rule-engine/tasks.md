## 1. 依赖与规则数据结构

- [x] 1.1 在根 `Cargo.toml` 添加 `fancy-regex` 依赖
- [x] 1.2 新建 `src/novel/toc_rule.rs`：定义 `TocRule { name, rule, is_volume, enable, example, serial_number }`（serde，对齐 Legado `TxtTocRule`）
- [x] 1.3 定义检测产物类型：`TocEntry { title, offset, is_volume }` 或等价结构，供引擎输出
- [x] 1.4 编写内置默认规则集（卷规则、章/节/回规则、特殊章节规则、排除规则），编译进二进制

## 2. 检测引擎（核心）

- [x] 2.1 实现逐行扫描：解码 + `trim`（含全角空格/`\r`）+ 跳空行 + 长度上限（默认 35，按 `chars().count()`）
- [x] 2.2 实现卷优先匹配 + 排除表过滤 + 「不以句末标点结尾」启发式
- [x] 2.3 实现章节规则匹配，记录每条标题的字节偏移
- [x] 2.4 实现多规则竞争选择（按有效命中数 + 相邻命中最小间隔评分，选主章节规则）
- [x] 2.5 输出扁平章节列表 + 平行 `volumes`（卷标题 + 首章索引）
- [x] 2.6 单元测试：覆盖「第N章」「第N节」「卷优先」「正文引用不误判」「超长行排除」

## 3. 规则配置加载

- [x] 3.1 实现从 `~/.novel/toc_rules.json` 加载用户规则（复用 `utils::novel_catch_dir()`）
- [x] 3.2 合并策略：内置默认兜底 + 用户规则覆盖/追加
- [x] 3.3 配置缺失/损坏时安全回退到内置默认（不崩溃）
- [x] 3.4 单元测试：无文件、正常文件、损坏文件三种路径

## 4. 数据模型与缓存接入

- [x] 4.1 在本地小说章节数据中承载 `volumes` 平行元数据（不改 `current_chapter`/字节区间逻辑）
- [x] 4.2 `src/novel/local_novel.rs::request_chapters` 改为调用新引擎（`request_toc`），产出章节 + 卷
- [x] 4.3 `src/cache/local_novel.rs::LocalNovelCache` 新增 `volumes` 字段（`#[serde(default)]`）
- [x] 4.4 验证旧缓存（无 `volumes`）可正常反序列化并退化为无卷展示

## 5. 折叠树目录 UI

- [x] 5.1 `src/pages/read_novel/select_chapter.rs`：用 `tui-tree-widget` 的 `TreeSelect` 构建「卷 → 章」树
- [x] 5.2 叶节点标识携带扁平章节索引；选中叶节点触发既有 `on_select(flat_index)`
- [x] 5.3 无卷时章节平铺在根层级
- [x] 5.4 搜索态塌缩为扁平过滤列表（保留子串过滤 + `$index` 定位），清空恢复树
- [x] 5.5 打开目录时定位/高亮当前章节并展开其所属卷

## 6. 真值验收与回归

- [x] 6.1 集成测试/脚本：对 `test-novels/吞噬星空(1-500章).txt` 断言 500 章 / 0 卷
- [x] 6.2 集成测试/脚本：对 `test-novels/《蛊真人》…txt` 断言 6 卷 / 2334 章（节）
- [x] 6.3 跑 `cargo clippy --all-targets --all-features --workspace -- -D warnings` 与 `cargo fmt --all --check`
- [ ] 6.4 手动验证：在两本书中打开折叠树、展开卷、跳章、搜索、续读位置正确（需在交互终端中由人工确认）

## 7. 文档

- [x] 7.1 在 `docs/` 增补：目录规则文件 `~/.novel/toc_rules.json` 的格式与示例（关联 issue #49）
- [x] 7.2 README/CHANGELOG 记录章节解析与分卷能力（CHANGELOG 由 git-cliff 在发布时自动生成）

## 8. Phase 2：网络小说分卷（独立提交，破坏性变更隔离）

- [x] 8.1 `crates/parse-book-source`：`RuleToc` 新增可选 `isVolume` 规则字段
- [x] 8.2 `Chapter` 新增 `is_volume: bool`（`#[serde(default)]`），`parse_to_chapter` 填充
- [x] 8.3 bump `parse-book-source` 版本号（0.2.3 → 0.3.0，同步更新根依赖）
- [x] 8.4 `src/novel/network_novel.rs` 消费 `is_volume`（`split_volumes`），产出 `volumes` 元数据
- [x] 8.5 网络小说复用折叠树目录；含卷分组、不含卷平铺（经 `ReadNovel<T>` 通用 `get_volumes` 路径自动生效）
- [x] 8.6 回归：既有书源（无 `isVolume`）行为不变（`no_volumes_when_none_flagged` 测试 + serde 默认）
