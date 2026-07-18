# TUI / ratatui-kit

## 概览

主程序 UI 基于 `ratatui-kit`（一个 React-like 的 ratatui 封装,外部依赖、已升 0.30）。覆盖 hooks 的求值时机坑、键位约定、路由。`src/pages`、`src/components`、`src/hooks`。

## Hooks 求值时机

### 别在 use_effect_state / use_future 的参数 block 同步部分 spawn（狂闪 bug）

`use_effect_state` / `use_future` 的**第一个参数是 future（已构造好的 future 表达式）**,而函数参数在 Rust 里是 **eager 求值**——组件**每次渲染都会重新求值这个 block**。若在 block 的**同步部分**（`async move {` 之前）执行 `tokio::spawn(...)` 等副作用,会**每帧执行一次**,绕过 hook 内部的 deps/once 控制。对 render 类副作用（开浏览器取页）表现为**系统栏狂闪 + 反复开关浏览器**;reqwest 类快副作用（ms 级）则不易察觉但也在浪费。

**正确做法**：
- 副作用（`tokio::spawn` / 取页 / await）一律放进 `async move {}` body **内部**——只有 future 被 await（受 deps 控制）时才执行。
- block 的同步部分只用来**捕获值**（`let page = page.get();` `let engine = props.engine.read().clone();`）。

**不要做**：
```rust
hooks.use_effect_state(
    {
        let future = engine.map(|e| tokio::spawn(async move { e.explore(...).await })); // ✗ 每帧 spawn!
        async move { future.unwrap().await? }
    }, deps);
```
应改为：
```rust
hooks.use_effect_state(
    {
        let engine = props.engine.read().clone();   // 只捕获
        let page = page.get();
        async move { engine.unwrap().explore(&url, page, sz).await }  // ✓ await 在 body 内,受 deps 控制
    }, deps);
```

**对比安全的几种**（无需改）：
- `use_memo(move || {...spawn...}, deps)` —— 参数是 **closure（lazy）**,use_memo 只在 deps 变时调它,不是每帧。
- `async move { tokio::spawn(...).await? }` —— spawn 在 async body 内,随 future 执行。
- 事件回调 `use_events(move |ev| {...spawn...})` —— 按键时才触发。

**相关文件**：`src/pages/network_novel/select_books/find_book.rs`（修复 commit `6ff4999`）、`src/hooks/use_init_state.rs`（use_effect_state 实现:内部用 `use_async_effect(async{ init_f.await }, deps)`,200ms loading 防抖）

## State 读写（generational-box RwLock）

### 读 guard 存活期间写同一个 State = 死锁（不是 panic），表现为 TUI「卡死」

`State<T> = ReactiveHandle<T>`,底层是 `generational-box` 的 **`SyncStorage`（parking_lot `RwLock`）**。`state.read()` 返回持有读锁的 guard;`state.write()` 内部走 generational-box 的 `try_write`——但**该 `try_write` 名不副实、在 SyncStorage 上是阻塞的**（`sync.rs:302` → `Self::write` → `get_split_mut` → `sync.rs:121` 直接 `RwLock::write()`,无 try_）。它**只在「值已 drop/失效」时返回 Err,从不在「已被借用」时返回 Err**。因此 `reactive_handle.rs` 里 `write()` 末尾的 `.expect("...already borrowed")` **永远不会触发**——同线程「读 guard 未释放 + 写同一 State」不是 panic,而是 **parking_lot RwLock 永久阻塞**。渲染主循环 `dispatch` 是同步调用（`render/tree.rs`),一旦在事件 handler 里死锁,**整个 TUI 卡死**。（真正非阻塞、借用冲突返 Err 的路径只在 `UnsyncStorage`/RefCell,框架用的是 SyncStorage,用不到。）

**最隐蔽的触发形态：`read()` 临时量作为回调实参**。Rust 临时量作用域规则:`callback(state.read().iter()....collect())` 里 `state.read()` 的 guard **存活到整条语句结束（分号处）**,即在 `callback(...)` 执行期间读锁仍持有;若该 `callback` 内部写同一个 `state` → 死锁。`.collect()` 完成**不会**提前释放它。同理 `if let Some(x) = state.read().f { … state.write() … }` / `match state.read().x { … }`——scrutinee 的读 guard 存活到整个块结束。

