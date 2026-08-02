# reader-paging

## ADDED Requirements

### Requirement: 滚动坐标以内容总行数为唯一基准
阅读正文的滚动状态 SHALL 基于三个量派生：内容总行数 `total`、可见行数 `view`、贴底位置 `end_scroll = total - view`（不足一屏时为 0）。落盘的阅读进度 `line_percent` SHALL 定义为 `current_line / total`，MUST NOT 在分母中包含视口高度。底部行号显示与滚动条 SHALL 同样以 `total` 为分母。

#### Scenario: 换终端尺寸后恢复到同一行
- **WHEN** 用户在某终端高度下读到第 500 行并退出，改变终端高度后重新打开同一本书的同一章
- **THEN** 恢复的位置仍是第 500 行（受整行取整影响，误差不超过 1 行）

#### Scenario: 底部行号显示总行数
- **WHEN** 阅读一章共 1200 行的正文
- **THEN** 底部行号的分母显示 1200，而非「1200 − 视口高度」

#### Scenario: 内容不足一屏
- **WHEN** 某章正文行数少于可见行数
- **THEN** `end_scroll` 为 0，正文不可滚动，向下滚动/翻页直接进入章末边界行为

### Requirement: 翻页步长由重叠行数派生
`PageUp`/`PageDown` 的滚动步长 SHALL 为 `max(view - page_overlap, 1)`，其中 `page_overlap` 取自阅读偏好配置。步长 MUST NOT 被持久化，SHALL 在每次渲染时按当前视口高度重新派生。

#### Scenario: 翻页保留重叠行
- **WHEN** `page_overlap` 为 2、可见 30 行，在第 0 行按下 `PageDown`
- **THEN** 滚动到第 28 行，即上一屏最后 2 行成为新一屏的开头

#### Scenario: 重叠为零时整屏平移
- **WHEN** `page_overlap` 为 0、可见 30 行，在第 0 行按下 `PageDown`
- **THEN** 滚动到第 30 行，上下屏无公共行

#### Scenario: 终端过矮时步长不为零
- **WHEN** 终端高度使 `view` 小于等于 `page_overlap`
- **THEN** 步长取 1，翻页仍然可以推进，MUST NOT 卡死

#### Scenario: 终端尺寸变化后步长自适应
- **WHEN** 用户在阅读过程中放大终端窗口
- **THEN** 后续翻页按新的可见行数减去重叠行数滚动，无需重启或改配置

### Requirement: 末尾翻页留白而非贴底钳位
当剩余内容不足一整屏时，`PageDown` SHALL 仍按完整步长滚动、允许最后一行之下出现留白，MUST NOT 将落点钳到贴底位置。逐行滚动（`ScrollDown`）SHALL 保持贴底钳位不变。

#### Scenario: 末尾翻页步长保持一致
- **WHEN** 共 100 行、可见 30 行、重叠 2 行，从第 56 行按下 `PageDown`
- **THEN** 滚动到第 84 行（步长仍为 28），屏幕下方出现 14 行留白，正文最后一行仍可见

#### Scenario: 逐行滚动仍然贴底
- **WHEN** 用户用向下滚动键持续滚到章末
- **THEN** 停在贴底位置（最后一行位于屏幕底部），不出现留白

### Requirement: 进度百分比 1.0 表示章末
`line_percent` 为 `1.0` 时 SHALL 渲染为贴底位置（最后一屏完整可见），而非按绝对坐标直译到内容末行。用户滚动产生的百分比 SHALL 始终小于 1.0，MUST NOT 与该语义位置冲突。

#### Scenario: 跳到结尾
- **WHEN** 用户按下「跳到结尾」
- **THEN** 正文停在贴底位置，最后一屏完整可见，屏幕下方无留白

#### Scenario: 顶部向上翻回上一章
- **WHEN** 用户在章首连按向上滚动键翻回上一章
- **THEN** 上一章停在贴底位置，最后一屏完整可见

#### Scenario: 留白位置往返稳定
- **WHEN** 用户翻页进入留白区后退出并重新进入该章
- **THEN** 恢复到同一留白位置，MUST NOT 被误判为章末而跳到贴底

### Requirement: 翻页键与滚动键共用边界行为
`PageUp`/`PageDown` 在章首/章末 SHALL 走与 `ScrollUp`/`ScrollDown` 完全相同的边界逻辑：全书首/末章只提示不武装；章内边界首次按下仅武装并提示，连续第二次才翻章；任何其他滚动操作解除武装。两组按键的边界逻辑 SHALL 为单一实现，MUST NOT 各自复制一份。

#### Scenario: 章末翻页进入下一章
- **WHEN** 用户用 `PageDown` 滚到章末后再次按下 `PageDown`
- **THEN** 底部提示「已到本章末尾 · 再按一次进入下一章」，第三次按下才真正进入下一章

#### Scenario: 章首翻页返回上一章
- **WHEN** 用户用 `PageUp` 滚到章首后再次按下 `PageUp`
- **THEN** 底部提示「已到本章开头 · 再按一次返回上一章」，再次按下返回上一章并落到其章末

#### Scenario: 全书边界只提示不翻章
- **WHEN** 用户在最后一章章末反复按 `PageDown`
- **THEN** 持续提示「已是全书最后一章」，MUST NOT 承诺不存在的下一章

#### Scenario: 翻页解除武装
- **WHEN** 章末已武装后用户按下向上翻页
- **THEN** 武装解除，提示消失，需重新滚到章末才能触发翻章

### Requirement: 整页翻页键提供 Vim 默认绑定
`page_up`/`page_down` 的默认键位 SHALL 在物理 `PageUp`/`PageDown` 之外补充 `ctrl-b`/`ctrl-f`。这两个 action 的配置键名保持不变，用户既有 `keybindings.toml` 覆盖 MUST 继续生效。

#### Scenario: Vim 键翻页
- **WHEN** 无用户配置时在阅读页按 `Ctrl-F`
- **THEN** 正文向下翻一页，行为与按 `PageDown` 完全一致

#### Scenario: 用户覆盖仍然优先
- **WHEN** 用户在 `keybindings.toml` 中为 `page_down` 指定了自定义键
- **THEN** 以用户配置为准，默认表中的键不再额外生效
