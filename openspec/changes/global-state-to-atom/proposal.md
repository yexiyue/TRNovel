## Why

`ratatui-kit` 0.7 把全局 store 重设计为 Jotai 式 `Atom`(模块级 `static`,`use_atom(&ATOM)` 细粒度订阅)。TRNovel 当前用 `src/app/mod.rs` 里一条 **7 层(`upgrade-ratatui-kit-07` 后 6 层)嵌套 `ContextProvider`** 把若干 `State<T>` 透传给后代,`CLAUDE.md` 自己都警告「重排 provider 会破坏后代查找」;且为了让**非 UI 代码**(`build_engine` 撞反爬挑战)能弹浏览器验证模态,额外用 `browser_assist` 里一套 `OnceLock` 把 `State` 句柄塞进全局——绕了一大圈。

`Atom` 直接消除这两处:**ambient 单例**(主题 / TTS 模型句柄 / 浏览器提示)改成 module-level `static`,后代 `use_atom` 订阅,删掉嵌套 provider;`AtomState` 是 `Copy + Send` 且 `static` 全局可达,`build_engine` 能直接写 `BROWSER_PROMPT.state()`,**整套 `OnceLock` 黑魔法可删**。

但**不是所有全局都该 atom 化**:`History`/`BookSourceCache`/`TTSConfig` 各有 `impl Drop { save() }` 退出兜底存档,而 **Rust 的 `static` 析构永不运行**——atom 化会丢掉这个安全网。故这 3 个**保持 `App` `use_state`**。

## What Changes

- **3 个 ambient 单例 → module-level `static Atom`**:`theme_config: ThemeConfig`、`novel_tts: Option<NovelTTS>`、`browser_prompt: Option<BrowserPrompt>`。`App` 启动期把磁盘/初始化值写入对应 atom(取代现 `use_state` + 加载)。
- **3 个 Drop-存档缓存保持 `use_state`**:`History`/`BookSourceCache`/`TTSConfig` 仍由 `App` 持有并经 `ContextProvider` 透传——它们的 `Drop::save()` 必须随 App 卸载触发,`static` 析构不运行,**不能 atom 化**。
- **BREAKING(内部 API)删除 `browser_assist` 的 `OnceLock` 桥**:`init_browser_ui(state)` 改为基于 `static BROWSER_PROMPT: Atom<Option<BrowserPrompt>>`;`browser_ui()`/`build_engine` 撞挑战时直接 `BROWSER_PROMPT.state().set(...)`,删 `OnceLock<Arc<dyn BrowserUi>>`。
- **33 处 `use_context` 收敛**:3 个 atom 化的改 `use_atom(&ATOM)`;3 个保留缓存继续 `use_context`(provider 链从 6 层缩到 3 层);`hooks/use_theme_token.rs`(`UseThemeConfig`)内部改读 `THEME` atom。
- **向后兼容**:对终端用户**零可见行为变化**(主题/TTS/历史/书源/浏览器验证流程逐项保持);存档语义不变(Drop-缓存仍 use_state、显式 `.save()` 路径不动)。

## Capabilities

### New Capabilities
- `global-state-store`: 全局状态的**存储架构契约**——区分两类:(a) ambient 单例(主题 / TTS 模型句柄 / 浏览器提示)以进程级 `static Atom` 承载,后代细粒度订阅,跨页面持久,且**可从非 UI 引擎代码读写**(浏览器验证提示无需 UI 句柄穿线);(b) 带退出兜底存档(`Drop::save`)的缓存(历史 / 书源 / TTS 配置)MUST 由 App 持有于会带 `Drop` 的存储中,**MUST NOT** 放入 `static`(析构不运行会丢存档)。此前由嵌套 `ContextProvider` + `OnceLock` 承担,无对应 spec。

### Modified Capabilities
<!--
  none:本 change 是行为保持的状态存储重构。主题/TTS/历史/书源/浏览器验证对终端用户均无规格级行为变更。
  唯一新增的是存储架构契约(global-state-store),约束「谁能 atom、谁必须留 use_state」。
  openspec/specs/ 当前为空;tui-input-dispatch 由 upgrade-ratatui-kit-07 引入,与本 change 正交。
-->

## Impact

- **前置**:建议在 `upgrade-ratatui-kit-07` 之后做——`Atom`/`use_atom` 是 0.7 能力,且该 change 已删 `is_inputting` provider、动过 `app/mod.rs` 的链。
- **新增 statics**:`src/app/mod.rs` 或专门模块定义 `static THEME: Atom<ThemeConfig>`、`static NOVEL_TTS: Atom<Option<NovelTTS>>`;`browser_assist` 定义 `static BROWSER_PROMPT: Atom<Option<BrowserPrompt>>`。
- **`src/app/mod.rs`**:删 theme/novel_tts/browser_prompt 三个 `use_state` + 对应 `ContextProvider` 层;启动期 future 改为写 atom;保留 History/BookSourceCache/TTSConfig 的 `use_state` + 3 层 provider。
- **`src/browser_assist`**:删 `OnceLock<Arc<dyn BrowserUi>>`;`init_browser_ui` 接 `BROWSER_PROMPT.state()`;`browser_ui()`/`build_engine` 改查 atom。
- **`src/hooks/use_theme_token.rs`**:`UseThemeConfig` 内部从 `use_context::<State<ThemeConfig>>` 改为 `use_atom(&THEME)`。
- **调用点**:`pages/**` 中读 theme(`use_theme_config` 间接)/novel_tts/browser_prompt 的 `use_context` → `use_atom`(theme 多经 `UseThemeConfig` 收敛,novel_tts ~4 处、browser_prompt 1 处直接)。
- **类型校验**:`NovelTTS`(`unsafe impl Send/Sync`,Arc<KokoroTts>+OutputStream)、`BrowserPrompt`(Clone enum)、`ThemeConfig` 均满足 `Atom` 要求的 `Unpin+Send+Sync`(已核实)。
- **风险**:① `BrowserPrompt::Click` 含 `Arc<AtomicBool>` 取消信号,atom 写入须用**替换而非清空**,且异步 `authorize()` 仍持有该 `Arc`(读 `cancel.load()`)期间不可被提前 drop;② Drop-缓存若被中途 provider 重建会 untimely save——保持其 provider 全程存活(App 根,本就如此);③ theme 33 订阅点写入若不收敛可能 rerender 风暴——主题改动经既有 `.save()`/effect,不每键写。
- **不影响**:`crates/parse-book-source`、`crates/novel-tts`;存档文件格式与显式 `.save()` 调用路径。