**正确做法（先收集释放读锁,再调回调）**:
```rust
// ✓ 两条语句:内层块结束即 drop 读 guard,on_select 执行时不持任何 guard
let items: Vec<T> = { let g = state.read(); g.iter().map(|&i| data[i].clone()).collect() };
on_select(items);
```
框架内置 `MultiSelect` 正是这么写的（`let chosen = selected_items(&items, &selected.read()); on_select(chosen);`）——可直接对照。

**不要做**:
```rust
// ✗ 读 guard 跨 on_select 存活;若 on_select 内 state.write() 同一 State → 死锁卡死
on_select(state.read().iter().map(|&i| data[i].clone()).collect());
```

**判别**:不同 State「读一个写另一个」安全（如读 `state` 写 `selected`);只有「同一个 State 读 guard 存活期间写它自己」才死锁。排查 TUI 按某键后卡死(非崩溃/无 panic 输出)时,优先怀疑此类自借用。全仓扫描确认（2026-07）当前仅 `MultiListSelect` 一处曾中招,已修。

**相关文件**：`src/components/multi_list_select.rs`（Enter 分支修复:收集释放读锁再调 on_select）、`src/pages/network_novel/book_source_manager/import_book_source.rs`（on_select 内 `selected.write().clear()`）、`generational-box` `sync.rs`（`try_write`→阻塞 `write`）、`ratatui-kit` `reactive_handle.rs`（`write().expect()` 永不触发）

### 子组件依赖父级 async 初始化的资源时，要把父级 loading 透传下去，否则错显空态

页面用 `use_init_state` 异步构建引擎/资源（`build_engine`+`warmup`+`explore_entries`,render 源可达数秒）,期间该资源为 `None`。若子组件（`FindBooks`）在资源为 None 时「立即返回空列表、loading=false」,列表区会渲染**空态文案**（「暂无书籍」）,让用户误以为**书源不可用**。父级若把 `use_init_state` 的 loading 标志丢弃（`let (engine, _, error) = …`）,这段就完全无加载提示。

**正确做法**:父级保留 init loading 并透传进子组件,子组件 `loading: own_loading || parent_init_loading`。`use_init_state` 的 loading 已带 200ms 防抖,不会闪。

**相关文件**：`src/pages/network_novel/select_books/mod.rs`（透传 `engine_loading`）、`src/pages/network_novel/select_books/find_book.rs`（`FindBooksProps.engine_loading` + `loading: loading.get() || props.engine_loading`）

### 列表组件 Enter 取项用 `data.get(i)` 而非裸 `data[i]`：强制初始选中 + 空列表 = 越界 panic 崩溃

`ListSelect`/`MultiListSelect` 等列表组件为让快捷键即时可用,常给 `ListState.selected` 一个**强制初始值 `Some(0)`**(如 `SelectBookSource`、`SelectChapter`)。但列表**为空时**该选中仍是 `Some(0)`,Enter handler 若写裸 `data[selected]` → `index out of bounds: len is 0 but index is 0` → **整个 TUI panic 崩溃退出**(VHS 端到端实测:无书源时按 Tab 进只选模式再 Enter,或把书源删到空后 Enter,都会崩)。`FileSelect` 则相反——**初始无选中**(`None`),不先按 j/k 落光标,Enter 静默无效。

**正确做法**:
- Enter 取项一律走 `.get()`:`if let Some(item) = data.get(path) { on_select(item.clone()) }`;多选 `filter_map(|&i| data.get(i).cloned())`。
- 「强制 Some(0) 让快捷键即时可用」与「空列表」必须同时考虑:给了初始选中就要在取用处防越界。

**相关文件**：`src/components/list_select.rs`、`src/components/multi_list_select.rs`

### 传了 `state` 的内置组件，也要传它的 `default_*` prop —— 否则挂载帧被空值清空

