# Spec: global-settings

## Requirement: 统一的设置入口

首页菜单「全局设置」与顶栏「全局设置」按钮 MUST 导航到 `/settings`，而不是主题设置页。

### Scenario: 从首页进入

- **WHEN** 用户在首页选择「全局设置」并回车
- **THEN** 打开 `/settings`，左栏显示「阅读 / 外观 / 键位 / 数据 / 关于」五个分组，默认停在「阅读」

### Scenario: 关闭并返回

- **WHEN** 用户在设置页按 Esc
- **THEN** 返回上一页（`navigate.go(-1)`），不写入任何配置

## Requirement: 阅读分组与浮层一致

设置页「阅读」分组的条目顺序与文案 MUST 与 `describe_setting` 的全局序号一致，←/→ 调值后立即作用到阅读页，且不需要重启。

### Scenario: 调整每行字数

- **WHEN** 光标停在「每行字数」并按 →
- **THEN** 值从「跟随窗口」变为「10 字(20 列)」，阅读页版心随之收窄；按 ← 减到下限后回到「跟随窗口」

## Requirement: 只读信息如实展示

键位分组 MUST 显示当前实际绑定（随 `keybindings.toml` 变化）；数据分组 MUST 显示本次运行实际使用的三个目录；关于分组 MUST 反映当前二进制的构建特性。

### Scenario: 纯本地构建

- **WHEN** 以 `--no-default-features --features tts` 构建并打开「关于」
- **THEN** 构建一行显示「纯本地（无网络书源）+ 朗读」
