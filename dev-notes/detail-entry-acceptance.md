# 设置标题与网络详情阅读入口验收

日期：2026-11-15。

- 设置正文去掉“设置”标题及其占位空行，直接显示 Tabs，header 开关两种状态均验证。
- 网络详情的主要阅读动作改为页内常驻按钮；缓存书籍显示“继续阅读”，新书显示“开始阅读”。加载中禁用；已有 Enter、L、→ 仍调用相同的打开逻辑。按钮固定在详情页底部居中，长简介独立滚动；header 隐藏时 hover 提示不会覆盖入口。
- 使用 loopback HTTP 书源、隔离 HOME、预置第二章阅读记录，鼠标从历史进入详情并继续阅读。验证加载时点击不导航，加载后 hover 反馈、单击进入并请求 `/second`，未请求 `/first`；返回后 Enter 仍可续读。长简介滚动及 52×18 窗口下，按钮保持底部居中。
- 全功能 workspace 测试、Clippy、格式、严格 rustdoc，无默认/仅 TTS 测试及主程序构建通过。原有全局设置 PTY 回归保留。

```sh
cargo build --locked --all-features --bin trn
uv run tests/pty/book_detail.py
uv run tests/pty/settings_tabs.py
```

图像来自真实 PTY 的字符/颜色网格导出。

截图仅保留在本地 `dev-notes/images/`（Git 忽略），不随仓库上传；文字验收记录和复现脚本继续入库。