`TreeSelect` 传了 `state` 却不传 `default_selection`(默认空 `Vec`)时,选中会在**挂载帧被清空**:它内部 `use_effect` 调 `sync_default_tree_selection`,首次同步 `last_default=None != Some(&[])` → 判定「default 变了」→ `select(vec![])`,而**空路径 = 清除选中**。组件树父先子后更新,故这次清除总在父组件 seeding 之后,表现为「`use_state` 里明明 `st.select(...)` 了,挂载后却没有任何高亮」。`open_ancestors(&[])` 是空循环,所以**卷是展开的、偏偏没高亮** —— 这个不对称是该 bug 的特征指纹。

**正确做法**:把「定位」交给组件的 `default_selection`,别在 `use_state` 里手动 select。它的 `use_effect` deps 就是该 prop,**值变化时会重新定位** —— 这顺带解决了异步加载的时序:章节列表 await 完成后 `current_chapter` 由 0 变真实章号,路径随之变化触发重新选中。`use_state` 初始化闭包只跑一次,做不到这点(这正是「首次进入停在第 0 章」的成因)。

**不要做**:`use_state(|| { let mut st = TreeState::default(); st.select(path); st })` —— 既会被子组件清空,又对后到的 props 失聪。

**判别**:选中丢失但节点展开正常 → 查内置组件是否有未传的 `default_*` prop。**排查时别只截屏幕前几行**:widget 只在选中项落在可见窗口外才滚动(`tui-tree-widget` 的 `while ensure_index_in_view >= end`),索引小于一屏高度时高亮就在原位,截前 8 行会误判成「没选中」。

**框架侧已修**(`../ratatui-kit`,0.10.2 之后):`sync_default_tree_selection` 现在跳过「首次同步 + 空 default」,不再误清调用方预设的选中;「从非空显式改回空 = 清除选中」的语义保留。TRNovel 仍应显式传 `default_selection`(它要的就是定位)。

**相关文件**:`src/pages/read_novel/select_chapter.rs`、`ratatui-kit` `components/tree_select.rs`(`sync_default_tree_selection`)

## 键位

### shell 键 q/g/b 是 Low 优先级 —— 页面占用同名键会让它在该页彻底失效

`layout.rs` 的 shell 键注册为 `EventScope::Current + EventPriority::Low`。同层分发按 **priority 降序**(`High→Normal→Low`,`input/mod.rs` 的 `handlers[b].priority.cmp(&handlers[a].priority)`)且 `Consumed` 早停,故**页面级 `Normal` handler 先跑**。页面一旦占用同一个键并 `Consumed`,该 shell 键在那个页面就**彻底收不到事件**。书源页曾用 `b` 切换浏览器辅助验证,把后退键吃掉了 —— 现已改用 `w`。

**新增页面快捷键时避开 `q`/`g`/`b`**。调换 priority 不是修法:把 shell 改成 `High` 会反向压死所有页面对这些键的合法覆盖。

**相关文件**:`src/app/layout.rs`、`src/pages/network_novel/book_source_manager/mod.rs`

### CLI 子命令导航要一次性,否则后退键被 effect 弹回

`Layout` 用 `use_effect(.., params)` 按 CLI 子命令(`-n`/`-H`/`-l`)首屏导航。deps 是 `params`,而 `/home` 的 route state 就是 `TRNovel` 本身,故按 `b` 从目标页退回 `/home` 会让 params 再次变化 → effect 重跑 → **把用户 push 回原页**,现象与「后退键无效」一模一样。用 `use_state(|| false)` 的一次性标志挡住重复导航。

**判别**:这个坑会与键位冲突**叠加**并互相掩盖 —— 修好键位后 `b` 才第一次真正执行 `go(-1)`,这个 bug 才暴露。验证后退键时用**不带 CLI 参数**启动(`subcommand=None`,effect 不导航)手动进页面,能干净地把两者分开。

**相关文件**:`src/app/layout.rs`

### 键位分层:阅读页走 keymap(可配置),其余页面仍 match KeyCode

