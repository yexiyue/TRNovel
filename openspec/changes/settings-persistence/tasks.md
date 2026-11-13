# Tasks

## 1. 修复

- [x] 1.1 全局设置页增加阅读偏好防抖落盘
- [x] 1.2 增加 `use_on_drop` 同步补写
- [x] 1.3 更新模块注释

## 2. 测试

- [x] 2.1 `ReaderDisplayConfig` 全字段序列化往返
- [x] 2.2 `AppearanceConfig` 全字段（含自定义主题 colors）序列化往返
- [x] 2.3 `ConfigStore` 真实文件读写往返
- [x] 2.4 `SETTING_ROWS` 条目覆盖与文案非空

## 3. 验收

- [x] 3.1 `cargo fmt --all --check`
- [x] 3.2 `cargo clippy --all-targets --all-features --workspace -- -D warnings`
- [x] 3.3 纯本地构建 clippy 与单测
- [x] 3.4 PTY：全局设置改每行字数 → 直接退出 → 磁盘与重启后均为新值
- [x] 3.5 README 说明
