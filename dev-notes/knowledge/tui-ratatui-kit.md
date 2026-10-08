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

### `use_effect` 的闭包**不要求 `'static`**,`use_async_effect` 要求 —— 决定了 effect 能否借用 props

两个签名只差一处,但直接决定「重活是每帧做还是只在 deps 变时做」:

```rust
fn use_effect<F, D>(&mut self, f: F, deps: D)  where F: FnOnce(),                        // 无 'static
fn use_async_effect<F, D>(&mut self, f: F, deps: D) where F: Future<Output = ()> + 'static; // 有 'static
```

`use_effect` 的 `F: FnOnce()` **没有 `'static` 约束**,所以闭包可以直接**借用 `props`**,把递归收集、遍历这类重活写在闭包体内 —— 闭包每帧构造,但**体内代码只在 deps 变化时跑**。改成 `use_async_effect` 就必须把数据 `clone` 进 future(`'static`),而 future 每帧都要构造,等于把重活变成**每帧一次全量拷贝**。

**正确做法**:纯内存的状态同步(重置选中/展开、校正索引)用 `use_effect` + 借用 props;只有真正的 IO/await 才用 `use_async_effect`。

**别为了「不卡 UI」把同步 effect 异步化**:`use_effect` 是**同步执行**的(就在渲染体内、deps 比较后立即调),这保证「渲染出去的那一帧,派生状态必定与本帧数据自洽」。异步化会插入一个「数据已换、状态还没同步」的中间帧 —— `FileSelect` 里那一帧按 Enter 就会打开一个**已经被筛掉**的文件。

**相关文件**:`src/components/file_select.rs`、`ratatui-kit` `hooks/use_effect.rs`

### `use_memo` 省的是「重新计算」,不是「拷贝」—— 它每帧都 clone 返回值

实现是 `hook.memoized_value.clone().expect(...)`,且约束就写着 `T: Clone`。deps 没变时它跳过的只有闭包体,**返回值照样每帧 clone 一份**。所以拿 `use_memo` 去缓存一棵大树/一大段文本,指望「每帧不再拷贝」是无效的 —— 该拷的一次没少。

**正确做法**:`use_memo` 用来避免**昂贵的重新计算**(排版、解析、聚合);要避免的是**每帧拷贝大结构**,只能靠借用(见上条 `use_effect`)或把数据放进 `State`/`Arc` 传句柄。

**相关文件**:`ratatui-kit` `hooks/use_memo.rs`

### 树/列表的「数据变了要重置选中」:deps 用**指纹**,不要收集全部标识符

`use_effect` 的 deps 每帧都要构造并比较。若写成 `collect_identifiers(&props.items)`(递归收集全树 `Vec<PathBuf>`),就是**每帧一次全树递归 + 堆分配**;而组件渲染本身已经 `clone` 过一次整棵树(`TreeSelect` 的 `items` prop 要 owned),等于把开销直接翻倍。

**正确做法**:用 `DefaultHasher` 递归 hash 出一个 `u64` 指纹当 deps —— **只遍历不分配**,比较也退化成整数相等。hash 时要**带上层级结构**(每层先 hash `items.len()` 再递归),否则同名文件换了目录层级会算出同一指纹而漏掉重置。

