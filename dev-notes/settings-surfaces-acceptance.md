# 阅读、听书与配色设置统一验收

日期：2026-11-15。基线：`d675528`。需求与任务：`openspec/changes/unify-settings-surfaces/`。

## 交互与实现

- 全局设置、阅读弹窗和听书共用 Tabs、限宽分组表单、显式数值加减、直接开关、独占选择器；配色编辑器共用表单与选择器，保留名称和 hex 输入。
- 标签和表单独立聚焦，hover 只反馈，不切页、选项或改值。标签点击只聚焦字段，值区执行对应操作。操作行只有 Enter/点击激活，左右键不能触发重播、准备或释放资源。
- 滚轮浏览不改焦点；返回键盘时显露当前项。一行视口优先显示字段；矮窗口缩减听书说明和留白。
- 模型与音色保留搜索，空结果有提示；配色选择显示真实色块。未完成的 hex 与字段说明保持可见。
- 阅读偏好即时保存，配色仍显式保存/取消。声音仍暂存至下次播放，显式应用重播才改变当前声音；速度、音量和续章立即生效。
- 删除旧 `SettingItem` 隐形左右半行交互，阅读项元数据与边界规则由阅读模块统一提供。

## 验证

通过全功能 locked workspace lib/tests/examples、Clippy `-D warnings`、格式、严格 rustdoc、无默认 feature 与仅 TTS 的 lib 测试、两种主程序构建及严格 OpenSpec 校验。

真实 PTY 回归：

```sh
cargo build --locked --all-features --bin trn
uv run tests/pty/settings_tabs.py
uv run tests/pty/settings_surfaces.py
```

使用隔离 HOME，不读取或修改真实用户配置。不要在 PTY 运行中用不同 feature 重建同一路径的程序。`TRNOVEL_TEST_BINARY` 可指定独立二进制，`TRNOVEL_TEST_ARTIFACTS` 可保存字符网格、颜色和结果。

验证阅读标签、加减和开关、嵌套 Esc、关闭重开；听书模型筛选、空结果、hover 与鼠标确认；配色基准选择、色块、部分 hex、小窗口焦点显露、保存和取消。全局设置原有终端回归保留。对听书操作行发送左右键前后，fixture 收到的请求完全相同，确认不会误触发动作。

以下图像由真实 PTY 字符及颜色网格导出，非设计稿。此轮不包含 Windows 实机或真实模型、音频验收。

截图仅保留在本地 `dev-notes/images/`（Git 忽略），不随仓库上传；文字验收记录和复现脚本继续入库。
