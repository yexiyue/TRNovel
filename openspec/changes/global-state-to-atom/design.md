## Context

`src/app/mod.rs` 把 7 个全局 `State<T>` 经一条深嵌套 `ContextProvider` 链(ThemeConfig→History→BookSourceCache→TTSConfig→NovelTTS→is_inputting→browser_prompt→RouterProvider)透传给后代,后代用 `*hooks.use_context::<State<T>>()` 按类型取。`CLAUDE.md` 警告重排会破坏后代查找;按类型取还有「同类型多实例会撞」的隐患。

为让**非 UI 代码**触发浏览器验证模态,`browser_assist` 用 `OnceLock` 把 `browser_prompt` 的 `State` 句柄在 `init_browser_ui(state)` 时登记,`build_engine` 撞挑战时经该 `OnceLock` 反查句柄再 set——一套把 UI 状态「漏」给后端的桥。

`ratatui-kit` 0.7 的 `Atom`(module-level `static`,`use_atom(&ATOM)` 订阅,写入只唤醒订阅者)天然适配单实例 TUI:全局单例本就唯一,`static` 正合适;`AtomState: Copy+Send` 且 `static` 全局可达,非 UI 代码可直接读写。前置 `upgrade-ratatui-kit-07` 已删 `is_inputting`,链变 6 层。

## Goals / Non-Goals

**Goals:**
- 3 个 ambient 单例(theme/novel_tts/browser_prompt)→ `static Atom`,删对应嵌套 provider。
- 删 `browser_assist` 的 `OnceLock` 桥,非 UI 代码经 `BROWSER_PROMPT.state()` 直接读写。
- `use_context` 收敛:atom 化的改 `use_atom`,provider 链缩到 3 层。
- 对终端用户零可见行为变化;存档语义不变。

**Non-Goals:**
- atom 化 `History`/`BookSourceCache`/`TTSConfig`(Drop 存档,见 D1)。
- 改存档文件格式或显式 `.save()` 路径。
- 触碰后端两 crate。

## Decisions

### D1:Drop-存档缓存留 `use_state`,不 atom 化

`History`/`BookSourceCache`/`TTSConfig` 各有 `impl Drop { save() }` 作退出兜底。**Rust `static` 的析构永不运行**,atom 化即丢这层安全网。虽另有 10 处显式 `.save()`,但 Drop 兜底覆盖「用户直接退出、未触发显式保存」的路径,不可丢。故这 3 个保持 `App` `use_state` + 3 层 `ContextProvider`(承载者全程随 App 存活,卸载时 Drop 触发存档)。

**Alternative(否决)**:全 atom 化 + 退出时显式 `save-on-exit` 收尾 hook。→ 需新写并验证收尾逻辑,漏一个就丢档;Drop 兜底是现成且经过验证的,无谓承担风险。

### D2:`browser_prompt` atom 化顺带消灭 `OnceLock` 桥

`static BROWSER_PROMPT: Atom<Option<BrowserPrompt>> = Atom::new(|| None);` 一立,`OnceLock<Arc<dyn BrowserUi>>` 整套即多余:`init_browser_ui` 改传 `BROWSER_PROMPT.state()`(或直接让 `browser_ui()` 返回 atom 句柄);`build_engine` 撞挑战 `BROWSER_PROMPT.state().set(Some(prompt))`,UI 侧 `use_atom(&BROWSER_PROMPT)` 订阅弹模态。这是把「UI 状态漏给后端」的绕行换成「后端直接写进程级状态、UI 订阅」——更直、更少间接。

### D3:`UseThemeConfig` 内部切到 `THEME` atom,收敛主题订阅

主题被 33 处中的多数经 `hooks/use_theme_token.rs` 的 `UseThemeConfig` 间接读。把该 hook 内部从 `use_context::<State<ThemeConfig>>` 改为 `use_atom(&THEME)`,则所有 `use_theme_config()` 调用点**一行不改**即切到 atom。novel_tts(~4 处)、browser_prompt(1 处)直接调用点改 `use_atom`。

### D4:启动期写 atom,取代 `use_state` + 加载

现 `App` 的启动 future 把磁盘值 set 进各 `use_state`。改为:atom 的 `Atom::new(default)` 提供默认,启动 future 把加载值写进 `THEME`/`NOVEL_TTS`(browser_prompt 默认 `None`,无需启动写)。Drop-缓存(History/BookSourceCache/TTSConfig)的加载路径不变。

## Risks / Trade-offs

- **[`BrowserPrompt::Click` 的 `Arc<AtomicBool>` 提前 drop]** atom 写入换值时若旧 `Click` 被 drop,而后台 `authorize()` 仍 `cancel.load()` 该 `Arc` → 悬挂/逻辑错。**Mitigation**:写入用替换语义(`set(Some(new))` 而非先清空);`authorize()` 持有的是 `Arc` 克隆,引用计数保命——核对 `authorize()` 确实 clone 了 `Arc` 再轮询(spec scenario「取消信号存活」验收)。
- **[Drop-缓存承载者被中途重建 → 过早存档]** 若重构误把这 3 个的 provider 移到会随页面卸载的位置。**Mitigation**:保持在 `App` 根的 `use_state` + provider,全程存活(本就如此);tasks 显式核对。
- **[主题写入 rerender 风暴]** 33 订阅点,若每次主题微调都写 atom 可能频繁重渲染。**Mitigation**:主题变更经既有 `.save()`/effect 节奏,非每键写;`use_atom` 写入只唤醒订阅者,范围已最小。
- **[与 `upgrade-ratatui-kit-07` 的顺序]** 本 change 用 `Atom` API,且动 `app/mod.rs` 同一条链。**Mitigation**:排在 0.7 升级之后做(proposal 已注前置)。

## Migration Plan

1. 定义 3 个 `static Atom`(theme/novel_tts/browser_prompt)。
2. `UseThemeConfig` 内部切 `use_atom(&THEME)`;novel_tts/browser_prompt 直接调用点切 `use_atom`。
3. `browser_assist`:删 `OnceLock`,`init_browser_ui`/`browser_ui`/`build_engine` 改走 `BROWSER_PROMPT.state()`。
4. `app/mod.rs`:删 3 个 ambient `use_state` + 对应 provider 层;启动 future 改写 atom;保留 3 个 Drop-缓存的 `use_state` + provider。
5. 验证:CI 四件套 + `cargo run`;逐 spec scenario 手验(主题全局生效、TTS 跨页保留、引擎撞挑战弹模态、取消信号存活、退出存档触发)。

回滚:独立分支,`git revert`;atom 与 use_context 改动可逐项还原。

## Open Questions

- **statics 放哪个模块?** `app/mod.rs` 顶部,还是新 `src/state.rs` 聚合?倾向后者(集中、避免 app/mod.rs 膨胀),实现期定。
- **`browser_ui()` 返回类型**:删 `OnceLock` 后是返回 `AtomState` 句柄还是保留 `dyn BrowserUi` trait 抽象(便于测试/替换)?看 `build_engine` 对 trait 的依赖程度,实现期定。