阅读页(`read_novel` 子树)已迁移到 `ratatui-kit-keymap` 的语义 action 分发,键位可经 `~/.novel/keybindings.toml` 的 `[reader]` 表自定义(issue #49);其余页面/组件仍在各自 `use_event_handler` 里 match `KeyCode`,后续变更逐 scope 迁移。快捷键帮助浮层在 `src/components/modal/shortcut_info_modal.rs`。

**正确做法**:
- 阅读页新增快捷键:在 `src/keymap/mod.rs` 的 `ReaderAction` 加变体 + `reader_defaults()` 绑默认键(变体名 snake_case 即用户配置键名,是稳定契约,改名 = 破坏用户配置);事件侧在对应组件的 `use_keymap_handler` 回调里加分支。
- 键位表经 `KEYMAP: Atom<AppKeymap>` 分发,`hooks.use_atom(&KEYMAP).read().reader.clone()` 每帧取 `Arc`(引用计数,非深拷贝);hook 用 `use_keymap_handler(scope, priority, arc, |action, _key| ...)`,未命中自动 `Ignored` 不拦截 shell 键。
- 帮助浮层/底部提示的键名一律走 `keymap::display_keys` / `display_first_key`(显示层折叠 `Shift-单字母` 为大写、方向键转箭头),保证显示与实际绑定一致;组件内部自处理的键(TreeSelect 导航、TTS 面板内 h/l)保持硬编码。
- 迁移前 `'i' | 'I'` 这类大小写双匹配 → 默认表绑 `["i", "I"]`(crate 把大写字母视为 shift 意图,"I" ≡ shift-i);只绑小写会让 Shift+字母 失效。
- 配置加载在 `App` 启动 init 内(`crate::keymap::load_keymap`),任何问题降级为中文告警接非阻断 `WarningModal`(ESC 关闭),**不得**进入致命 error 路径。

**不要做**:
- 不要在阅读页组件里重新 match 物理 `KeyCode`——会绕过用户自定义。
- 不要把页面级 action(ToggleReadMode 等)在 `ReadContent` 里消费:它 `Ignored` 交给 `mod.rs` 的 handler,两处 action 集合不相交。

**相关文件**:`src/keymap/mod.rs`(+tests)、`src/state.rs`(`KEYMAP`)、`src/app/mod.rs`(加载+告警)、`src/pages/read_novel/{mod.rs,read_content.rs}`、`openspec/changes/configurable-keybindings/keybindings.example.toml`(示例配置)、contrib 仓库 `crates/ratatui-kit-keymap`

### 阅读页章末/章首「再按一次」防误触跳章 + 翻回位置恢复

阅读正文滚到章末(`current_line == line_count`)再按 ↓,旧行为**直接翻下一章**(网络小说还要重拉正文、丢失位置),读者读到最后一行没看完手滑就跳章。现改为**边界二次确认**:到章末/章首的**首次** ↓/↑ 只「武装」并在底部状态栏居中提示(`● 已到本章末尾 · 再按 ↓ 进入下一章` / `● 已到本章开头 · 再按 ↑ 返回上一章`,accent+bold),**连续第二次**才真正翻章;任何滚动/翻页/显式 ←→ 翻章键都解除武装。用一个 `enum Edge { None, Prev, Next }` 的 `use_state` 管理,`edge.get()` 是 Copy(读 guard 即时释放,不触发前述死锁)。显式 `→/L`、`←/H` 保持**立即翻章**(不走二次确认),给想快速跳的用户留快路。

配套:`on_prev(is_scroll_top)` 之前 `is_scroll_top=true` 分支直接 return(顶部 ↑ 根本不翻上一章),现启用并按来源恢复位置——**顶部 ↑ 翻回上一章落到章末(`line_percent=1.0`)**(承接向上连读、误触后原路找回位置),显式 `←/H` 落到章首(`0.0`)。

**全书边界必须与章内边界分开处理**:`ReadContent` 只知道「章内滚到底/顶」,不知道「全书还有没有下一章」——那信息只在 `ReadNovel` 里。最初漏传,导致最后一章章末仍提示「再按 ↓ 进入下一章」,而第二次按下时 `on_next` 的 `if new_chapter >= chapters.len() { return; }` 静默 no-op:**承诺了不存在的章节 + 提示闪掉 + 零反馈**。现由 `has_prev`/`has_next` props 下传,`Edge` 增加 `AtFirst`/`AtLast` 两态:全书边界**只提示不武装**(「● 已是全书最后一章」/「● 已是第一章」),再按也不翻章、提示保持。

**同族坑 —— TTS 在最后一章的「假播放中」**:自动播放靠 `is_listening_done` 触发 `on_next`。最后一章 `on_next` 静默 no-op → `props.content` 不变 → 以 `content` 为 deps 的清理 effect **不重跑** → `is_listening` 永停 `true`,底部永久显示「播放中」且 `p` 只在 pause/play 间空转、无法重播。故最后一章须显式 `is_listening.set(false)`。**通用教训**:凡「靠 props 变化驱动状态复位」的 effect,在「操作被静默 no-op」的边界都会失效,得手动复位。

**相关文件**：`src/pages/read_novel/read_content.rs`（`Edge` 状态 + `has_prev`/`has_next` + Up/Down 边界逻辑 + 底部提示 + TTS 复位）、`src/pages/read_novel/mod.rs`（`on_prev` 按 `is_scroll_top` 恢复位置、计算并下传 `has_prev`/`has_next`）

## 0.6 → 0.7 迁移实战（change `upgrade-ratatui-kit-07`）

### 事件系统:use_events → use_event_handler（输入层 + 中央分发器）

`hooks.use_events(|e| {…})`（广播订阅）→ `hooks.use_event_handler(EventScope, EventPriority, |e| -> EventResult)`。每个 handler 显式返回 `Consumed`/`Ignored`。约定:

- 背景 shell 键 `q`/`g`/`b`（`layout.rs`）与页面/Outlet 子树一律 `EventScope::Current`（root 层）+ `Normal`,非自身键 `Ignored`。
- **`q`/`g`/`b` 绝不设 `EventScope::Global`**:Global phase 先于一切且不受 `blocks_lower` 截断,会**劫持文本输入**（编辑搜索框按 q 直接退出 App）。留 Current,由活跃输入层的 blocks_lower 自动抑制——这正是旧 `is_inputting` 手做的事,现由层栈零竞态完成。
- 输入框/独占模态 → `use_input_layer(open, blocks_lower=true)` 开层独占,层内 handler `High` + `Consumed`。
- 全屏独占表单页（如 `book_source_login`）= `let layer = hooks.use_input_layer(true, true);` + handler 用 `EventScope::Layer(layer)`,**取代旧的「进页设 is_inputting=true / 离页设 false」**;离页卸载层自动消失。
- 同层多个可聚焦子组件抢同一键（如 tts settings/voice 的 h/l）:保留各自 `is_editing` 门控（`if !is_editing { return Ignored }`），仅 focused 者 Consumed,否则一次按键被多个子项重复响应。

**删全局 `is_inputting`**:门控用法 `&& !is_inputting.get()` 整段删（交输入层）;视觉态用法 `is_editing: !is_inputting.get() && X` → `is_editing: X`,下沉为页面局部 state/props。

### element! 去 sigil 的真坑:`{ }` 紧跟自闭合组件会被当成它的 children

`#(expr)` → `{ expr }`、`$expr` → `widget(expr)`（整条链进去:`widget(Line::from(..).style(..).centered())`）、`$(w,s)` → `stateful(w,s)`。

**最隐蔽的坑**:`Component(props) { … }` 里的 `{ }` 是该组件的 **children 块**。若把 `#(if …)` 机械替换成 `{ if … }` 且它紧跟一个**自闭合组件**（`SearchInput(…)` / `Select(…)`,props 在括号、无 children），宏会把这个 `{ if … }` 当成那个组件的 children 解析,里面的 `element!(…)` 触发 `error: expected identifier`。

**正解:用一等控制流**（0.7 新增,无需 `{ }` 包裹）作兄弟子节点,分支体直接写**原生 element 子节点**,去掉内层 `element!(…)` 与 `.into_any()`:
```rust
element!(View {
    SearchInput(…)
    if is_empty {
        Border(…) { Center(…) { Text(…) } }   // 原生子节点,不是 element!(Border(…))
    } else {
        TreeSelect<TocId>(…)
    }
})
```
`{ expr }` embed 仅用于注入预先算好的值/变量,且别紧贴自闭合组件。

children 透传:`{ &mut props.children }` 会因生命周期 `'1 must outlive 'static` 失败 → 用 `{ std::mem::take(&mut props.children) }`（取所有权,owned `Vec<AnyElement<'static>>`）。

### 自定义 Hook / Component / 第三方 widget 的 0.7 适配

- **deps 约束变了**:`use_effect`/`use_async_effect`/`use_effect_state` 的 deps 从 `D: Hash` 改为 **`D: PartialEq + Unpin + 'static`**（按相等比较 + 跨帧存储）。自定义包装 hook（`use_init_state`/`use_debounce_effect`）与做 deps 的类型（如 `ExploreListItem`）都要补 `PartialEq`。deps 里别写 `&x.clone()`（临时值借用),直接传 owned。
- **`SendBlock` 已移除**:0.7 `Component: Any + Unpin`（砍了 Send+Sync bounds）。手写 Component 里 `pub block: SendBlock` → `pub block: Option<Block<'static>>`,调用点 `block: some_block` 由宏自动 `Some`。
- **`widget(expr)` 要 `for<'a> &'a T: Widget`**（按引用渲染 + Clone + Unpin）。只实现按值 `Widget` 的第三方 widget 会报 `&T: Widget not satisfied`。**首选升级该 widget crate**:`tui-big-text` 0.8.4→0.8.7 即补了 `&BigText: Widget`,`widget(big_txt)` 直接可用,无需自写 `impl Widget for &Wrapper` 适配层。
- **模态/选择/搜索内置组件**:框架 0.7 的 `ConfirmModal`/`AlertModal`/`SearchInput` 自带独占输入层。当时项目自定义同名组件改成「薄主题适配层」;0.10 主题重构后不再用旧 `UseThemeConfig`,应改读内置组件主题或项目 `ComponentTheme`。`Select`/`ListView`/`TreeSelect`/`MultiSelect` 的滚动条/loading/虚拟化内置缺,保留项目渲染只迁内部事件。

