# Tasks

- [x] 1. 重构 `src/migration.rs` 核心：`status()` 状态检查（含 `DECLINED` 标记）、进度快照结构、协作式取消、目录 fsync 批量化、输出收敛为计数；`migrate()` 保留为测试/无头兼容入口
- [x] 2. 新增迁移 UI 状态 Atom 与 `src/migration/ui.rs`：`MigrationView`（询问/进度/汇总、左右键选择与告警式按钮）+ `MigrationModal`（应用内宿主）
- [x] 3. `src/app.rs` 启动门控：Pending 时经 `Notify` 等待决定后再加载缓存；挂载 `MigrationModal`
- [x] 4. `src/lib.rs`：移除 `migration::startup()` 自动迁移；新增 `Commands::Migrate { yes }` 分派（`MigrationRoot` 全屏 TUI / `--yes` 无头报告器）
- [x] 5. 测试：status 转移、declined 持久化与 clear 存活、取消不发布标记且续迁复用、计数报告（kept/skipped 汇总）、无头迁移端到端
- [x] 6. 文档：`docs/src/content/docs/guides/migration.mdx` 改写为新交互；`AGENTS.md` 迁移段更新
- [x] 7. 验证：`cargo test --locked --all-features --workspace --lib --tests --examples`、clippy/fmt/doc 全绿；dev-notes 知识更新
