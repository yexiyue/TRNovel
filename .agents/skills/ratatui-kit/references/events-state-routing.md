# Events, State & Routing Reference

What this file covers: the central `InputRuntime` keyboard/mouse event system, the two state systems (`use_state` local vs `Atom`/`use_atom` global), and the `router` feature (`routes!`, `RouterProvider`, `Outlet`, navigation/params/route-state hooks). This is the source of truth for generating correct `ratatui-kit` code in these three areas — every prop name, type, and enum variant here is real Rust API.

All examples assume `use ratatui_kit::prelude::*;`. The `prelude` re-exports `crate::input::*` (so `EventScope`, `EventPriority`, `EventResult`, `EventOptions`, `InputLayer` are in scope), all hook traits, `crossterm`, `ratatui`, and the procedural macros. `crossterm::event::{Event, KeyCode, KeyEventKind}` still need an explicit import: `use ratatui_kit::crossterm::event::{Event, KeyCode, KeyEventKind};`. (`prelude::*` brings the library's own `Event` re-export and `EventScope` etc., but `KeyCode`/`KeyEventKind` come from `crossterm` and must be imported separately.) Multi-span lines need `Span` alongside `Line`: `use ratatui::text::{Line, Span};` (a bare `Line::from(...)` of one string needs no extra import, but the moment you build a `Line` from several styled `Span`s you must import `Span`).

## Table of contents

1. [Event system (central InputRuntime + input layers)](#1-event-system-central-inputruntime--input-layers)
2. [State: `use_state` (local) vs `Atom`/`use_atom` (global, `atom` feature)](#2-state-use_state-local-vs-atomuse_atom-global-atom-feature)
3. [Routing (`router` feature)](#3-routing-router-feature)

---

## 1. Event system (central InputRuntime + input layers)

The old broadcast-subscribe model (`use_events` / `use_local_events`) has been removed. The current model is a single raw event source feeding a central `InputRuntime`. Each frame calls `begin_frame` to clear layers and handlers, and components re-register during their `update` pass.

### 1.1 Enums and types (verbatim from `src/input/mod.rs`)

```rust
// Handler return value. Default = Ignored.
pub enum EventResult {
    Ignored,   // not consumed; keep delivering to later handlers
    Consumed,  // consumed; stop propagation (early-out)
}

// Delivery priority. Within one layer, High runs before Normal before Low. Default = Normal.
pub enum EventPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

// Handler ownership scope.
pub enum EventScope {
    Current,              // inherit the nearest input layer on the context stack; falls back to the root layer (most common)
    Layer(InputLayer),    // explicitly bind to a given layer (used when a parent opens a layer for a child Modal)
    Global,               // truly global; never cut off by any blocks_lower (Resize, global help keys)
}

// Registration options.
pub struct EventOptions {
    pub hit_test: bool,   // when true, mouse events only fire if they land inside the handler's component area
}

// Input layer handle (Copy). Returned by use_input_layer, valid only within the same frame; never store it in use_state across frames.
pub struct InputLayer { /* pub(crate) id */ }
```

Note the exact variant names: `EventResult::Ignored` / `EventResult::Consumed` (not `Handled`/`Pass`), `EventPriority::{Low, Normal, High}`, `EventScope::{Current, Layer, Global}`.

### 1.2 Hook signatures (verbatim from `src/hooks/use_input.rs`)

```rust
pub trait UseEventHandler: private::Sealed {
    fn use_event_handler<F>(&mut self, scope: EventScope, priority: EventPriority, f: F)
    where
        F: FnMut(Event) -> EventResult + 'static;

    fn use_event_handler_with_options<F>(
        &mut self,
        scope: EventScope,
        priority: EventPriority,
        options: EventOptions,
        f: F,
    ) where
        F: FnMut(Event) -> EventResult + 'static;
}

pub trait UseInputLayer: private::Sealed {
    // open=true: participate in dispatch this frame; blocks_lower=true: act as the active stack top and cut off lower layers (modal exclusivity).
    fn use_input_layer(&mut self, open: bool, blocks_lower: bool) -> InputLayer;
}
```

The closure signature is `FnMut(Event) -> EventResult`. It receives an **owned** `Event` (by value); you can match on it directly (`let Event::Key(key) = event else { ... }`) or borrow it (`&event`).

### 1.3 Standard keyboard-handler shape (from `examples/start/counter.rs`)

```rust
use ratatui_kit::{
    crossterm::event::{Event, KeyCode, KeyEventKind},
    prelude::*,
};

#[component]
fn Counter(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut count = hooks.use_state(|| 0_u64);
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            count += 1;
        }
    });

    let mut exit = hooks.use_exit();
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        {
            exit();
            return EventResult::Consumed;
        }
        EventResult::Ignored
    });

    element!(/* ... render count.get() ... */)
}
```

Idiomatic text-input forwarding (from `examples/components/input.rs`): first `let Event::Key(key) = &event else { return EventResult::Ignored; };`, then `if key.kind != KeyEventKind::Press { return EventResult::Ignored; }`, then `match key.code { ... }`. Forward unrecognized keys to the underlying widget with `input.write().handle_event(&event);` (where `input` is `use_state(tui_input::Input::default)`, and you need `use ratatui_kit::prelude::tui_input::backend::crossterm::EventHandler;`), then `return EventResult::Consumed`.

```rust
hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
    let Event::Key(key) = &event else {
        return EventResult::Ignored;
    };
    if key.kind != KeyEventKind::Press {
        return EventResult::Ignored;
    }

    match key.code {
        KeyCode::Esc => {
            if input.read().value().is_empty() {
                exit();
            } else {
                input.write().reset();
                status.set("cleared draft".to_string());
            }
            EventResult::Consumed
        }
        KeyCode::Enter => {
            // ... commit submitted value ...
            EventResult::Consumed
        }
        _ => {
            input.write().handle_event(&event); // forward to tui_input::Input
            status.set("editing draft".to_string());
            EventResult::Consumed
        }
    }
});
```

Consuming all keys to forward them to `tui_input::Input` means global command keys (`q` quit, etc.) are DEAD while the field is focused — the user must Esc out first. If you want command keys to stay live alongside typing, don't hand-roll this — use `SearchInput`, whose `activate_key` gates an input layer so the background keeps receiving commands.

### 1.4 Priority and layering semantics (determines whether generated code is correct)

- Dispatch runs in two phases: **Phase 1 Global** (`scope = Global`) runs first, ordered by `(priority desc, order asc)`, and any `Consumed` terminates the entire dispatch. **Phase 2 in-layer** is ordered by `(layer z-order desc, priority desc, order asc)`.
- **z-order is the primary sort key; priority is never compared across layers**: a lower layer's `High` cannot beat an upper layer's `Normal`.
- A layer with `blocks_lower = true` cuts off all non-`Global` handlers below it, from the stack top downward (modal exclusivity over input).
- Parents register before children (top-down registration; `order` ascending breaks ties). `EventPriority::High` (as in `ProjectsPage` in `examples/routing/router.rs`) lets that component receive events before its parent shell within the same layer.
- A persistent shell/parent that shares an input layer with child pages (or with a `Select`) must return `EventResult::Ignored` for every key it does not own — parents are dispatched before children in the same layer, so a blanket `_ => Consumed` swallows the children's `j`/`k`/`Enter`. Give a child that must pre-empt the shell `EventPriority::High`.
- The `InputLayer` returned by `use_input_layer` is valid only **within the same frame**; never hold it across frames in `use_state`. Use it to pass the handle to a child `Modal` (`layer` prop) or to this component's own `EventScope::Layer(handle)`.
- A hand-written `Component` (not a `#[component]` function component) must call `let mut hooks = hooks.with_context_stack(updater.component_context_stack());` before using these hooks. Function components are upgraded automatically by the macro and work out of the box.

### 1.5 Custom Ctrl+C behavior

Ctrl+C exits the render loop before event dispatch by default. Keep that default
for ordinary applications. When the application needs to cancel an agent, ask for
confirmation, or require a second press to quit, disable the automatic exit from
the root component and handle Ctrl+C through a `Global` handler:

```rust
use ratatui_kit::{
    crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers},
    prelude::*,
};

#[component]
fn App(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut cancel_requested = hooks.use_state(|| false);

    // SystemContext is shared by the whole tree. Release its RefMut before
    // registering handlers or rendering children.
    {
        let mut system = hooks.use_context_mut::<SystemContext>();
        system.set_auto_quit_on_ctrl_c(false);
    }

    hooks.use_event_handler(EventScope::Global, EventPriority::High, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            cancel_requested.set(true);
            return EventResult::Consumed;
        }
        EventResult::Ignored
    });

    element!(/* render from cancel_requested.get(), or trigger app logic */)
}
```

The setter is persistent state on `SystemContext`; call it from the root component
on every update so the intended policy stays explicit. If the policy is conditional,
set both branches (`set_auto_quit_on_ctrl_c(!custom_ctrl_c)`) rather than only ever
setting `false`. Once automatic exit is disabled, Ctrl+C follows normal dispatch:
Global handlers run first, and `Ignored` allows the event to continue into active
layer handlers. Return `Consumed` when the global action owns the key.

---

### 1.6 Pointer (mouse) delivery semantics

Read [pointer-events.md](pointer-events.md) before implementing mouse behavior.
It covers entry options, mouse-only reverse draw ordering, active-layer hover,
per-button capture, and the `mouse` modal example. Keyboard ordering above stays
unchanged.


## 2. State: `use_state` (local) vs `Atom`/`use_atom` (global, `atom` feature)

Both are backed by `ReactiveHandle<T, N>` (`src/reactive_handle.rs`), both are `Copy`, and both wake the render loop on write.

### 2.1 Handle methods common to `State<T>` and `AtomState<T>`

```rust
state.get()                 // T: Copy — read by value
state.read()                // ReactiveRef<T> (Derefs to &T); use for non-Copy types
state.write()               // ReactiveMutRef<T> (DerefMut; on drop, notifies if a deref_mut occurred)
state.write_no_update()     // write without notifying; also try_read/try_write/try_write_no_update return Option
state.set(value)            // &mut self — overwrite and notify
state.set_no_update(value)  // overwrite without notifying
// Operator overloads (T: Copy + the matching arithmetic trait):
state += 1;  state -= 1;  state *= 2;  state /= 2;   // *Assign forms trigger a notification
// state + 1 / - / * / return T (no write-back). Also implements PartialEq<T>/PartialOrd<T>, Display/Debug/Hash.
```

### 2.2 Local state: `use_state` (`src/hooks/use_state.rs`)

```rust
fn use_state<T, F>(&mut self, init: F) -> State<T>
where
    F: FnOnce() -> T,
    T: Unpin + Send + Sync + 'static;
```

Each component gets its own `Owner`, released when the component unmounts. `State<T> = ReactiveHandle<T, SingleWaker>`.

```rust
let mut count = hooks.use_state(|| 0_u64);            // Copy type
count += 1;                                            // triggers a re-render
let history = hooks.use_state(Vec::<String>::default); // non-Copy
history.write().insert(0, item);                      // .write()/.read()
let snapshot = history.read().clone();
```

### 2.3 Global atom: `Atom` / `use_atom` (`src/atom/mod.rs`, requires the `atom` or `full` feature)

`Atom<T>` is built with `const fn new(init: fn() -> T)`, so it can be a module-level `static`. Its backing `AtomState<T> = ReactiveHandle<T, WakerMap>` is lazily inserted into the process-global `OWNER` on first access.

Declaration plus in-component subscription (real snippet from `examples/hooks/atom_state.rs`):

```rust
// Module-level declaration: no macros, no structs. init must be a non-capturing fn pointer.
static FOCUS: Atom<String> = Atom::new(|| "Review runtime".to_string());
static SCORE: Atom<i32>    = Atom::new(|| 2);

#[component]
fn ScoreCard(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let score = hooks.use_atom(&SCORE);   // subscribe: register this component's waker, return a Copy + Send handle
    element!(
        Text(text: Line::from(format!("{:02}", score.get())))
    )
}

#[component]
fn ScoreEditor(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut score = hooks.use_atom(&SCORE);  // a different component subscribing to the same Atom

    // inside an event handler:
    // score += 1;        // a write wakes only components subscribed to this atom (fine-grained)
    // score.set(0);
    // score.get();
    // reading is the same: let focus = hooks.use_atom(&FOCUS); focus.read().to_string();
    element!(/* ... */)
}
```

`use_atom` signature:

```rust
fn use_atom<T>(&mut self, atom: &'static Atom<T>) -> AtomState<T>
where
    T: Unpin + Send + Sync + 'static;
```

Outside components / in background tasks you can call `FOCUS.get()` (`T: Copy`) / `FOCUS.set(v)` / `FOCUS.state()` directly, or `move` the `Copy + Send` handle returned by `use_atom` into `tokio::spawn` to update it from the background. `use_atom` automatically cleans up the old waker when the atom argument changes or the component unmounts (`on_drop`).

Summary comparison: `use_state` is per-component, released on unmount, `SingleWaker`. `Atom` is a process-level `static`, lazily a singleton, `WakerMap` with multiple subscribers, and writes wake only subscribers.

| | `use_state` | `Atom` / `use_atom` |
|---|---|---|
| Scope | per-component | process-global `static` |
| Lifetime | released on unmount | lazy singleton, lives for the process |
| Waker | `SingleWaker` | `WakerMap` (multi-subscriber) |
| On write | re-renders the owning component | wakes only subscribed components |
| Feature gate | none | `atom` (or `full`) |

---

## 3. Routing (`router` feature)

Requires the `router` or `full` feature. The flow: `routes!` builds the route table → `RouterProvider` owns the history stack → `Outlet` renders the currently matched page → `use_navigate` / `use_route` / `use_params` / `use_route_state`.

### 3.1 `routes!` macro + `RouterProvider` (from `examples/routing/router.rs`)

```rust
let routes = routes! {
    "/" => AppShell {                            // nested routes: parent shell + child pages (rendered inside AppShell's Outlet)
        "/" => OverviewPage,
        "/projects/:slug" => ProjectDetailPage,  // dynamic param segment :slug
        "/projects" => ProjectsPage,
        "/activity" => ActivityPage,
    },
};

element!(RouterProvider(
    routes: routes,
    index_path: "/",          // default home path (String; string literals auto-convert)
    // history_length: 10,    // optional: Option<usize>; passing a bare usize auto-wraps to Some, defaults to 10
    // state: ...,            // optional: Option<RouteState>, the initial route state
))
.fullscreen()
.await?;
```

`routes!` supports `element!`-style `Comp(prop: val)` props syntax on the right-hand side, e.g. `"/hi" => Greet(name: "world".to_string())`, and these may also carry a child-route block. Declare static routes before dynamic routes sharing the same prefix (`Outlet` takes the first match in declaration order). `RouterProviderProps` fields: `routes: Routes`, `index_path: String`, `history_length: Option<usize>`, `state: Option<RouteState>`.

### 3.2 `Outlet` — placing the child-page render slot inside a shell

```rust
#[component]
fn AppShell(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    // ... navigation handler ...
    element!(
        Border( /* ... */ ) {
            View(flex_direction: Direction::Horizontal, gap: 2) {
                Border(/* sidebar */) { /* ... */ }
                View(width: Constraint::Fill(1)) {
                    Outlet           // the currently matched child route renders here; the shell stays mounted
                }
            }
        }
    )
}
```

### 3.3 Hook signatures (`src/hooks/use_router.rs`)

```rust
pub trait UseRouter<'a>: private::Sealed {
    fn use_navigate(&mut self) -> Navigate;
    fn use_route(&self) -> Ref<'a, Route>;
    fn use_params(&self) -> Ref<'a, HashMap<String, String>>;
    fn try_use_route_state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>>;
    fn use_route_state<T: Send + Sync + 'static>(&self) -> Arc<T>;  // panics if missing / type mismatch
}
```

`Navigate` (`Clone + Copy`) methods:

```rust
navigate.push(path: &str)
navigate.push_with_state<T: Send + Sync + 'static>(path: &str, state: T)
navigate.replace(path: &str)
navigate.replace_with_state<T: Send + Sync + 'static>(path: &str, state: T)
navigate.go(delta: i32)   // >0 forward, <0 back
navigate.back()           // = go(-1)
navigate.forward()        // = go(1)
```

### 3.4 Navigation + params + route state (real combination, from `examples/routing/router.rs`)

Navigation (inside an event handler):

```rust
let mut navigate = hooks.use_navigate();
// inside the closure:
navigate.push("/projects");
navigate.back();
navigate.forward();
navigate.replace("/");
// navigate with state:
navigate.push_with_state(
    &format!("/projects/{}", project.slug),
    RouteNotice { message: format!("selected: {}", project.name) },
);
```

Reading dynamic params + optional route state (`ProjectDetailPage`):

```rust
#[component]
fn ProjectDetailPage(hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let slug = {
        let params = hooks.use_params();            // Ref<HashMap<String, String>>
        params.get("slug").cloned().unwrap_or_default()
    };
    // optional route state: the custom type passed to push_with_state, downcast by type
    let notice = hooks
        .try_use_route_state::<RouteNotice>()       // Option<Arc<RouteNotice>>
        .map(|state| state.message.clone())         // clone the field out — you cannot move it out of the Arc
        .unwrap_or_else(|| "opened without route state".to_string());

    element!(/* ... use slug / notice ... */)
}
```

`use_route_state::<T>()` is the panicking variant (use it when presence is guaranteed); `use_route()` returns `Ref<Route>` for current route info. Both `try_use_route_state` and `use_route_state` return `Arc<T>` (`try_` wraps it in `Option`); clone the fields you need out of it (`state.field.clone()`) — you cannot move fields out of the `Arc`. `RouteState::new::<T>(state)` (`T: Any + Send + Sync`) / `.downcast::<T>() -> Option<Arc<T>>` is the underlying type-erased container. A dynamic segment `/:name` matches a single segment only (`[^/]+`, does not cross `/`); the remaining path is passed down to nested `Outlet`s.

---

## Key file paths

- Event runtime: `crates/ratatui-kit/src/input/mod.rs`
- Event hooks: `crates/ratatui-kit/src/hooks/use_input.rs`
- Local state: `crates/ratatui-kit/src/hooks/use_state.rs`
- Reactive handle (operators / read-write): `crates/ratatui-kit/src/reactive_handle.rs`
- Global atom: `crates/ratatui-kit/src/atom/mod.rs` + `crates/ratatui-kit/src/atom/use_atom.rs`
- Router hooks: `crates/ratatui-kit/src/hooks/use_router.rs`
- Router components / Route / Routes / RouteState: `crates/ratatui-kit/src/components/router/mod.rs` + `router_provider.rs` + `outlet.rs`
- Examples: `examples/start/counter.rs`, `examples/components/input.rs`, `examples/hooks/atom_state.rs`, `examples/routing/router.rs`
