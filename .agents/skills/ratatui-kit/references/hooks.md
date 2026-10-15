# Hooks Reference

> What this file covers: every built-in `ratatui-kit` hook — exact signature, required feature, minimal usage, and pitfalls — plus the shared `State<T>` / `AtomState<T>` data type that hooks return.

All hooks are obtained by calling a method on `Hooks` (`#[component] fn Foo(mut hooks: Hooks)`). Each method is exposed via `pub trait UseXxx: private::Sealed`, where `Sealed` is implemented only for `Hooks`, so external code cannot implement it. Everything is re-exported through `prelude::*`.

> **Global hook-order rule (applies to every hook)**: `Hooks` indexes hooks by **call order** (`hook_index`) — pushing on the first frame and `downcast`-ing them back in order on later frames. Therefore you **must never** place a hook call inside `if` / `for` / `match` / a `?` early return, or anywhere that changes the count or order of hook calls per frame. Doing so triggers a `"Hook type mismatch"` panic.

## Table of contents

- [`State<T>` / `AtomState<T>` capabilities](#statet--atomstatet-capabilities-core-data-type)
- Core hooks: `use_state`, `use_future`, `use_effect` / `use_async_effect`, `use_async_state`, `use_memo`, `use_context*`, `use_palette` / `use_component_theme`, `use_event_handler*`, `use_input_layer`, `use_insert_before`, `use_terminal_size`, `use_previous_size`, `use_exit`, `use_on_drop`
- Pointer hooks (mouse capture required): `use_click`, `use_hover`, `use_drag`, `use_pointer`, `use_hover_row`, `use_scrollbar`
- Router hooks (feature `router`): `use_navigate`, `use_route`, `use_params`, `use_route_state` / `try_use_route_state`
- Global state (feature `atom`): `use_atom`

---

## State<T> / AtomState<T> capabilities (core data type)

`State<T>` and `AtomState<T>` are both aliases of `ReactiveHandle<T, N>` (differing only in the `Notifier`: `State<T> = ReactiveHandle<T, SingleWaker>`, `AtomState<T> = ReactiveHandle<T, WakerMap>`). They share identical methods and operators, and both require `T: Send + Sync + 'static`.

- **`Copy`** (also `Clone`): the handle can be freely copied / moved into closures / `tokio::spawn`; copies share the same underlying storage.
- **Binding mutability — default to `let mut`**: State and AtomState handles are `Copy`. Their reads — `.get()`, `.read()`, `.write()` — take `&self`, but `.set()`, `.set_no_update()`, and the compound operators (`+=`, `-=`, `*=`, `/=`) take `&mut self`. Because you usually discover the need to mutate *after* writing the declaration, bind **every** handle `let mut x = hooks.use_state(..)` by default rather than deciding per-handle; the moment you `.set()`/`+=` a non-`mut` binding it's a hard "cannot borrow as mutable" error, while an unused `mut` is only a warning — the asymmetry is all upside. (Drop `mut` only for a handle you are certain stays read-only.)
- **Read** (take `&self`, no `mut` binding needed):
  - `fn get(&self) -> T` — only when `T: Copy`; returns a copy of the value.
  - `fn read(&self) -> ReactiveRef<'_, T, N>` — read-only guard (`Deref<Target=T>`); panics on conflict.
  - `fn try_read(&self) -> Option<ReactiveRef<'_, T, N>>`
- **Write (triggers a change notification / wakes the render loop)**:
  - `fn set(&mut self, value: T)` — takes `&mut self`, so requires a `let mut` binding.
  - `fn write(&self) -> ReactiveMutRef<'_, T, N>` — takes `&self`; mutable guard (`DerefMut`); only marks dirty and calls `wake()` on `Drop` **if it was actually `deref_mut`-ed**.
  - `fn try_write(&self) -> Option<ReactiveMutRef<'_, T, N>>`
- **Write (without notification)**:
  - `fn set_no_update(&mut self, value: T)` — takes `&mut self`, so requires a `let mut` binding.
  - `fn write_no_update(&self) -> ReactiveMutNoUpdate<'_, T, N>` / `fn try_write_no_update(&self) -> Option<...>`
- **Arithmetic operator overloads** (from `reactive_handle.rs`), requiring `T: Copy`:
  - `+= -= *= /=` (`AddAssign/SubAssign/MulAssign/DivAssign`): take `&mut self`, so require a `let mut` binding — they **mutate in place and trigger a change notification** (internally via `try_write`). Example: `count += 1;`
  - `+ - * /` (`Add/Sub/Mul/Div<T, Output = T>`): **read-only evaluation, returns `T`, does not write back and does not notify**. Example: `let n = count + 1;`
- **Comparison**: `PartialEq<T>` / `PartialOrd<T>` (against a raw value), `PartialEq` / `PartialOrd` / `Eq` (between handles, compared by value), plus `Debug` / `Display` / `Hash` (by current value).

> **Formatting gotcha:** the `Display`/`Debug` impls above are on the **handle** (`State<T>`/`AtomState<T>`), so `format!("{x}")` works directly. The **guards** returned by `.read()`/`.write()` (`ReactiveRef`/`ReactiveMutRef`) `Deref` to `&T` but do **not** themselves implement `Display`/`Debug` — interpolating `x.read()` straight into a `format!` / `Line::from` fails to compile (`ReactiveRef doesn't implement std::fmt::Display`). Either format the handle directly (`format!("{x}")` when `T: Display`), or deref the guard (`&*x.read()`), or pull the value out first (`x.read().clone()`, `x.read().as_str()`).

> Key difference: `State<T>` (`SingleWaker`) records a single waker; `AtomState<T>` (`WakerMap`) records multiple subscriber wakers keyed by `ElementKey`, so one atom can wake multiple components, and **all** subscribers are woken on write.

---

## use_state

- **Purpose**: component-local reactive state (like React `useState`).
- **Feature**: core.
- **Signature** (`use_state.rs`):
  ```rust
  fn use_state<T, F>(&mut self, init: F) -> State<T>
  where F: FnOnce() -> T, T: Unpin + Send + Sync + 'static;
  ```
  `init` is called only once, on the first frame. Returns `State<T>` (`Copy`, movable into closures).
- **Minimal usage**:
  ```rust
  let mut count = hooks.use_state(|| 0_u64);
  // read: count.get()   write + notify: count += 1; / count.set(5);
  ```
- **Pitfalls**: local state is released when the component unmounts (each component has its own `Owner`). The read/write guards (`read()` / `write()`) have borrow-conflict checks; do not hold a read guard and a write guard at the same time.

## use_future

- **Purpose**: register a one-shot async task (timers, network, async loops).
- **Feature**: core.
- **Signature** (`use_future.rs`):
  ```rust
  fn use_future<F>(&mut self, f: F)
  where F: Future<Output = ()> + 'static;
  ```
  No return value; the future is driven inside the render loop's `poll_change` and dropped on completion. The future is registered only once on the first frame (not rebuilt on later frames) and is **not `Send`** (uses `boxed_local`).
- **Minimal usage**:
  ```rust
  let mut count = hooks.use_state(|| 0_u64);
  hooks.use_future(async move {
      loop {
          tokio::time::sleep(std::time::Duration::from_secs(1)).await;
          count += 1; // State is Copy + Send, so it can be moved in and written back
      }
  });
  ```
- **Pitfalls**: dependencies do not restart it — to re-run on dependency changes use `use_async_effect` / `use_async_state`.

## use_effect / use_async_effect

- **Purpose**: run a side effect (sync / async) when dependencies change.
- **Feature**: core.
- **Signature** (`use_effect.rs`; the same `UseEffect` trait provides both):
  ```rust
  fn use_effect<F, D>(&mut self, f: F, deps: D)
  where F: FnOnce(), D: PartialEq + Unpin + 'static;

  fn use_async_effect<F, D>(&mut self, f: F, deps: D)
  where F: Future<Output = ()> + 'static, D: PartialEq + Unpin + 'static;
  ```
  `f` runs only when `deps != previous deps` (the first frame, where the previous `deps` is `None`, always runs once). No return value and no cleanup return (use `use_on_drop` for cleanup).
- **Minimal usage**:
  ```rust
  hooks.use_effect(move || {
      // runs when (visible_len, cursor_value) changes
      if next != cursor_value { cursor.set(next); }
  }, (visible_len, cursor_value));
  ```
- **Pitfalls**: `deps` must implement `PartialEq`; combine multiple dependencies with a tuple. The sync `use_effect` runs synchronously during update; the async version is queued and driven by `poll_change`.

## use_async_state

- **Purpose**: run an async task when dependencies change, automatically maintaining `data` / `loading` / `error` tri-state (a minimal React Query equivalent).
- **Feature**: core.
- **Signature** (`use_async_state.rs`):
  ```rust
  fn use_async_state<F, Fut, D, T, E>(&mut self, f: F, deps: D) -> AsyncState<T, E>
  where
      F: FnOnce() -> Fut + 'static,
      Fut: Future<Output = Result<T, E>> + 'static,
      D: PartialEq + Unpin + 'static,
      T: Unpin + Send + Sync + 'static,
      E: Unpin + Send + Sync + 'static;
  ```
  Returns:
  ```rust
  pub struct AsyncState<T, E> {
      pub data:    State<Option<T>>,
      pub loading: State<bool>,
      pub error:   State<Option<E>>,
  }
  ```
- **Minimal usage**:
  ```rust
  let request_id = refresh.get();
  let result = hooks.use_async_state(move || async move {
      tokio::time::sleep(Duration::from_millis(700)).await;
      Ok::<Vec<String>, String>(vec![format!("req #{request_id}")])
  }, request_id);

  if result.loading.get() { /* loading */ }
  if let Some(err) = result.error.read().as_ref() { /* error */ }
  if let Some(items) = result.data.read().as_ref() { /* data; old data stays during refresh */ }
  ```
- **Pitfalls**: internally this is `use_state × 3 + use_async_effect`, so it occupies 4 hook slots — the ordering rule still applies. On refresh it does not clear the old `data` (only sets `loading = true`), so the previous data remains visible during reloads. Because it is a compound hook (4 slots), it is subject to the same call-order rule — never call it conditionally.

## use_memo

- **Purpose**: cache a computed result while dependencies are unchanged (performance optimization).
- **Feature**: core.
- **Signature** (`use_memo.rs`):
  ```rust
  fn use_memo<F, D, T>(&mut self, f: F, deps: D) -> T
  where F: FnOnce() -> T, D: PartialEq + Unpin + 'static, T: Clone + Unpin + 'static;
  ```
  Recomputes when `deps` change or on the first frame; otherwise returns a `clone()` of the cached value.
- **Minimal usage**:
  ```rust
  let visible = hooks.use_memo(move || matching_indices(commands, &query_text), query_deps);
  ```
- **Pitfalls**: `T: Clone` (every frame returns a clone); `deps` must be `PartialEq`.

## use_context / use_context_mut / try_use_context / try_use_context_mut

- **Purpose**: look up a dependency-injected value walking the context stack upward (theme, config, `SystemContext`, etc.).
- **Feature**: core.
- **Signature** (`use_context.rs`, `trait UseContext<'a>`):
  ```rust
  fn use_context<T: Any>(&self) -> Ref<'a, T>;            // panics if not found
  fn use_context_mut<T: Any>(&self) -> RefMut<'a, T>;     // panics if not found
  fn try_use_context<T: Any>(&self) -> Option<Ref<'a, T>>;
  fn try_use_context_mut<T: Any>(&self) -> Option<RefMut<'a, T>>;
  ```
  Returns a `std::cell::Ref` / `RefMut` guard.
- **Minimal usage**:
  ```rust
  let theme = hooks.use_context::<Theme>();           // &*theme
  let mut sys = hooks.use_context_mut::<SystemContext>();
  ```
- **Pitfalls**: this is not a hook slot (it does not occupy an order position and does not call `use_hook`), but it **requires a context-aware `Hooks`**: `#[component]` function components are auto-upgraded by the macro via `with_context_stack`; a **hand-written `Component`** must first do `let mut hooks = hooks.with_context_stack(updater.component_context_stack());`, otherwise `self.context` is `None` and `use_context*` will panic (`"context not available"`). Only one guard of the same context type may be held at a time; a repeated borrow panics (`AlreadyBorrowed`).

## use_palette / use_component_theme

- **Purpose**: read the always-on theme protocol — the current `Palette` from
  the nearest `PaletteProvider`, or a specific component's resolved
  `FooTheme`. Full field-by-field reference, per-component `*Theme` tables,
  and how to make your own component theme-aware: `references/theming.md`.
- **Feature**: core (always on, zero extra deps).
- **Signature** (`trait UseTheme`, on `Hooks`; also inherent methods on
  `ComponentUpdater` for hand-written `Component`s):
  ```rust
  fn use_palette(&self) -> Palette;                       // owned copy
  fn use_component_theme<T: ComponentTheme>(&self) -> T;  // resolved FooTheme
  ```
- **Minimal usage**:
  ```rust
  let palette = hooks.use_palette();
  let border_theme = hooks.use_component_theme::<BorderTheme>();
  ```
- **Pitfalls**: both are **passive reads, not subscriptions** — they don't
  register a waker themselves, so reading `use_palette()` alone never causes
  a re-render on its own. Runtime theme switching works by writing the
  `Palette` into `use_state`/`Atom` state that drives `PaletteProvider`; the
  *next* render (triggered by that state write) then sees the new value.
  With no `PaletteProvider` in scope, `use_palette()` returns
  `Palette::default()` — it never panics. In a hand-written `Component`, call
  `updater.use_palette()` / `updater.use_component_theme::<T>()` in `update`
  instead — same resolve chain, no `Hooks` context-stack upgrade needed
  (unlike `use_context`, above).

## use_event_handler / use_event_handler_with_options

- **Purpose**: register a consumable input-event handler (keyboard / mouse / resize, etc.). Replaces the old `use_events` / `use_local_events`.
- **Feature**: core (depends on the input runtime).
- **Signature** (`use_input.rs`, `trait UseEventHandler`):
  ```rust
  fn use_event_handler<F>(&mut self, scope: EventScope, priority: EventPriority, f: F)
  where F: FnMut(Event) -> EventResult + 'static;

  fn use_event_handler_with_options<F>(
      &mut self, scope: EventScope, priority: EventPriority, options: EventOptions, f: F,
  ) where F: FnMut(Event) -> EventResult + 'static;
  ```
  `Event` = `crossterm::event::Event`. Related types (`input/mod.rs`):
  ```rust
  enum EventResult { Ignored /*default, keeps propagating*/, Consumed /*stops later handlers*/ }
  enum EventPriority { Low = 0, Normal = 1 /*default*/, High = 2 }  // within a layer, High delivers first
  enum EventScope {
      Current,            // inherit the nearest CurrentLayer from context, else the root layer (common for background components / Modal subtrees)
      Layer(InputLayer),  // explicitly bind to a layer (handler in a parent component; Modal opening a layer via a handle)
      Global,             // truly global, never cut off by any blocks_lower (Resize, global help keys)
  }
  struct EventOptions { pub hit_test: bool }  // true: mouse events only fire when the hit is inside the component's area
  ```
- **Minimal usage**:
  ```rust
  let mut exit = hooks.use_exit();
  hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
      let Event::Key(key) = event else { return EventResult::Ignored; };
      if key.kind == KeyEventKind::Press && matches!(key.code, KeyCode::Char('q')) {
          exit();
          return EventResult::Consumed;
      }
      EventResult::Ignored
  });
  // mouse hit-test filtering:
  hooks.use_event_handler_with_options(
      EventScope::Current, EventPriority::Normal,
      EventOptions { hit_test: true },
      move |event| { /* ... */ EventResult::Ignored });
  ```
- **Pitfalls**: the closure is `FnMut` and is re-handed to the input runtime every frame via `register_handler` (not stored across frames). Only returning `Consumed` stops later handlers. `hit_test` filters only mouse events, not keyboard. Requires a context-aware `Hooks` (a hand-written Component must first do `with_context_stack`).

## use_input_layer

- **Purpose**: declare an input layer (modal exclusivity, background blocking); returns a same-frame handle that the subtree or this component's handlers can bind to.
- **Feature**: core.
- **Signature** (`use_input.rs`, `trait UseInputLayer`):
  ```rust
  fn use_input_layer(&mut self, open: bool, blocks_lower: bool) -> InputLayer;
  ```
  When `open = true` the layer participates in dispatch this frame; when `blocks_lower = true` it sits as the active stack top and cuts off all non-`Global` handlers below it. `InputLayer` is `Copy`.
- **Minimal usage**:
  ```rust
  let edit_layer = hooks.use_input_layer(editing.get(), true);
  hooks.use_event_handler(EventScope::Layer(edit_layer), EventPriority::High, move |event| {
      if !editing.get() { return EventResult::Ignored; }
      /* ... */ EventResult::Consumed
  });
  ```
- **Pitfalls**: the returned `InputLayer` is **valid only for the current frame** (re-minted each frame; the `LayerId` is not stable across frames) — **do not store it in `use_state` for cross-frame use**; on the next frame that layer is no longer on the stack and its handlers go silently deaf. Requires a context-aware `Hooks`.

## Pointer hooks: click / hover / drag / pointer / row hover / scrollbar

Core APIs; explicitly enable `RunOptions { mouse: true }` at the entry. Read
[pointer-events.md](pointer-events.md) for signatures, filtering, capture,
coordinate semantics, and modal composition. Reference implementation:
`examples/input/mouse.rs`.

## use_insert_before

- **Purpose**: insert one-shot content **above** the terminal's main render area (e.g. log lines that scroll off-screen).
- **Feature**: core.
- **Signature** (`use_insert_before.rs`):
  ```rust
  fn use_insert_before(&mut self) -> InsertBeforeHandler;
  ```
  `InsertBeforeHandler` (`Clone`) methods:
  ```rust
  fn insert_before<F>(&self, height: u16, callback: F) -> &Self
      where F: FnOnce(&mut Buffer) + 'static;
  fn render_before<T: Widget + 'static>(&self, widget: T, height: u16) -> &Self;
  fn finish(&self);  // wakes the render loop so it processes the queued content
  ```
- **Minimal usage**:
  ```rust
  let insert_before = hooks.use_insert_before();
  insert_before.render_before(Line::from("a log line"), 1);
  insert_before.finish();
  ```
- **Pitfalls**: queued content is actually written out in `post_component_update` via `terminal().insert_before`; the handler is `Clone` and can be moved into closures.

## use_terminal_size

- **Purpose**: get the terminal's current size and have it auto-update on a terminal `Resize` (responsive layout).
- **Feature**: core.
- **Signature** (`use_size.rs`, `trait UseTerminalSize`):
  ```rust
  fn use_terminal_size(&mut self) -> (u16, u16);  // (width, height)
  ```
- **Minimal usage**:
  ```rust
  let (w, h) = hooks.use_terminal_size();
  ```
- **Pitfalls**: the method is named `use_terminal_size` (**there is no `use_size` method** — the module file is `use_size.rs` but it exports `use_terminal_size` / `use_previous_size`). Internally it uses `use_state` plus a dedicated hook that registers a `Resize` handler in `post_component_update`, obtaining the root `SystemContext` through the updater (and returning `Ignored` so multiple subscribers all receive it) — therefore a hand-written Component can use it **without** first calling `with_context_stack`.

## use_previous_size

- **Purpose**: get this component's render area (`Rect`) from the **previous frame**.
- **Feature**: core.
- **Signature** (`use_size.rs`, `trait UsePreviousSize`):
  ```rust
  fn use_previous_size(&mut self) -> Rect;  // ratatui::layout::Rect
  ```
- **Minimal usage**:
  ```rust
  let area = hooks.use_previous_size();  // the drawer.area from the previous frame's draw
  ```
- **Pitfalls**: it returns the area from the **previous frame's** draw (on the first frame this is `Rect::default()`, all zeros), because it is backfilled in `pre_component_draw`.

## use_exit

- **Purpose**: get a callback that, when called, triggers application exit.
- **Feature**: core.
- **Signature** (`use_exit.rs`):
  ```rust
  fn use_exit(&mut self) -> impl FnMut() + 'static;
  ```
- **Minimal usage**:
  ```rust
  let mut exit = hooks.use_exit();
  // inside an event-handler closure: exit();
  ```
- **Pitfalls**: internally this is `use_state(|| false)` + `use_context_mut::<SystemContext>()` — the returned closure only sets the state flag; on the **next frame** the hook body detects `state.get()` is true and then actually calls `system_ctx.exit()`. Requires a context-aware `Hooks`. Whether the returned closure is `Copy` depends on its captures (here it can be moved into an event closure, commonly as `mut exit`).

## use_on_drop

- **Purpose**: run a cleanup callback when the component unmounts.
- **Feature**: core.
- **Signature** (`use_on_drop.rs`):
  ```rust
  fn use_on_drop<F>(&mut self, f: F) where F: FnMut() + 'static;
  ```
- **Minimal usage**:
  ```rust
  hooks.use_on_drop(|| { /* clean up resources */ });
  ```
- **Pitfalls**: the callback is invoked in the hook's `on_drop`. **Do not use `State` inside the callback** (the source comment explicitly warns: when the component unmounts, the local `State`'s `Owner` may already be released). The callback is replaced every frame (`callback.replace`), so it captures the current frame's values.

