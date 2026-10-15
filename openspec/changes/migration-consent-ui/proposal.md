# Proposal

## Why

v0.18.0 起的启动自动迁移在 TUI 绘制前同步运行：成功路径零输出（大目录逐文件 fsync，559 MB/2068 文件实测可达分钟级），终端表现为无提示假死；部分失败后的重试对每个已存在文件打印一行「保留新文件…未覆盖」，用户机器上刷约 1800 行。用户全程无感知、无控制权，也无法显式触发或跳过。

## What Changes

- 移除启动路径上的整体自动迁移（`migration::startup()` 不再在 TUI 前同步执行拷贝）；改为**启动时快速状态检查**（stat 级：遗留目录 + 三个标记文件），TUI 内首次运行弹窗询问是否迁移。
- 新增 `trn migrate` 子命令：独立迁移界面，运行前展示计划（条目、文件数、总大小），迁移中展示进度（总进度条、当前条目、已拷贝/保留/跳过计数），支持取消，结束展示汇总；`--yes` 跳过界面直接迁移并打印汇总（脚本/无终端场景）。
- 新增 `DECLINED` 标记（`.legacy-migration-declined-v1`）：用户在首次运行弹窗选择跳过后写入，此后不再自动询问；`trn migrate` 不受其影响，随时可显式迁移。
- 应用内首启弹窗选择「迁移」时，在同一界面内运行迁移并展示进度，**完成后才加载缓存**（历史/书源读到的是迁移后的数据）；选择「跳过」或迁移结束（含取消、失败）后正常进入应用，任何迁移结果都不阻断启动。
- 迁移引擎改为**可观察、可取消**：进度快照（条目/文件/字节三级计数 + 保留/跳过/错误汇总）写入进程级 Atom 供 UI 订阅；协作式取消标志在文件间检查，取消后不发布完成标记，下次启动可续迁。
- 性能：目录 fsync 从逐文件改为收尾按去重目录批量执行（文件内容 fsync 与完成标记语义保留）。
- 「保留已有新文件」「跳过非普通文件」等重试输出从逐行 eprintln 收敛为计数与简短汇总，不再刷屏。
- 非 TUI 子命令（tts/clear/doctor/import/gen-fontmap）不再触发任何迁移逻辑；`clear` 的 CLEARED 语义与既有 MARKER/锁/不覆盖/不删除源文件约定全部保留。

## Capabilities

### New Capabilities

- `legacy-migration`：旧版 `~/.novel` 数据到 `~/.trnovel` 的迁移生命周期——状态检查（待迁移/已完成/已拒绝/无需迁移）、用户同意门控、显式 `trn migrate` 命令、进度与取消反馈、完成/跳过标记及崩溃重试语义。

### Modified Capabilities

（无——现有 reader-paging / reader-settings-panel 的需求不受影响。）

## Impact

- `src/migration.rs`（+`src/migration/`）：核心重构为状态检查 + 进度引擎 + 取消；标记常量新增 DECLINED。
- `src/lib.rs`：移除 `migration::startup()` 调用；新增 `Commands::Migrate` 分派（TUI 界面与 `--yes` 无头模式）。
- `src/app.rs`：启动初始化改为「迁移门控 → 加载缓存」，`MigrationModal` 挂载进根组件树。
- `src/components/modal/`：新增迁移弹窗/进度组件（复用 BrowserPromptModal 的 Atom + 独占输入层模式）。
- `docs/src/content/docs/guides/migration.mdx`：更新为新的迁移交互说明。
- 不新增依赖；不改变 `~/.trnovel` 目录布局与配置格式。
