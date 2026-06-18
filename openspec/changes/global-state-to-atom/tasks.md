## 1. 定义全局原子

- [x] 1.1 statics 落点:新建 `src/state.rs` 放 `THEME`/`NOVEL_TTS`;`BROWSER_PROMPT` 就近放 `browser_assist.rs`(与 `BrowserPrompt`/`TuiBrowserUi` 同模块,避免循环 import)
- [x] 1.2 `pub static THEME: Atom<ThemeConfig> = Atom::new(ThemeConfig::default);`
- [x] 1.3 `pub static NOVEL_TTS: Atom<Option<NovelTTS>> = Atom::new(|| None);`
- [x] 1.4 `pub static BROWSER_PROMPT: Atom<Option<BrowserPrompt>> = Atom::new(|| None);`(在 browser_assist.rs)
- [x] 1.5 `Unpin+Send+Sync` 编译验证通过(`Atom<T>` 要求 `T: Send+Sync`,`use_atom` 要求 `Unpin`)

## 2. 订阅点切换到 use_atom

- [x] 2.1 `use_theme_token.rs`:`UseThemeConfig::use_theme_config` 内部改 `use_atom(&THEME)`;**签名 `&self`→`&mut self`**(use_atom 是注册 waker 的 hook),波及 confirm/search_input/shortcut_info_modal/SettingItem 4 处非 mut hooks → 改回 `mut hooks`
- [x] 2.2 novel_tts 调用点(实际仅 1 处 `tts/mod.rs`)→ `use_atom(&crate::state::NOVEL_TTS)`
- [x] 2.3 `components/modal/browser_prompt.rs`:`use_context` → `use_atom(&BROWSER_PROMPT)`;helper fn `respond`/`cancel_click` 参数 `State`→`AtomState`
- [x] 2.4 无遗留 `use_context::<State<ThemeConfig/Option<NovelTTS>/Option<BrowserPrompt>>>`(cargo check 确认)

## 3. 消灭 OnceLock 浏览器桥

- [x] 3.1 `browser_assist.rs`:删 `OnceLock<Arc<dyn BrowserUi>>` + `init_browser_ui`;`TuiBrowserUi` 变无状态单元结构体(读写 `BROWSER_PROMPT` 而非持 State 字段)
- [x] 3.2 `browser_ui()` 直接返回 `Some(Arc::new(TuiBrowserUi))`;各 `BrowserUi` 方法用 `BROWSER_PROMPT.set(...)` / `.state().write()`
- [x] 3.3 `authorize` 的「无弹窗才弹」用 `BROWSER_PROMPT.state().write()` 守卫跨 check+set(原子性同旧);`Click` 的 `Arc<AtomicBool>` 由引擎侧克隆保活,atom 替换写入不会提前 drop

## 4. app/mod.rs 收敛 provider 链

- [x] 4.1 删 theme/novel_tts/browser_prompt 三个 `use_state` + 三层 `ContextProvider`(链 6→3)
- [x] 4.2 启动 future 把加载的 theme 写 `crate::state::THEME.set(...)`(novel_tts/browser_prompt 默认 None,无需启动写)
- [x] 4.3 **保留** History/BookSourceCache/TTSConfig 的 `use_state` + 3 层 `ContextProvider`(Drop 存档全程随 App 存活)
- [x] 4.4 删 `init_browser_ui(browser_prompt)` 调用 + 相关 import(NovelTTS/ThemeConfig)

## 5. 验证

- [x] 5.1 `cargo clippy -D warnings`(0) / `cargo fmt --check` / `cargo test`(全绿) / `cargo doc -D warnings`(0) 通过
- [x] 5.2 `cargo build` 主程序通过(`build_engine` 写 `BROWSER_PROMPT` 的 Future 仍 `Send`)
- [ ] 5.3 (待人工)`cargo run` 逐 spec `global-state-store` scenario 手验:主题全局即时生效 / TTS 模型跨页保留 / 引擎撞挑战弹模态 / 点击取消信号存活 / 退出后历史·书源·TTS 配置已落盘
- [x] 5.4 gotcha 记入 `dev-notes/knowledge/tui-ratatui-kit.md`(atom 非 hook 写入 Atom::set/get、use_atom &mut self ripple、Drop-缓存不 atom)
