# Tasks: reader-polish

## 1. Esc 返回

- [x] 1.1 搜索为空且不在编辑时 `ClearContentSearch` 返回 Ignored
- [x] 1.2 PTY 验证：打开 txt → Esc 回到文件选择页

## 2. 阅读设置浮层

- [x] 2.1 `SETTING_ROWS` 恢复全部条目（TTS 构建 12 项）
- [x] 2.2 `visible_rows` 按终端高度计算，面板高度动态
- [x] 2.3 跨页整页滚动，12 项全部可达
- [x] 2.4 PTY 验证两页条目

## 3. 目录折行

- [x] 3.1 `wrap_title` 按显示宽度折行，CJK 计 2 列
- [x] 3.2 分卷与子章按层级扣减宽度
- [x] 3.3 单测 3 个
- [x] 3.4 PTY 验证 1/4 目录栏下长章节名完整显示

## 4. 留白与状态栏

- [x] 4.1 删除边框 padding，正文改用内层 View margin
- [x] 4.2 状态栏不随留白缩进
- [x] 4.3 PTY 验证 h_margin=6 时进度贴角

## 5. 正文换行

- [x] 5.1 换行预算扣除首行缩进，缩进过大时钳制
- [x] 5.2 跳过 textwrap 返回的空段，宽度 0 保持逐字行
- [x] 5.3 单测：缩进不超宽/不丢字、超大缩进钳制、中文标点不落行首
- [x] 5.4 PTY 验证 146 个连续汉字在 indent=8 下完整渲染
- [x] 5.5 `total` 取预排版行数

## 6. 文档与验收

- [x] 6.1 README 更新新增行为
- [x] 6.2 `cargo fmt --check`、clippy（all-features / 纯本地）零警告
- [x] 6.3 全量 lib 测试（migration 环境问题除外）
- [ ] 6.4 截图归档（可选）