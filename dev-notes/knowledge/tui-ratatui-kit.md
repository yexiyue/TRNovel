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

## 键位

### 没有中央 keymap，每个页面自己 match KeyCode

全项目没有统一键位表;每个页面/组件在自己的 `use_events` 闭包里 match `KeyCode`（vim 风格 j/k/h/l、Tab、Enter）。`is_inputting` context 在 `SearchInput` 聚焦时 gate 页面快捷键,页面必须检查它避免双重处理。快捷键帮助浮层在 `src/components/modal/shortcut_info_modal.rs`。

**相关文件**：`src/app/layout.rs`（少数 app 级键）、各 page 的 `use_events`

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
- **模态/选择/搜索内置组件**:框架 0.7 的 `ConfirmModal`/`AlertModal`/`SearchInput` 自带独占输入层,项目自定义同名组件改「薄主题适配层」（保留项目名作 wrapper,内读 `UseThemeConfig` 映射 `theme.*`→Style 后委托内置）;`Select`/`ListView`/`TreeSelect`/`MultiSelect` 的滚动条/loading/虚拟化内置缺,保留项目渲染只迁内部事件。

### 全局 store → Atom（change `global-state-to-atom`）

ambient 单例(主题 / TTS 模型句柄 / 浏览器提示)从「`App` `use_state` + 深嵌套 `ContextProvider` 链 + 后代 `use_context`」改为 module-level `static Atom`:

- 声明:`pub static THEME: Atom<ThemeConfig> = Atom::new(ThemeConfig::default);`(`Atom::new(fn() -> T)` 是 **const fn**,可作 static;无捕获闭包 `|| None` 也行)。`Atom<T>` 要 `T: Send+Sync`,`use_atom` 另要 `Unpin`。
- 组件内订阅:`hooks.use_atom(&THEME)` 返回 `AtomState<T>`(Copy 句柄,API 同 `State`)。
- **组件外/后端直接读写**:`THEME.set(v)` / `THEME.get()`(`Atom::set/get` 取 `&self`,无需 hooks)——这把 `browser_assist` 那套「OnceLock 持 UI State 句柄给 build_engine」的桥**整个删掉**:`BROWSER_PROMPT` 是 static 全局可达,`TuiBrowserUi` 退化成无状态单元结构体直接读写它。
- **坑:`use_atom` 是 `&mut self`**(注册 waker 的 hook)。把它包进 `&self` 的辅助方法(如 `UseThemeConfig::use_theme_config`)会强制该方法变 `&mut self`,**波及所有非 mut hooks 的调用点**(`fn Foo(.., hooks: Hooks)` → `mut hooks`)。本次波及 confirm/search_input/shortcut_info_modal/SettingItem 4 处。
- **不 atom 化带 Drop 存档的缓存**:`History`/`BookSourceCache`/`TTSConfig` 有 `impl Drop { save() }`,而 `static` 析构永不运行 → 仍由 `App` `use_state` 持有(provider 链从 6 缩到 3)。
- `BrowserPrompt::Click` 的 `Arc<AtomicBool>` 取消信号:atom 替换写入会 drop 旧 `Click`,但引擎侧持有 `Arc` 克隆保活,`cancel.load()` 不悬挂。

**相关 change**:`openspec/changes/upgrade-ratatui-kit-07`、`global-state-to-atom`（均已实施 + CI 全绿,design.md 有完整决策）。
