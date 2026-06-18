## Why

TRNovel 的 UI 跑在自研框架 `ratatui-kit` 上,当前钉死 **0.6.0**。0.7.0(2026-06-17 发布)带 4 项 breaking:① 重写事件系统为「输入层栈 + 中央分发器」,实现框架级输入互斥;② 全局 store 改 Jotai 式 `Atom`;③ `element!` 去 sigil(`#()`→`{}`、`$expr`→`widget()`/`stateful()`);④ 内置组件体系重写。

两条直接收益:**(a)** 新事件系统从根上修掉本项目 `search_input.rs` 注释里吐槽的「分发中途列表看到 `is_inputting=false` → 误触选中→进入阅读/关列表」**跨帧竞态**——旧的广播订阅 + 全局 `is_inputting` 手动门控本质上无法消除这个 race;**(b)** 0.7 的内置组件(`SearchInput`/`Select`/`MultiSelect`/`TreeSelect`/`VirtualList`/`ConfirmModal`/`AlertModal`)本就**吸取自 TRNovel 自己的组件**,是这些自定义实现的上游演进版。继续钉 0.6 = 拿不到这些修复、与上游持续漂移、且 22 个文件背着一套手写输入门控。

本 change 只做与「升级 + 事件系统 + 组件」**强耦合**(bump 一动即编译不过)的部分。全局 store→`Atom` 解耦给后续 change `global-state-to-atom`。

## What Changes

- **依赖 bump**:`ratatui-kit` 0.6.0 → 0.7.0(`ratatui` 维持 0.30.1,已是最新)。一改即强制下列全部同步,否则编译不过。
- **`element!` 去 sigil**(纯语法,零行为变化):9 个文件的 `#( … )` → `{ … }`;25 处 `$expr` → `widget(expr)`、2 处 `$(w, s)` → `stateful(w, s)`。坑:`$Line::from(..).style().centered()` 这类链式须把**整条链**包进 `widget(...)`。
- **BREAKING 事件系统迁移**:21 个文件 `hooks.use_events(|e| {…})` → `hooks.use_event_handler(EventScope, EventPriority, |e| {…→ EventResult})`,每个 handler 显式返回 `Consumed`/`Ignored`。约定:背景 shell 键 `q`/`g`/`b`(`layout.rs`)与页面/Outlet 子树一律 `EventScope::Current`(root 层)+`Normal`,**随活跃输入层自动让位**;输入框/独占模态 → `use_input_layer(open, blocks_lower=true)` 开层独占 + 层内 handler `High` + `Consumed`,自动截断背景键(取代 `is_inputting` 手动门控)。`EventScope::Global` 仅留给 Resize 等必达事件——`q`/`g`/`b` **不设 Global**,否则会劫持文本输入(编辑时按 `q` 会退出)。
- **BREAKING 删除全局 `is_inputting`**:7 处 `use_context::<State<bool>>` 的「输入互斥门控」用法由输入层自动截断取代;残留的「视觉态」用法(`is_editing`/`is_scroll`)下沉为各页 `props`/局部 `use_state`。删除即消除跨帧误触竞态。
- **组件改走「薄主题适配层」**:7 个自定义组件(`confirm`/`warning`/`search_input`/`select`/`file_select`/`list_select`/`multi_list_select`)保留**项目组件名**作 wrapper,内部读 `UseThemeConfig` 把 `theme.*` 映射成内置所需的 `Style`/`Color` props 后委托 0.7 内置;内置缺失的能力(`Select`/`MultiSelect` 无 loading、无滚动条、非虚拟化;`TreeSelect` 对目录也触发 `on_select`、无空态)在 wrapper 内补回。调用点(≈30 处)基本不动,主题映射每组件只写一次。
- **`shortcut_info_modal` 保留自定义**:与内置 `ShortcutInfoModal` 仅 40% 对齐(数据模型扁平 tuple vs 分 section、关闭回调模型不同、9 个调用点),全量重写不划算;只迁其内部 `use_events`。
- **向后兼容**:对终端用户**零可见行为变化**(键位、模态、选择、阅读流程逐项保持),唯一可观察差异是输入互斥跨帧竞态被修复(严格改善)。全局 store 架构不在本 change(仍是现有 7 层 `ContextProvider` 链,只是少一层 `is_inputting`)。

## Capabilities

### New Capabilities
- `tui-input-dispatch`: 终端键盘/鼠标输入的**分层分发与互斥**契约——基于输入层栈 + 中央分发器:模态/输入框开 `blocks_lower` 层独占输入、背景 handler 自动截断;`Consumed`/`Ignored` 决定传播;全局键(退出/前进/后退)真全局可达不被输入层吞;每帧重建层与 handler(关闭的弹窗/卸载的组件下一帧自动退出);同一活页内分发消除跨帧门控竞态。此前由广播订阅 + 全局 `is_inputting` 手动门控承担,无对应 spec。

### Modified Capabilities
<!--
  none:本 change 主体是行为保持的框架迁移。依赖 bump、去 sigil、组件薄主题适配层、shortcut 内部迁移
  对终端用户均无规格级行为变更(键位/模态/选择/阅读流程逐项保持)。唯一有可观察行为变化的是输入分发模型
  (新 capability tui-input-dispatch),且为竞态修复(严格改善)。openspec/specs/ 当前为空,无既有 UI/输入 spec 可改。
-->

## Impact

- **依赖**:根 `Cargo.toml` `ratatui-kit` 0.6.0→0.7.0(`features=["full"]` 不变)。
- **事件系统**(21 文件):`src/app/layout.rs`、`src/pages/**`(home/select_history/read_novel{,/read_content,/select_chapter,/tts/*}/theme_setting{,/select_color}/local_novel/network_novel/**)、`src/components/modal/browser_prompt.rs`。难点:`tts/settings.rs` 与 `tts/voice_select.rs` 同层竞争 `h`/`l` 键,迁移后须「仅 focused 子组件 `Consumed`」,否则重复响应。
- **`is_inputting` 删除**:`src/app/mod.rs`(去掉该 `use_state` 与对应 `ContextProvider` 层)、7 处 `use_context` 调用点。
- **组件薄主题适配层**:`src/components/{search_input,select,list_select,multi_list_select,file_select}.rs` 与 `src/components/modal/{warning,confirm}.rs` 重写为内置 wrapper;`shortcut_info_modal.rs` 仅迁内部事件。`list_select`/`multi_list_select` 须在 wrapper 保留虚拟化 + loading(内置 `Select`/`MultiSelect` 无);`file_select` 须包 `on_select` 过滤目录 + 空态;调用点(≈30 处)主要受 prop 改名影响(`is_editing`→`active`、`default_value`→`default_index`、`empty_message` 类型 `String`→`TextParagraph`)。
- **去 sigil**(9 + 8 文件):`src/app/mod.rs`、`src/components/{loading,select}.rs`、`src/components/modal/shortcut_info_modal.rs`、`src/pages/**`(home/read_novel{/mod,/read_content,/select_chapter,/tts/{download,settings,voice_select}}/theme_setting/network_novel/{book_detail,book_source_login})。
- **验证**:CI 四件套(test/clippy/fmt/doc)+ **额外 `cargo run` 跑通首帧**(框架迁移须真跑,单测覆盖不到 UI)。
- **不影响**:`crates/parse-book-source`、`crates/novel-tts`(纯 Rust 后端,不碰 ratatui-kit)。全局 store→Atom 留给 `global-state-to-atom`。
