# Spec: reader-navigation-keys

## Requirement: 空格翻页

阅读页中 Space MUST 与 PageDown 等价（章末连按翻下一章、章首连按翻上一章）。它 MUST 出现在 `page_down` 的绑定列表里，因此可被 `keybindings.toml` 重绑。

### Scenario: 空格下翻

- **WHEN** 阅读页按 Space
- **THEN** 正文向下翻一页，步长等于「可见行数 - 翻页重叠」

### Scenario: 重绑后失效

- **WHEN** 用户在 `keybindings.toml` 里把 `page_down` 改成 `["f"]`
- **THEN** Space 不再翻页，`f` 翻页

## Requirement: 方向键翻页开关

`ReaderDisplayConfig.arrows_page` 默认 false，旧配置缺字段 MUST 按 false 加载。开启后，实际按下的 ↑/↓ MUST 等价于 PageUp/PageDown；`j`/`k` 与其他绑到滚动动作的自定义键 MUST 保持逐行滚动。

### Scenario: 开启后整页翻

- **WHEN** 在设置里把「↑/↓ 翻页」调成「开」并回到正文按 ↓
- **THEN** 向下翻一页；到章末首次只提示「再按一次翻下一章」，第二次才翻章

### Scenario: 关闭后逐行滚

- **WHEN** 把该开关调回「关」并按 ↓
- **THEN** 正文向下滚动一行