### 全局 store → Atom（change `global-state-to-atom`）

ambient 单例(主题 / TTS 模型句柄 / 浏览器提示)从「`App` `use_state` + 深嵌套 `ContextProvider` 链 + 后代 `use_context`」改为 module-level `static Atom`:

- 声明方式:`pub static FOO: Atom<FooConfig> = Atom::new(FooConfig::default);`(`Atom::new(fn() -> T)` 是 **const fn**,可作 static;无捕获闭包 `|| None` 也行)。`Atom<T>` 要 `T: Send+Sync`,`use_atom` 另要 `Unpin`。旧版示例里的 `THEME: Atom<ThemeConfig>` 已被 0.10 主题重构替换为 `APPEARANCE` / `READER_DISPLAY`。
- 组件内订阅:`hooks.use_atom(&THEME)` 返回 `AtomState<T>`(Copy 句柄,API 同 `State`)。
- **组件外/后端直接读写**:`THEME.set(v)` / `THEME.get()`(`Atom::set/get` 取 `&self`,无需 hooks)——这把 `browser_assist` 那套「OnceLock 持 UI State 句柄给 build_engine」的桥**整个删掉**:`BROWSER_PROMPT` 是 static 全局可达,`TuiBrowserUi` 退化成无状态单元结构体直接读写它。
- **坑:`use_atom` 是 `&mut self`**(注册 waker 的 hook)。把它包进 `&self` 的辅助方法会强制该方法变 `&mut self`,**波及所有非 mut hooks 的调用点**(`fn Foo(.., hooks: Hooks)` → `mut hooks`)。旧 `UseThemeConfig::use_theme_config` 就踩过这个坑,当前主题代码不再保留该 helper。
- **不 atom 化带 Drop 存档的缓存**:`History`/`BookSourceCache`/`TTSConfig` 有 `impl Drop { save() }`,而 `static` 析构永不运行 → 仍由 `App` `use_state` 持有(provider 链从 6 缩到 3)。
- `BrowserPrompt::Click` 的 `Arc<AtomicBool>` 取消信号:atom 替换写入会 drop 旧 `Click`,但引擎侧持有 `Arc` 克隆保活,`cancel.load()` 不悬挂。

