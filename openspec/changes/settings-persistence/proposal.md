# 设置持久化审计与修复

## Why

全局设置页的「阅读」分组只把新值写进进程内的 `READER_DISPLAY`，而写盘逻辑只存在于阅读页
`ReadNovel` 的防抖 effect 里。用户在这一页调完值直接返回首页或退出，改动不会进
`config.toml`；下次启动仍是旧值。其余分组与主题设置页都是即时保存，只有这一条链路断了。

## What Changes

- 全局设置页补上阅读偏好的防抖落盘与卸载兜底，与阅读页共用同一语义：改完约 500ms 写盘，
  切页/退出前同步补写，写失败不推进已保存快照、下次改动重试。
- 设置页模块注释不再声称「不直接落盘」。
- 新增逐字段序列化测试与整份配置落盘往返测试，覆盖 `ReaderDisplayConfig` 全部 13 个字段、
  `AppearanceConfig` 全部 5 个字段（含自定义主题的 `colors` 覆盖表）。
- 新增设置浮层条目表完整性测试：`SETTING_ROWS` 恰好覆盖每个条目一次，且每项都有标签与值文案。
- README 写明全局设置页的改动会立即持久化。

## Capabilities

### New Capabilities

- `settings-persistence`：所有设置项的读、写、落盘与重启恢复行为一致，且字段无遗漏。

### Modified Capabilities

- `global-settings`：阅读偏好在本页修改后立即持久化。

## Impact

- `src/pages/settings/mod.rs`
- `src/config.rs`、`src/cache/setting.rs`
- `src/pages/read_novel/settings/mod.rs`
- `README.md`
