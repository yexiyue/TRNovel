# Tasks: reader-display-controls

## 1. 配置层

- [x] 1.1 `ReaderDisplayConfig` 新增 `show_hint_bar` / `lines_per_row` / `first_line_indent` / `paragraph_gap` / `h_margin` / `frameline` / `show_frameline_title`，全部 `#[serde(default)]`
- [x] 1.2 新增 `Frameline` 枚举（顺序、标签、inset、字符集、next/previous）
- [x] 1.3 `normalize()` 钳制各字段并把 `paragraph_spacing` 与 `paragraph_gap` 同步
- [x] 1.4 各字段的增减方法（钳制统一在配置层，面板只报方向）
- [x] 1.6 单测：旧配置缺字段走默认、边框循环与序列化、钳制边界、`0` 为「跟随终端」的特殊值、旧 `paragraph_spacing` 升级

## 2. 排版

- [x] 2.1 `ContentLayout::with_options` 接收 `LayoutOptions { width, paragraph_gap, indent }`
- [x] 2.2 段首缩进只加在段首可视行，不进入 ranges
- [x] 2.3 段间距按 `gap` 精确补空行，不与原文空行叠加
- [x] 2.4 旧 `ContentLayout::new` 退为测试辅助（`#[cfg(test)]`）
- [x] 2.5 单测：缩进/段间距不改变搜索高亮偏移

## 3. 阅读页渲染

- [x] 3.1 `visible_lines(height, &display)` 统一计算 chrome，三个调用点传快照
- [x] 3.2 版心宽度 = 可用宽度 − 2×边框 − 2×留白，与「每行字数」取小
- [x] 3.3 边框：`borders`/`border_set`/`padding`，`none` 时隐藏标题
- [x] 3.4 正文与状态栏之间插入提示行，高度随配置为 0/1 行
- [x] 3.5 `keymap::hint_bar` 由键位映射生成并按显示宽度裁剪
- [x] 3.6 单测：`visible_lines` 不为 0、`hint_bar` 在 0..60 列都不溢出

## 4. 阅读设置面板

- [x] 4.1 条目扩到 9（+TTS）项，改双列布局，面板宽 80%
- [x] 4.2 数值型条目的增减语义与「0 = 跟随终端」的处理
- [x] 4.3 面板高度足够容纳全部条目

## 5. 文档与验收

- [x] 5.1 README 新增「阅读页排版与外观」小节
- [ ] 5.2 VHS/截图验收：版心、缩进、五种边框、提示条、面板双列
- [ ] 5.3 更新 docs 站阅读设置章节
- [ ] 5.4 四件套全绿（test / clippy -D warnings / fmt / doc）
