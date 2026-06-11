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

<!-- 随开发补充:ratatui 0.30 升级坑（Block !Send / WidgetRef 门控 / TextArea 下线）、自定义 hook 约定等 -->
