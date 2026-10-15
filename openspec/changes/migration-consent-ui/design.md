# Design

## Context

v0.18.0 起 `migration::startup()` 在 clap 解析后、TUI 绘制前同步执行整体迁移（change `integrate-talechime-v7` 的 Migration Plan）。成功路径零输出、逐文件 fsync，559 MB/2068 文件量级下终端假死数分钟；部分失败后的重试对每个已存在文件逐行打印。诊断结论：拷贝机制（tempfile + noclobber rename + 锁 + 标记 + 源文件保留）是健全的，问题全部在**可见性与位置**。本变更保留机制，重构交互。

## Goals / Non-Goals

迁移永远可见、可取消、可重试；同意门控 + 显式命令双入口；任何结果不阻断启动。不改变 `~/.trnovel` 布局、配置格式与 `clear` 语义；不引入新依赖；不做数据条目的轻重拆分（用户已同意整体迁移，仅改为同意后执行）。

## Decisions

- **状态检查与执行分离**：`status()` 只做 stat 级检查（`~/.novel` 是否普通目录 + `MARKER`/`CLEARED`/`DECLINED` 标记），返回 NotNeeded/Done/Declined/Pending。执行入口 `run()` 接受进度快照槽与取消标志。扫描（文件数/字节数）仅在 Pending 时由 UI 后台执行，用于弹窗概览；扫描是尽力而为，失败不阻塞迁移。
- **进度经进程级 Atom**：`static MIGRATION: Atom<MigrationUi>`（BROWSER_PROMPT 同模式），迁移线程直接写快照（条目/文件/字节计数、当前条目、保留/跳过/错误计数、终态）。写入频率为每文件一次，仅唤醒订阅组件；快照是小结构，克隆成本可忽略。
- **取消是协作式的**：`Arc<AtomicBool>` 在每个文件拷贝前检查；取消后不发布 MARKER，已拷贝文件因 noclobber 语义天然成为下次的「保留已有」。Ctrl+C（框架默认退出）等价于取消。
- **fsync 批量化**：文件内容 fsync 保留（单文件粒度，成本低）；父目录 fsync 从逐文件改为收集去重目录、在发布 MARKER 前统一执行。MARKER 发布路径不变（仍含自身 fsync），维持「完成标记不早于内容持久化」的次序。
- **首启门控放 App 初始化 future**：初始化 future 先做 `status()`；Pending 时置 Atom 为 Ask 并 `DECIDED.notified().await`（`tokio::sync::Notify` 为 const 构造），弹窗回调决定后 notify，future 再加载缓存——保证「迁移后才加载缓存」。Decline 写 `DECLINED` 标记（`publish` 原子写入，clear 不触碰根级标记）。
- **一套迁移视图，两个宿主**：`MigrationView`（询问/进度/汇总三态渲染 + `OutlineButton` 鼠标按钮与 `Gauge` 进度条）被 `MigrationModal`（应用内，挂 App 根，独占输入层，Esc 取消、Enter 关闭）与 `MigrationRoot`（`trn migrate` 独立全屏根，Enter/q 退出）复用。`trn migrate --yes` 用打印报告器无头执行，供脚本与测试。
- **进度条与鼠标**：进度用 ratatui `Gauge`（`widget()` 桥接，label 为百分比）；按钮用应用自有 `components::OutlineButton`（描边风格：透明底 + 彩色边框与同色文字，悬停以描边色整块填充、文字按 WCAG 亮度配黑/白；Primary=前景色、Secondary=强调色、Danger=错误色）。左键 Down 触发，作为 Modal/根的子组件自动落在宿主输入层，无需手动绑层。键盘（Enter/Y/N/Esc/q）与鼠标双通道并存，底栏提示两者。独立根自带 `PaletteProvider`（读 `APPEARANCE` Atom），与应用根同源取色——否则脱离应用树的组件回退框架默认调色板。
- **非 TUI 子命令零迁移**：`migration::startup()` 从 `try_run_inner` 移除；tts/clear/doctor/import/gen-fontmap 不再触碰迁移。帮助/版本行为不变（clap 内退出）。
- **重试输出收敛**：原 8 处 `eprintln!` 全部收进引擎的快照计数（kept/skipped/errors），无头模式打印计数汇总与错误列表，不再逐文件输出。

## Risks / Trade-offs

- 目录 fsync 批量化后，极端掉电窗口内「重命名已可见但内容未刷盘」理论上存在；但 MARKOR 发布前统一 fsync 目录 + 内容逐文件 fsync 已覆盖既有持久化承诺，且源文件始终保留、可重迁，风险可接受。（实测 48 MB/29 文件含 fsync 的全量迁移在 50ms 内完成。）
- 首启弹窗挡在缓存加载前，用户不决定就无法进入应用——这是刻意的（不决定就不该读到「看似空」的书架）；跳过一键可达，弹窗文案直接给出 `trn migrate` 的后续入口。
- 进度按字节比例，极小文件瞬时跳变——条目名 + 计数提供稳定参考，进度条只做趋势。
- 按钮行 3 行已计入各状态高度；内容列不使用 `gap`（每子项 1 行 gap 会让高度计算随子项数平方膨胀并裁掉按钮——知识笔记的「面板高度」坑），改用 1 行显式间隔。

## Migration Plan

发布后首次运行旧用户会遇到询问弹窗（而非静默假死）；已迁移用户（MARKER 存在）无感知。v0.18.0–0.18.2 期间部分迁移留下的中间态对 `status()` 而言仍是 Pending，按新流程重试即可。`docs` 的 migration 指南同步改写；`AGENTS.md` 的迁移描述更新为门控语义。