**相关 change**:`openspec/changes/upgrade-ratatui-kit-07`、`global-state-to-atom`（均已实施 + CI 全绿,design.md 有完整决策）。

## 0.7.1 → 0.10.1 迁移实战

### ScrollView 与主题化组件 prop 变化

`ratatui-kit` 0.10.1 引入主题化组件 API 后,若干 props 与类型名相对 0.7.1 有破坏性变化。

**正确做法**：
- `ScrollView` 滚动条配置改为 `scrollbars: Scrollbars { ... }`；旧的 `scroll_bars: ScrollBars { ... }` 不再存在。
- `ScrollView` 是否响应内置键鼠滚动用 `active: bool`；旧的 `disabled` 字段不再存在,语义要反过来写成 `active: is_editing`。
- `Border` / `Text` 等主题化组件的 `style` / `border_style` 是 `Option<Style>` 覆盖主题,不能再传裸 `Color`。空态文本优先用项目语义槽 `theme.empty` 这类 `Style`。

**不要做**：
- 不要把裸 `Color` 直接传给 `Text(style: ...)`；需要用 `Style::new().fg(...)`。当前主题重构后,优先使用 `AppChromeTheme` / `ReaderTheme` 这类项目 `ComponentTheme` 的语义槽。

**相关文件**：`Cargo.toml`、`src/pages/network_novel/book_detail.rs`、`src/pages/read_novel/tts/mod.rs`、`src/components/select.rs`