---

## Router hooks (feature `router`)

All of these come from `use_router.rs`'s `trait UseRouter<'a>`, require the `router` feature, and the component must be inside a `RouterProvider` / `Outlet` subtree (they depend on `Route` / `RouteContext` / `State<RouterHistory>` and similar context).

### use_navigate

- **Purpose**: get the router navigator (like React Router's `useNavigate`).
- **Signature**:
  ```rust
  fn use_navigate(&mut self) -> Navigate;
  ```
  `Navigate` (`Clone + Copy`) methods:
  ```rust
  fn push(&mut self, path: &str);
  fn push_with_state<T: Send + Sync + 'static>(&mut self, path: &str, state: T);
  fn replace(&mut self, path: &str);
  fn replace_with_state<T: Send + Sync + 'static>(&mut self, path: &str, state: T);
  fn go(&mut self, delta: i32);   // >0 forward, <0 back
  fn back(&mut self);             // = go(-1)
  fn forward(&mut self);          // = go(1)
  ```
- **Minimal usage**:
  ```rust
  let mut navigate = hooks.use_navigate();
  navigate.push("/focus");
  navigate.push_with_state("/detail", RouteNotice { message: "hi".into() });
  navigate.back();
  ```
- **Pitfalls**: `push` / `replace` (without state) clears `ctx.state` (does not carry over the previous route state).

### use_route

- **Purpose**: get the current route information.
- **Signature**:
  ```rust
  fn use_route(&self) -> Ref<'a, Route>;  // std::cell::Ref<Route>
  ```
- **Pitfalls**: underneath this is `use_context::<Route>()`; calling it outside a router subtree panics.

### use_params

- **Purpose**: get the current route's dynamic-parameter map.
- **Signature**:
  ```rust
  fn use_params(&self) -> Ref<'a, HashMap<String, String>>;
  ```
- **Minimal usage**:
  ```rust
  let slug = {
      let params = hooks.use_params();
      params.get("slug").cloned().unwrap_or_default()
  };
  ```
- **Pitfalls**: returns a `Ref` guard (borrowed from `RouteContext`) — `cloned()` the value out and release the guard as early as possible to avoid borrow conflicts with other context.

### use_route_state / try_use_route_state

- **Purpose**: read the route state carried by `push_with_state` / `replace_with_state`.
- **Signature**:
  ```rust
  fn use_route_state<T: Send + Sync + 'static>(&self) -> Arc<T>;          // panics if missing / type mismatch
  fn try_use_route_state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>>;
  ```
- **Minimal usage**:
  ```rust
  let notice = hooks.try_use_route_state::<RouteNotice>()
      .map(|s| s.message.clone())
      .unwrap_or_else(|| "opened without route state".into());
  ```
- **Pitfalls**: returns `Arc<T>`; the type must exactly match the type passed at navigation time, otherwise the `downcast` fails (the `try_` version returns `None`, the non-`try_` version panics).

---

## Global state Atom (feature `atom`)

### use_atom

- **Purpose**: subscribe to a process-level global atom from within a component, returning a `Copy + Send` handle.
- **Feature**: `atom`.
- **Signature** (`atom/use_atom.rs`, `trait UseAtom`):
  ```rust
  fn use_atom<T>(&mut self, atom: &'static Atom<T>) -> AtomState<T>
  where T: Unpin + Send + Sync + 'static;
  ```
  `Atom<T>` (`atom/mod.rs`) is declared at module level and can be read / written even outside components:
  ```rust
  pub const fn new(init: fn() -> T) -> Self;   // usable as a static
  pub fn state(&self) -> AtomState<T>;         // lazily create and get a handle
  pub fn get(&self) -> T where T: Copy;        // read directly outside a component
  pub fn set(&self, value: T);                 // write directly outside a component (wakes subscribers)
  ```
  The returned `AtomState<T>` has all the methods and operators from the "State<T> / AtomState<T> capabilities" section above (`get` / `set` / `read` / `write`, `+=` / `-=`, etc.).
  Note: `AtomState::new(value)` (atom feature) takes a **value**, not a closure — you rarely need it; prefer `Atom::new(|| value)` for the static and `use_atom` / `Atom::state()` to obtain handles.
- **Minimal usage**:
  ```rust
  static SCORE: Atom<i32> = Atom::new(|| 0);

  #[component]
  fn ScoreCard(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
      let mut score = hooks.use_atom(&SCORE);   // subscribe; this component re-renders on SCORE writes
      // read: score.get()   write + wake all subscribers: score += 1; / score.set(0);
      element!( Text(text: Line::from(format!("{:02}", score.get()))) )
  }

  // outside a component / in a background task:
  // SCORE.set(5);  or  let h = SCORE.state(); tokio::spawn(async move { h.set(9); });
  ```
- **Pitfalls**:
  - **The parameter must be synced every frame**: `use_atom` calls `hook.set_state(state)` each frame; when the `atom` parameter changes it removes the old waker subscription and subscribes to the new atom; on unmount its `on_drop` cleans up this component's waker. When you write your own parameterized hook, sync the parameter on later frames the same way — do not rely solely on the first-frame initialization of `use_hook(|| ...)`.
  - `Atom` must be `&'static` (a module-level `static`). The underlying handle is lazily inserted into the process-level global `OWNER` on the first `use_atom` / `get` / `set`.
  - `AtomState<T>` uses `WakerMap` (multiple subscribers keyed by `ElementKey`), so a write wakes **all** subscribing components; the handle is `Send` and can be moved into `tokio::spawn` for background updates.