**为什么必须重置**:筛选后旧选中路径可能已不在树中,`TreeState` 不会自己校正,Enter 取到的仍是旧路径,`path.is_file()` 对「被筛掉但磁盘上还在」的文件照样为真 → **打开一个当前列表里根本看不见的文件**(issue #64)。反过来,只要这个同步 effect 到位,取项处就**不需要**再遍历树做二次校验(那又是一次每帧全树 clone)。

**配套**:筛选态要连带展开命中目录(`expand_all`)。`tui-tree-widget` 的 `TreeState` **没有 `open_all()`**,只有 `open(Vec<Identifier>)`,且要的是**从根到该节点的完整路径**,只传节点自身的 identifier 展不开嵌套目录。否则命中文件埋在折叠的子目录里,搜了等于没搜。

**相关文件**:`src/components/file_select.rs`、`src/pages/local_novel/mod.rs`

### 页面里的同步阻塞 IO 走 `spawn_blocking`,不是 `tokio::spawn`

`use_effect_state` 只负责「把 future 挂到 deps 上 + 200ms loading 防抖」,future 里跑什么它不管。本地小说扫描是 `walkdir` 递归(**同步阻塞**),原先包在 `tokio::spawn(async move { ... })` 里 —— 它确实不占 UI 渲染线程,但会**占住一个 async worker**;大目录扫描期间,同 runtime 上的网络书源请求、TTS 模型下载会被一起拖慢。

**正确做法**:`tokio::task::spawn_blocking(move || 同步函数())`,`.await?` 处理 `JoinError` 的写法不变。

**相关文件**:`src/pages/local_novel/mod.rs`、`src/hooks/use_init_state.rs`

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
- 阅读页新增快捷键:在 `src/keymap.rs` 的 `ReaderAction` 加变体 + `reader_defaults()` 绑默认键(变体名 snake_case 即用户配置键名,是稳定契约,改名 = 破坏用户配置);事件侧在对应组件的 `use_keymap_handler` 回调里加分支。
- 键位表经 `KEYMAP: Atom<AppKeymap>` 分发,`hooks.use_atom(&KEYMAP).read().reader.clone()` 每帧取 `Arc`(引用计数,非深拷贝);hook 用 `use_keymap_handler(scope, priority, arc, |action, _key| ...)`,未命中自动 `Ignored` 不拦截 shell 键。
- 帮助浮层/底部提示的键名一律走 `keymap::display_keys` / `display_first_key`(显示层折叠 `Shift-单字母` 为大写、方向键转箭头),保证显示与实际绑定一致;组件内部自处理的键(TreeSelect 导航、TTS 面板内 h/l)保持硬编码。
- 迁移前 `'i' | 'I'` 这类大小写双匹配 → 默认表绑 `["i", "I"]`(crate 把大写字母视为 shift 意图,"I" ≡ shift-i);只绑小写会让 Shift+字母 失效。
- 配置加载在 `App` 启动 init 内(`crate::keymap::load_keymap`),任何问题降级为中文告警接非阻断 `WarningModal`(ESC 关闭),**不得**进入致命 error 路径。

**不要做**:
- 不要在阅读页组件里重新 match 物理 `KeyCode`——会绕过用户自定义。
- 不要把页面级 action(ToggleReadMode 等)在 `ReadContent` 里消费:它 `Ignored` 交给 `mod.rs` 的 handler,两处 action 集合不相交。

**相关文件**:`src/keymap.rs`(+tests)、`src/state.rs`(`KEYMAP`)、`src/app.rs`(加载+告警)、`src/pages/read_novel.rs、src/pages/read_novel/read_content.rs`、`openspec/changes/configurable-keybindings/keybindings.example.toml`(示例配置)、contrib 仓库 `crates/ratatui-kit-keymap`

### 阅读页章末/章首「再按一次」防误触跳章 + 翻回位置恢复

阅读正文滚到章末(`current_line == line_count`)再按 ↓,旧行为**直接翻下一章**(网络小说还要重拉正文、丢失位置),读者读到最后一行没看完手滑就跳章。现改为**边界二次确认**:到章末/章首的**首次** ↓/↑ 只「武装」并在底部状态栏居中提示(`● 已到本章末尾 · 再按 ↓ 进入下一章` / `● 已到本章开头 · 再按 ↑ 返回上一章`,accent+bold),**连续第二次**才真正翻章;任何滚动/翻页/显式 ←→ 翻章键都解除武装。用一个 `enum Edge { None, Prev, Next }` 的 `use_state` 管理,`edge.get()` 是 Copy(读 guard 即时释放,不触发前述死锁)。显式 `→/L`、`←/H` 保持**立即翻章**(不走二次确认),给想快速跳的用户留快路。

配套:`on_prev(is_scroll_top)` 之前 `is_scroll_top=true` 分支直接 return(顶部 ↑ 根本不翻上一章),现启用并按来源恢复位置——**顶部 ↑ 翻回上一章落到章末(`line_percent=1.0`)**(承接向上连读、误触后原路找回位置),显式 `←/H` 落到章首(`0.0`)。

**全书边界必须与章内边界分开处理**:`ReadContent` 只知道「章内滚到底/顶」,不知道「全书还有没有下一章」——那信息只在 `ReadNovel` 里。最初漏传,导致最后一章章末仍提示「再按 ↓ 进入下一章」,而第二次按下时 `on_next` 的 `if new_chapter >= chapters.len() { return; }` 静默 no-op:**承诺了不存在的章节 + 提示闪掉 + 零反馈**。现由 `has_prev`/`has_next` props 下传,`Edge` 增加 `AtFirst`/`AtLast` 两态:全书边界**只提示不武装**(「● 已是全书最后一章」/「● 已是第一章」),再按也不翻章、提示保持。

**同族坑 —— TTS 在最后一章的「假播放中」**:自动播放靠 `is_listening_done` 触发 `on_next`。最后一章 `on_next` 静默 no-op → `props.content` 不变 → 以 `content` 为 deps 的清理 effect **不重跑** → `is_listening` 永停 `true`,底部永久显示「播放中」且 `p` 只在 pause/play 间空转、无法重播。故最后一章须显式 `is_listening.set(false)`。**通用教训**:凡「靠 props 变化驱动状态复位」的 effect,在「操作被静默 no-op」的边界都会失效,得手动复位。

**相关文件**：`src/pages/read_novel/read_content.rs`（`Edge` 状态 + `has_prev`/`has_next` + 滚动/翻页边界逻辑 + 底部提示 + TTS 复位）、`src/pages/read_novel.rs`（`on_prev` 按 `is_scroll_top` 恢复位置、计算并下传 `has_prev`/`has_next`）

### 阅读正文的滚动坐标系:进度百分比的分母必须是内容总行数

`line_percent` 会**落盘**（`~/.novel/local|network/*.json` 与历史记录）。旧实现把它定义为 `current_line / (total - view)`，分母含视口高度 → 换终端尺寸/字号/窗口大小后同一个百分比指向不同的行，恢复位置系统性偏移；且一个变量同时兼任滚动上限、进度分母与底部「n/m 行」的分母，末尾想留白就必然连带改坏进度语义。现拆成三个各司其职的量（change `reader-paging-preferences`）：

```
total      = paragraph.line_count(width - 2)     内容总行数,唯一绝对基准
view       = visible_lines(height)               可见行数(height - 3),只此一处定义
end_scroll = total - view                        贴底位置 = 章末判定线
```

**正确做法**：
- 落盘 `line_percent = current_line / total`；恢复 `current_line = round(percent * total)`,**再钳到 `end_scroll`**。
- **钳位必须用 `end_scroll` 而不是 `total - 1`**:比例是与视口无关的,换终端尺寸/重排后 `ratio * total` 可能落在贴底位置**之下** → 屏幕停在「最后一屏再往下」,`current_line < end_scroll` 恒假使前向滚动/翻页当场失效、误报章末、连按还跳章。旧实现的值域天然 ⊆ `[0, end_scroll]`,新公式没有这个不变量,要显式钳。
- **末尾留白是视口局部的视觉状态**,不能当普通比例存:留白量由当时的 `total`/`view` 决定,故 `ScrollTarget::Overscroll { ratio, end_scroll }` 记下产生它的 `end_scroll`,解析时不匹配就退化为贴底;落盘只存比例(重开回到合法位置)。
- **切章一定要重置** `scroll_target`:`on_next`/`on_prev`/**`on_select`(目录选章)** 三处都要——漏一处就会让新章沿用上一章的比例,短章节里直接落到贴底之下。
- `use_scrollbar` 的 content_length 传**最大滚动位置**(`end_scroll`)而非 `total`:它内部把两个入参同除以组件高度,传 `total` 会让 position 恒小于 content_len、滑块永远到不了底。
- 底部行号的分子取**最后可见行** `(current_line + view).min(total)`:读到章末显示 `121/121` 而不是 `104/121`(读完了只报 86%)。
- **「章末」是意图不是比例,用 `ScrollTarget { Ratio(f64), ChapterEnd }` 而非在 f64 里塞哨兵**：`GoBottom` 与「顶部 ↑ 翻回上一章」发出的是意图,落盘写回的是位置,两者共用一个 `State<f64>` 就只能靠取值区间(`>= 1.0`)区分 —— 那个「用户写回的值必然 < 1.0」的不变量横跨三处表达式,靠注释维持。拆成两个变体后 `resolve(total, end_scroll)` 是唯一解析点,f64 只在持久化边界转换(`from_ratio`/`as_ratio`,`ChapterEnd` 编码为 1.0)。
- 章末判定用 `current_line >= end_scroll`（最后一行已可见），**不是**「滚动到上限」——两者在末尾留白后不再等价。
- 底部行号与 `use_scrollbar` 的 content_length 都传 `total`（旧代码传的是 `total - view`，那个数没有阅读含义）。

**末尾留白无需额外的上限常量**：`current_line < end_scroll` 是执行 `+= step` 的前提，故落点上界 `(end_scroll-1)+step ≤ total-1`，天然不越界。多写一个 `max_scroll` 常量反而多一个要维护的不变量。

**破坏性**：分母变更让存量进度一次性前移约 `percent × view` 行（最多约一屏），已确认接受、不做迁移——迁移需要写入时的视口高度，而那从未被记录。

**相关文件**：`src/pages/read_novel/read_content.rs`（`visible_lines` / `percent_of` / memo 特判）、`src/cache/{local_novel,network_novel}.rs`（落盘字段）

### 逐行滚动与整页翻页要合并成同一支,别把边界语义抄两份

`ScrollDown` 与 `PageDown` 的唯一差异是步长，边界行为（全书边界只提示 / 章内边界武装 / 二次确认翻章）必须完全相同。旧代码把它们写成四个独立 match 分支，结果 `PageUp`/`PageDown` 到章首章末只是 `.min()` 停住、**不翻章**——纯用整页翻页读书的用户每到章末都得切去按 `j`（issue #63）。

**正确做法**：`ReaderAction::ScrollDown | ReaderAction::PageDown => { let delta = if action == PageDown { step } else { 1 }; ... }`，边界分支单一实现。加第三种步长（半页）时只需多一个 `delta` 取值。显式 `←/→` 翻章仍走各自分支、立即翻章不武装（快路保留）。

### shell 键 q/g/b 必须排除 Ctrl/Alt —— 否则 Ctrl+B 会被当成「返回」

`layout.rs` 的 shell 键只 match `key.code`,而 `key.code` **不带修饰信息**:`Ctrl+B` 与裸 `b` 对它完全一样 → 阅读页把 `ctrl-b`/`ctrl-f` 绑成翻页后,按 `Ctrl+B` 会直接退回上一路由;`Ctrl+Q` 同理会退出程序。keymap handler 在 `is_scroll == false`(章节选择模式、任一浮层打开)时返回 Ignored,事件就一路落到 root 层的 shell handler。

**正确做法**:handler 开头 `if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) { return Ignored }`。**SHIFT 必须放行**——大写字母 `Q`/`G`/`B` 本身就带它。

**相关文件**:`src/app/layout.rs`、`src/keymap.rs`(`ctrl-b`/`ctrl-f` 默认绑定)

### 防抖 effect 在组件卸载时是 **drop 而非 flush**,须用 use_on_drop 兜底

`use_debounce_effect` 底层是 `use_async_effect`,future 存在 hook 里、只靠 `poll_change` 驱动;组件卸载时 `InstantiatedComponent::drop` 只调 `hooks.on_drop()`,**那个还没睡满的 `sleep` 永远不会醒,回调从不执行**。把同步 `save()` 换成防抖后,「改完立刻退出/返回」的改动会静默丢失。

更隐蔽的是**丢了不会自愈**:若用「已落盘值」state 去重且它初始化自进程级 atom,重进页面时该 state 会被那个「已改但未落盘」的内存值初始化,首次比对即相等直接返回 —— 这次改动在本进程内**再也不会**被写盘。

**正确做法**:防抖之外再挂一个 `use_on_drop`,比对后同步补写(卸载路径阻塞一次写盘可接受,`History` 一直如此)。另外「记为已存」要放在**写盘成功之后**——先设后写会把失败的那次永久吞掉,而失败后不推进就能在下次改动时连同重试。

**相关文件**:`src/pages/read_novel.rs`、`src/hooks/use_debounce_effect.rs`

### 阅读偏好的落盘要收敛到单一防抖点

`READER_DISPLAY` atom 有多个写入方（`v` 键、阅读设置面板的每个条目），若各自 `save()` 会出现多套写盘时序，面板里连按 `←/→` 还会每帧写穿磁盘。**统一在 `ReadNovel`（页面级唯一实例）用 `use_debounce_effect(move || { let _ = cfg.read().save(); }, *cfg.read(), DebounceOptions::default())` 落盘**，写入方只改 atom。放页面级而非 `App` 根：该配置只在阅读页被修改，重渲染范围也就止于本子树。

**取值钳位放在配置类型内**（`ReaderDisplayConfig::page_overlap()` 访问器）而非各调用点——手改 JSON 写入的越界值由类型自己兜底。

**相关文件**：`src/cache/setting.rs`、`src/pages/read_novel.rs`、`src/pages/read_novel/settings/mod.rs`

### 设置条目要上提的是「按键协议」,不是边框

`src/components/setting_item.rs` 分两层,新增设置项一律用第二层:

- `SettingItem` —— 纯容器(边框 + 编辑态高亮),供交互特殊的条目用(下载进度的 Enter/Esc、音色选择的列表导航)。
- `AdjustableSettingItem` —— 容器 + 「←/→ 调整」协议(`label` / `value` / `on_decrease` / `on_increase`),调用方只给数据与两个回调,约 8 行。

曾经全仓有 5 处逐字相同的事件样板(`Event::Key` 解构 → `KeyEventKind::Press` → `if !is_editing { Ignored }` → `Left|h` / `Right|l`),「同层多个条目都收到这些键、只有聚焦者可消费」这条约定被复制 5 份 —— 漏写一处就是一次按键被多个条目同时响应,而开关型条目的取值展示已经开始漂移(`开/关` vs `true/false`)。**会漂移的是行为,不是外观**;将来把面板键位接入 keymap 体系也只需改这一处。

**步进与边界收进配置类型**(同配置类型内的步进方法):`ReaderDisplayConfig::increase_page_overlap()` / `page_step(view)`。UI 不该知道上界 —— 那样 `PAGE_OVERLAP_MAX` 才能保持私有。越界的磁盘值在 `load()` 归一,别用「读侧访问器兜底」:那会让脏值永远留在文件里,且字段可写、访问器同名,靠注释约束等于没有约束。

### 历史 TTS 高亮辅助函数的哨兵字符 —— `\u{002E}` 就是英文句点

正文搜索接入后，阅读组件改用共享的 `ContentLayout` 原文坐标排版；下面描述保留的 `highlight` 辅助函数与历史回归案例。

`textwrap` 换行会重排文本、原始字节偏移随之失效,所以高亮范围是用哨兵字符**嵌进文本**里跟着走的(`highlight` 塞标记 → `highlight_text` 按标记切 span)。起始标记一直是 `\u{001E}`(记录分隔符,没问题),**结束标记却写成了 `\u{002E}`** —— 那不是控制字符,就是普通的英文句点 `.`。

于是 `highlight_text` 里的 `split_once('\u{002E}')` 会在正文的第一个句点处提前收尾:朗读到「圆周率约等于 3.14」时,高亮只到「圆周率约等于 3」就断了。中文正文里句号是「。」所以平时不明显,一旦出现数字小数点、英文缩写(`Mr.`)、ASCII 省略号就露馅。

**正确做法**:两个标记都从 C0 控制字符里取(现用 `\u{001E}` / `\u{001F}`,记录/单元分隔符),并**提成命名常量**,别在 `format!` 里写裸转义 —— 裸写时 `001E` 和 `002E` 只差一个字符,肉眼极难发现。改这两个常量前先确认新字符不可能出现在小说正文中。

**验收**:回归测试要断言「高亮段含 ASCII 句点时不被截断」。写完把常量改回 `\u{002E}` 跑一遍,确认测试真的会红 —— 否则锁不住这个 bug。

**相关文件**:`src/pages/read_novel/read_content.rs`(`HIGHLIGHT_START` / `HIGHLIGHT_END`)

### 渲染路径上别编译正则 —— 找一个已知子串用 `str::find` 就够

`highlight` 曾在每次调用时 `regex::escape(&segment.text)` + `Regex::new(...).unwrap()`,只为在正文里定位一个**字面量**子串。它跑在渲染路径上(每次 TTS 高亮推进都会重算),等于每帧白编译一次正则;`.unwrap()` 还是 TUI 里的崩溃点。

不能直接 `&text[segment.start..][..len]` 切片,是因为 `TextSegment.text` 被 `trim()` 过,而 `start` 指向 trim **之前**的位置,两者差着几个空白 —— 这正是当初上正则的原因。但 `str::find` 同样能从 `start` 起搜:

```rust
let found = text.get(segment.start..).and_then(|rest| {
    rest.find(segment.text.as_str()).map(|offset| segment.start + offset)
});
```

**用 `get()` 而不是索引**:换章的瞬间 `highlight_range` 可能还指向上一章的偏移,越界或落在非字符边界上都会 panic —— 渲染路径上 panic 会直接掀掉整个 TUI。`get()` 越界返回 `None`,退化成无高亮的正常排版。

**相关文件**:`src/pages/read_novel/read_content.rs`(`highlight`)

### 遍历目录一律用 walkdir 缓存的 `file_type()`,不要 `Path::is_dir()`

`Path::is_dir()` / `is_file()` **每次调用都打一次 metadata syscall**,而 walkdir 的 `DirEntry` 已经缓存了类型。更要命的是两者语义不同:`Path::is_dir()` **跟随符号链接**,`DirEntry::file_type()` 不跟随(walkdir 默认 `follow_links(false)`)。

本地书库扫描一度**排序用 `file_type()`、遍历用 `path().is_dir()`** —— 指向目录的软链会被「排序时当文件排在后面、遍历时当目录递归进去」,顺序不符合「目录优先」的约定;而那段排序注释恰恰在讲判定不一致会破坏全序。判定散在两处就迟早会漂移,**取一次 `let is_dir = entry.file_type().is_dir();` 往下传**。

顺带两点:`WalkDir::min_depth(1)` 能直接跳过根目录自身,省掉逐条 `entry.path() == root` 的比较(原写法还 `to_path_buf()` 分配了一次);扩展名比较要用 `eq_ignore_ascii_case`,否则 Windows 上常见的 `.TXT` 扫不进来。

**相关文件**:`src/file_list.rs`(`scan`)

### 「索引」要真的预计算 —— 别把省下的扫盘换成每次过滤的重复分配

`NovelFileIndex` 的意义是「扫一次盘,之后反复过滤」。但过滤一度是 `path.file_name().to_string_lossy().to_lowercase().contains(&filter.to_lowercase())` —— 每个条目两次字符串分配,查询词的 `to_lowercase()` 还对**每个文件**重做一遍,而索引里明明已经存了 `name`。

**正确做法**:索引里存预先算好的 `name_lower`;查询词在过滤入口 `trim().to_lowercase()` **一次**,往下传 `&str`。

**顺带**:`filter(Option<&str>)` 里 `None` 和 `Some("")` 语义完全相同,是冗余表达,收成 `filter(&str)`(空串=不过滤)让调用点和测试都短一截。用 `is_directory: bool` + `children` 表达节点类型同理 —— 换成 `enum EntryKind { File, Directory(Vec<_>) }`,「文件不可能有子节点」就成了类型保证而不是口头约定。

**相关文件**:`src/file_list.rs`

### 给设置面板加条目:`ITEM_COUNT`、索引判断、**面板高度**三处必须同步

浮层高度是写死的 `Constraint::Length(N)`。只加条目不改高度,新条目会被**静默裁掉** —— 面板照常打开、导航索引也能移到它身上,就是看不见,极易误判成「条目没渲染」。

**正确做法**:让高度从条目数派生,加条目时改一个常量即可:

```rust
const ITEM_COUNT: usize = 3;
/// 每个条目 3 行(含边框),外加浮层自身的边框与外边距 4 行。
const PANEL_HEIGHT: u16 = ITEM_COUNT as u16 * 3 + 4;
```

**验收**:VHS 截图里数一遍条目数 —— 单测覆盖不到「被裁掉」这种纯布局问题。顺带确认导航键没有串扰调整(`↑/↓` 选择、`←/→` 调整,按 `↓` 走到某项时该项的值 MUST NOT 跟着变)。

**VHS 截图时序坑**:`Screenshot` 后若紧跟下一个按键而不留 `Sleep`,截到的可能是**按键之后**的帧 —— 曾据此误判「值在按右键之前就变了」。断言「操作前」状态的截图,前后都要留 ≥1s。

**相关文件**:`src/pages/read_novel/settings/mod.rs`

### 阅读页浮层用单一 `Panel` 状态,不是每个面板一个 bool

`enum Panel { None, Tts, ReaderSettings }` + `use_state`。互斥由类型保证,`is_scroll = panel == None && !info_modal_open`,两个入口 arm 合并成一支。用两个 bool 就得在每个入口手写「关掉另一个」,并让 `is_scroll` 退化成三重取反、帮助内容变 if/else-if 链 —— 接线随面板数平方增长,漏改一处是静默的状态错位。`info_modal_open` 保持独立:它叠在面板之上,不是同层互斥项。

其余三条既有约定仍然适用:

- `blocks_lower: false` —— 否则截断 root 层 handler,面板一开就关不掉(入口键在父级 `ReadNovel` 上)。
- 浮层关闭时**在 hooks 全部注册完之后提前返回** `element!(View).into_any()`:组件每帧重跑,提示行的 `describe()` + `format!` 与整棵子树否则会每帧构建一次,而它绝大多数时间不可见。同理 `ShortcutInfoModal` 的快捷键表(十余次 `describe()` + 字符串拼接)要包在 `if info_modal_open.get()` 里再构建。
- **UX**:数值型条目把派生值同屏展示(`翻页重叠: 2 行(每页滚动 26 行)`)——用户无法凭「2」想象效果,零成本的可解释性。

**相关文件**：`src/pages/read_novel/settings/mod.rs`、`src/components/setting_item.rs`、`src/pages/read_novel.rs`(`Panel`)

### `ScrollView(active: true)` 会吞掉子树的 j/k/h/l —— 面板做焦点导航必须关掉它

`ScrollView` 以 `EventScope::Current + Normal + hit_test` 在**自己这层**注册 handler,而 `ScrollViewState::handle_event` 对 `Up/Down/Left/Right`、`j/k/h/l`、`PageUp/PageDown`、`Home/End` 是**「match 命中即返回 true」**——`scroll_up()`/`scroll_down()` 是无条件的 `const fn`(`saturating_sub(1)` / `+1`),**不管能不能真的滚都会 `Consumed`**。它又是面板组件的子节点,于是父级的焦点导航永远收不到事件。

症状:听书面板打开后 `j/k/↑/↓` 无法移动焦点(始终停在第一项),`←/→` 调不到速度音量、切不了音色。**内容没超出一屏时照样发生**——因为吞键与能否滚动无关。

**正确做法**(用框架给的「跟随选中项」primitive,而不是关掉滚动能力):

```rust
let scroll_state = hooks.use_state(ScrollViewState::default);
// 焦点移动后把选中项滚进视口
hooks.use_effect(move || scroll_state.write().scroll_to_index(index.get()), index.get());
element!(ScrollView(active: false, state: scroll_state) { ... })
```

这样导航键归面板、视口仍会跟随焦点,终端过矮、条目超出一屏时也能看到当前项。

**判别**:同一套 handler 结构在 `Border` 里正常、在 `ScrollView` 里失灵 → 就是它。阅读设置面板(`settings/mod.rs`)用 `Border` 包裹,故一直正常。

**相关文件**:`src/pages/read_novel/tts/mod.rs`、`ratatui-kit` `components/scroll_view/{mod.rs,state.rs}`

### `use_memo`/`use_effect` 的 deps 必须传**值**,传 `State` 句柄等于永不重算

`ReactiveHandle` 的 `PartialEq` 是 `*self.read() == *other.read()`(**按值**比较)。而 deps 里存的旧句柄与本帧句柄**指向同一个 generational-box 槽** —— 比较时两边 deref 出的都是**当前值**,即「当前值跟自己比」,恒等。于是 `use_memo(f, some_state)` 的 deps 永远不变,**memo 永不重算**。

症状极具迷惑性:状态确实变了(debounce 写回磁盘的值是新的),但派生出来的 UI 冻结在首帧,看起来像「按键没生效」。音色选择曾中招——`←/→` 实际改了 `current_voice`、也落了盘,但显示的 `prev/current/next` 永远是首帧那组。

**正确做法**:`use_memo(f, current_voice.get())`;deps 里要放多个就用元组 `(a.get(), b.get())`。**不要**写 `use_memo(f, current_voice)`——它编译得过(`State<T>` 满足 `PartialEq + Unpin + 'static`),但语义是死的。

**相关文件**:`src/pages/read_novel/tts/voice_select.rs`、`ratatui-kit` `reactive_handle.rs`(`impl PartialEq<ReactiveHandle> for ReactiveHandle`)

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

ambient 单例(主题 / 浏览器提示)从「`App` `use_state` + 深嵌套 `ContextProvider` 链 + 后代 `use_context`」改为 module-level `static Atom`:

- 声明方式:`pub static FOO: Atom<FooConfig> = Atom::new(FooConfig::default);`(`Atom::new(fn() -> T)` 是 **const fn**,可作 static;无捕获闭包 `|| None` 也行)。`Atom<T>` 要 `T: Send+Sync`,`use_atom` 另要 `Unpin`。旧版示例里的 `THEME: Atom<ThemeConfig>` 已被 0.10 主题重构替换为 `APPEARANCE` / `READER_DISPLAY`。
- 组件内订阅:`hooks.use_atom(&THEME)` 返回 `AtomState<T>`(Copy 句柄,API 同 `State`)。
- **组件外/后端直接读写**:`THEME.set(v)` / `THEME.get()`(`Atom::set/get` 取 `&self`,无需 hooks)——这把 `browser_assist` 那套「OnceLock 持 UI State 句柄给 build_engine」的桥**整个删掉**:`BROWSER_PROMPT` 是 static 全局可达,`TuiBrowserUi` 退化成无状态单元结构体直接读写它。
- **坑:`use_atom` 是 `&mut self`**(注册 waker 的 hook)。把它包进 `&self` 的辅助方法会强制该方法变 `&mut self`,**波及所有非 mut hooks 的调用点**(`fn Foo(.., hooks: Hooks)` → `mut hooks`)。旧 `UseThemeConfig::use_theme_config` 就踩过这个坑,当前主题代码不再保留该 helper。
- **不 atom 化带 Drop 存档的缓存**:`History`/`BookSourceCache` 有 `impl Drop { save() }`,而 `static` 析构永不运行 → 仍由 `App` `use_state` 持有(provider 链从 6 缩到 3)。
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

**相关文件**：`src/cache/setting.rs`、`src/state.rs`、`src/app.rs`、`src/theme/mod.rs`、`src/pages/theme_setting/mod.rs`

### 正文搜索使用原文坐标与独占输入层

正文搜索先对无样式正文做 `textwrap::wrap`，记录每个显示行对应的原文字节范围，再裁切 spans 添加搜索和 TTS 样式。不要插入高亮标记参与换行：多处命中会改变排版，导航坐标也会失效。排版与匹配集合用 Arc 缓存，搜索修订号驱动导航和样式更新。

**正确做法**：
- 页面持有搜索状态，以章节索引和阅读模式重置，不能只依赖正文字符串（不同章节可以同文）。
- 受控单行输入复用框架 Input + tui_input，编辑时注册 blocks_lower 输入层；框架 SearchInput 的 is_editing 是“允许激活”，不是受控打开状态。
- 结果切换提示必须保留 n/N 的大小写，通用键名大写美化会把二者显示成同一个键。

**相关文件**：`src/pages/read_novel/search/mod.rs`、`src/pages/read_novel/read_content.rs`、`src/keymap.rs`。

### 可选听书模块的生命周期

`src/tts.rs` 下的 client/controller/ui 经根 tts feature 装配。App 只提供 Handle 和协议状态快照 Context，use_future 订阅 watch；不再持有 TTSConfig::Drop 或 native 模型 atom。

**正确做法**：启动路径在 run 的生命周期内创建轻量 actor，只有 Open/Toggle/Prepare 明确动作才连子进程。退出及 Ctrl+C 取消正在等待的命令，再在三秒内 shutdown 或 kill/reap。正文页用来源、正文摘要和当前终态过滤事件；挂载时记录终态修订号，防止旧 completed 使重进页面自动播放。只有已完成会话的明确下一章来源才能延续自动续章。

**坑**：不能在 hooks 参数同步求值处 spawn。正文摘要及请求应 use_memo，避免每帧重算整章 SHA。动态浮层节点先算成变量，放到 Fragment 内，不能紧贴没有 children 字段的 ReadContent 后嵌表达式。设置值只在子进程确认后更新，快速输入先排入有界 actor 队列。

**相关文件**：`src/app.rs`、`src/tts/controller.rs`、`src/pages/read_novel/read_content.rs`。

### 空 Fragment 会参与布局，不能用作关闭插件的零尺寸占位

ratatui-kit 0.10.3 的透明组件在 update 后继承第一个子节点的 LayoutStyle；无子节点时回退到默认布局约束。因此 `element!(Fragment)` 不保证零尺寸。无 TTS 构建把它作为听书面板占位，会在阅读页的垂直 View 中分走正文高度：打开阅读设置或帮助时，正文缩到上半屏，滚动行数仍按全终端算。

**正确做法**：可选面板禁用时返回 `View(width: Constraint::Length(0), height: Constraint::Length(0))`，与关闭 Modal 的布局占位一致。真正无子节点的 Fragment 不适合代替零尺寸 UI。VHS 必须分别录制带/不带 feature 的二进制；编译检查发现不了此布局问题。

**相关文件**：`src/pages/read_novel.rs`（`listening_panel`）、`docs/tapes/basic-ui.tape`。

### 听书视口跟随

跟随只消费已匹配来源与正文摘要的实际播放范围，通过 ContentLayout::match_line 定位。暂停/缓冲、搜索编辑、目录预览和模态面板不能自动移动窗口；f 是用户主动定位的例外。自由浏览标志放在 ReadNovel，不能放在会被目录/正文切换重建的 ReadContent，否则自动续章或视图切换会重新开启跟随。

阅读偏好 followTts 缺省 true，基础版仍保留该序列化字段但隐藏入口。use_effect 的可变状态闭包与 deps 共享句柄时，先计算 deps 值，避免参数求值造成借用冲突；超过十二项的 tuple deps 需分组以满足 PartialEq。

**相关文件**：`src/pages/read_novel/follow.rs`、`src/pages/read_novel/read_content.rs`。

### 从目录进入当前章节不能开启内容 loading

ReadNovel 的内容加载 effect 仅依赖 current_chapter。首次进入目录时正文已由初始化流程获取；确认当前章只是切换阅读视图，章号不变，不会触发 effect。此时置 content_loading=true 会永久遮住已有正文，本地和网络均受影响；左右切章改变依赖后才恢复。

**正确做法**：目录确认仅在目标章号不同于当前章时同时设置 loading 和章号。同章确认仍保留既有滚动重置与进入正文行为。

**相关文件**：`src/pages/read_novel.rs`。

### TTS 模型目录与组件回归

协议目录一行对应一个模型，后端选择去重后必须按 backend ID 定位当前行；不能比较整个 Capabilities，否则 Qwen 1.7B 会与保留的 0.6B 行不匹配。后端边界无变化时直接返回，保留模型与音色。

ratatui-kit test-util 可渲染实际设置组件并调用生产设置回调，对接隔离配置的真实 worker；中文宽字符的第二个空 cell 不属于文本。此类回归覆盖组件和协议，不等同于物理终端按键验收。

**相关文件**：`src/tts/ui.rs`、`src/tts/ui/tests.rs`。

### 模型切换与编译设备

同后端的模型也可能使用不同推理栈和设备，如 Nano ORT / Local-Realtime Candle。模型选择回调仅在原显式设备不属于目标模型 compiled_devices 时切换为 Auto；兼容设备保留，实际推理显式请求仍不降级。Nano 的显式目录 ID 让 model=None 的旧默认可以被重新选择。

**相关文件**：`src/tts/ui.rs`、`crates/novel-tts-backends/src/lib.rs`。

## 2026-10-08 独立发行与数据归属（当前实现）

本节取代上文的旧路径、子模块及双变体发行指导；历史验收记录保留。TRNovel 只分发 trnovel/trn，默认 tts，使用 crates.io talechime-protocol 0.1.0，JSON Lines v5，不要求应用同版本，不递归检出。worker 顺序：CLI --tts-program → config.toml 的 [tts].program → PATH talechime；无效显式路径不回退，缺 worker 不阻断普通阅读。

src/paths.rs 统一 ~/.trnovel 路径：config.toml 的 appearance/reader/browser/tts 分节由 ConfigStore 加锁、重新加载并原子替换；keybindings.toml、toc_rules.json 独立；data 保存书源/历史/登录/browser-profile/local/network，cache 保存字体等可重建资源。clear 只删历史、local/network、cache，不触碰配置、书源或登录态。旧目录不读取、不自动搬迁，手工说明在 guides/migration.mdx。

Talechime 独立拥有 ~/.talechime/config.json、checkpoints、resources，AppPaths 位于其 core crate。两个应用固定 cargo-dist 0.32.0、各一份原生 Cargo dist-workspace.toml；旧 generic dist.toml、basic/加速/ARM64 musl 专用脚本已移除。Talechime 标准包 CPU（Windows/Linux），Mac 添加全部 Metal；CUDA 源码与原生编译 CI 保留。三个平台隔离安装检查不下载模型，不视为真实模型试听通过。
