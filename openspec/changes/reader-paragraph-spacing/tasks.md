# Tasks: reader-paragraph-spacing

## 1. 配置层

- [x] 1.1 `src/cache/setting.rs`：`ReaderDisplayConfig` 增 `paragraph_spacing: bool`（`#[serde(default)]`，默认 `true`），并入 `Default`
- [x] 1.2 单测：默认值为 true、旧 JSON 缺字段走默认且不影响既有字段

## 2. 正文排版

- [x] 2.1 `append_wrapped_line` 增 `spacing` 参数：段落间空行受开关控制，**原文自带空行始终保留**（顺带合并原本重复的两个同体 if）
- [x] 2.2 `wrap_content` / `highlight` 透传开关；`ReadContent` 从 `READER_DISPLAY` atom 读取
- [x] 2.3 `paragraph` 的 `use_memo` deps 补上该开关，否则切换后排版冻结在首帧
- [x] 2.4 单测：开关关闭时段落不插空行、开启时插一个、原文空行两种模式下都保留且不重复

## 3. 设置面板

- [x] 3.1 `ITEM_COUNT` 2 → 3，新增「段落间距」条目（与「显示标题」同构）
- [x] 3.2 面板高度加一个条目的高度，避免第三项被裁掉

## 4. 验收与收尾

- [x] 4.1 全套 CI 检查（test / clippy -D warnings / fmt / doc）
- [x] 4.2 VHS 实测：面板三项导航、开关切换后正文排版即时变化、行数分母随之变化
- [x] 4.3 更新 docs 站 `read.mdx` 的「阅读设置」章节
- [x] 4.4 更新知识库（如产生新踩坑）
