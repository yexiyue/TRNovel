## 1. 依赖 bump + 去 sigil

- [x] 1.1 根 `Cargo.toml`:`ratatui-kit` `0.6.0` → `0.7.0`(`features=["full"]` 不变)
- [x] 1.2 去控制流 sigil `#( … )` → `{ … }` / 一等控制流(9 文件)
- [x] 1.3 去 widget sigil `$expr` → `widget(expr)`、`$(w,s)` → `stateful(w,s)`(8 文件)
- [x] 1.4 链式包裹核对:`$Line::from(..).style(..)` 等整条链进 `widget(...)`
- [x] 1.5 宏层编译核对

## 2. 组件适配层 — 🟢 易档(仅主题映射)

- [x] 2.1 `components/modal/confirm.rs` → 委托框架 `ConfirmModal`(主题映射,内置自带输入层)
- [x] 2.2 `components/modal/warning.rs` → 委托框架 `AlertModal`(按 is_error 分支映射,保留 exit 语义)
- [x] 2.3 confirm/warning 调用点 props 不破

## 3. 组件适配层 — 🟡 中档

- [x] 3.1 `components/search_input.rs` → 委托框架 `SearchInput`(主题映射 + 透传;is_inputting/竞态由内置解决)
- [x] 3.2 `components/select.rs` → **保留渲染 + 滚动条**,迁内部 use_events→use_event_handler + 去 sigil(行为零变,优于硬塞框架 Select)
- [x] 3.3 `components/file_select.rs` → 保留 TreeSelect + dir 过滤,仅迁事件
- [x] 3.4 调用点核对

## 4. 组件适配层 — 🔴 难档(保留虚拟化 + loading)

- [x] 4.1 `components/list_select.rs` → 保留项目 `ListView`(虚拟化)+ loading,仅迁内部事件
- [x] 4.2 `components/multi_list_select.rs` → 同上,保留虚拟化 + loading + 逐项渲染
- [x] 4.3 调用点核对(决策:保留 ListView 比换 VirtualList 零行为变、零调用点改名)

## 5. 页面事件迁移 + 删 is_inputting

- [x] 5.1 `app/layout.rs`:q/g/b → `EventScope::Current`+Normal,不设 Global,删 is_inputting
- [x] 5.2 簇 home+history
- [x] 5.3 簇 read_novel-core
- [x] 5.4 簇 tts(同层 h/l 保留 is_editing 门控 = focused-only)
- [x] 5.5 簇 theme+local
- [x] 5.6 簇 network(book_source_login 表单页用 `use_input_layer(true,true)` 独占层)
- [x] 5.7 `components/modal/browser_prompt.rs`
- [x] 5.8 删全局 `is_inputting`:app/mod.rs 去 provider + use_state;清 7 处 use_context(含遗漏的 import_book_source)

## 6. shortcut 模态内部迁移

- [x] 6.1 `components/modal/shortcut_info_modal.rs`:保留自定义,仅去 sigil(无 use_events)

## 7. 验证

- [x] 7.1 `cargo clippy --all-targets --all-features --workspace -- -D warnings` 通过(0 问题)
- [x] 7.2 `cargo fmt --all --check` / `cargo test`(全绿) / `cargo doc -D warnings` 通过;`cargo build` 主程序链接正常
- [ ] 7.3 (待人工)`cargo run` 首帧 + 走查关键流(本环境无法驱动全屏 TUI)
- [ ] 7.4 (待人工)逐条手验 spec `tui-input-dispatch` 5 个 scenarios
- [x] 7.5 gotcha 已记入 `dev-notes/knowledge/tui-ratatui-kit.md` + `toolchain.md`

## 8. 实施中发现并完成的额外基础设施(tasks 原未覆盖)

- [x] 8.1 `components/list_view.rs`(唯一手写 Component):`SendBlock` → `Option<Block<'static>>`(0.7 砍 SendBlock + Send/Sync bounds)
- [x] 8.2 自定义 hook `use_init_state.rs` / `use_debounce_effect.rs`:deps 约束 `Hash` → `PartialEq + Unpin + 'static`(0.7 effect deps 改按相等比较)
- [x] 8.3 `ExploreListItem` 补 `PartialEq`(作 effect deps);read_content deps 去 `&x.clone()` 临时值借用
- [x] 8.4 `tui-big-text` 0.8.4→**0.8.7**(补 `&BigText: Widget`,去掉 `widget(big_txt)` 的自写适配层)
- [x] 8.5 死依赖清理:删 trnovel 的 `tui-scrollview`/`rodio`/`tokio-util` + novel-tts 的 `futures`(`ort` 是版本钉死,豁免)