### 0.10 主题系统接入（change `refactor-theme-system`）

`ratatui-kit` 0.10 的主题系统以 `PaletteProvider` 为根，组件通过内置主题、`use_palette()` 或自定义 `ComponentTheme` 被动读取当前 palette。TRNovel 不再维护旧的 `ThemeConfig` 六色派生树，也不再通过 `UseThemeConfig` 把项目样式传给所有组件。

**正确做法**：
- `App` 订阅 `APPEARANCE: Atom<AppearanceConfig>`，每帧从 `theme_slug + BackgroundMode` 派生 `Palette`，并用 `PaletteProvider` 包裹 router/provider 子树和启动错误弹窗。
- `Palette.bg` 只是颜色值，不会自动填满终端背景；根背景层需要显式设置 `Style::new().bg(palette.bg)`。当前 `App` 用 no-border `Border` 包裹 router/provider 子树和启动错误弹窗。当 `BackgroundMode::Terminal` 时，`ratatui-kit-themes::terminal_background` 会把背景转成终端背景，同时保留文本、边框、高亮和语义色。
- 项目级外观槽放在 `src/theme/mod.rs` 的小型 `ComponentTheme` 中：通用 chrome 用 `AppChromeTheme`，阅读正文用 `ReaderTheme`。不要重新创建一个覆盖全项目的大 `AppTheme`。
- 页面或组件需要主题时直接 `hooks.use_component_theme::<AppChromeTheme>()` / `hooks.use_component_theme::<ReaderTheme>()`；只有确实需要原始色板时才用 `hooks.use_palette()`。
- 命名主题来自 `ratatui-kit-themes::ThemeName::all()`；展示用 `display_name()`，持久化用 `slug()`。新配置写入 `~/.novel/appearance.json`，旧 `~/.novel/theme.json` 不读取、不迁移。
- 阅读标题显示是行为偏好，不属于主题。`v` 键切换写 `READER_DISPLAY: Atom<ReaderDisplayConfig>` 与 `~/.novel/reader-display.json`。

**不要做**：
- 不要恢复 `ThemeConfig` / `ThemeColors` / `UseThemeConfig` 兼容层；这会让新旧两套主题系统并存。
- 不要在 list item 等领域渲染结构体里携带旧主题快照。若自定义 `WidgetRef` 需要样式，只携带小型 `ComponentTheme` 或显式 `Style`。
- 不要把 `show_title`、阅读行为或其他偏好塞进 `AppearanceConfig`；外观配置只保存命名主题与背景策略。

**相关文件**：`src/cache/setting.rs`、`src/state.rs`、`src/app/mod.rs`、`src/theme/mod.rs`、`src/pages/theme_setting/mod.rs`
