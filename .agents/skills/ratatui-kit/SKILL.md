---
name: ratatui-kit
description: >-
  Build polished, component-driven terminal UIs (TUIs) in Rust with the
  ratatui-kit framework — React-style components, hooks, props, the element!
  macro, routing, and global state, on top of Ratatui + Tokio. Use this skill
  whenever the user wants to build, scaffold, redesign, or extend a terminal /
  console / CLI user interface, dashboard, wizard, picker, or interactive
  command-line app in Rust, OR mentions ratatui-kit, the element! macro,
  #[component], use_state / use_atom / use_event_handler, RouterProvider, or asks
  to "make a TUI", "build a terminal app", "add a screen/panel/widget", or "wire
  up keyboard or mouse input" in a ratatui-kit project — even if they don't name the
  framework explicitly. Reach for it before writing ratatui-kit code by hand, so
  the output uses correct APIs and idioms instead of guessed ones.
license: MIT
metadata:
  author: yexiyue
  framework: ratatui-kit
  version: "1.1.0"
---

# ratatui-kit — Building Terminal UIs in Rust

[ratatui-kit](https://github.com/yexiyue/ratatui-kit) is a component framework for
terminal UIs, built on [Ratatui](https://ratatui.rs) and [Tokio](https://tokio.rs).
If you know React, the mental model transfers almost 1:1: `element!` is JSX,
`#[component]` is a function component, `use_state` is `useState`, `use_future` is
an async effect, props and context work the same way, and there is a built-in
router and global store.

**This framework is niche — do not generate its code from memory.** The macro
syntax, component props, hook signatures, and the input system are specific and
easy to get subtly wrong. Use the minimal template below plus the reference files
in `references/` as your source of truth. A wrong prop name or a hook in the wrong
place is a hard compile error, not a warning.

**Companion skills.** This skill covers the *framework* and assumes general Rust
competence — it does not re-teach ownership, `mut` bindings, borrow rules, or async.
Install **`rust-best-practices`** and **`rust-async-patterns`** alongside it; they
catch the general-Rust mistakes (e.g. a missing `mut` on a mutated binding) that
aren't specific to ratatui-kit. And whatever else you do, **compile before you call
it done** (see *Verifying*) — `cargo check` names every such trivial error precisely.

---

## The build workflow

When asked to build, scaffold, or extend a ratatui-kit UI, work in this order:

1. **Establish the project.**
   - New project: `cargo new <name>`, then add deps (see *Project setup* below).
     Decide which **features** you need — components are feature-gated and won't
     compile if the feature is off. When in doubt, start with `features = ["full"]`.
   - Existing project: check `Cargo.toml` for `ratatui-kit` and which features are
     enabled before using a gated component.

2. **Plan the component tree.** Sketch the screens and panels as `#[component]`
   functions. Decide **state ownership**: ephemeral, per-component state →
   `use_state`; state shared across distant components or pages → an `Atom`
   (`atom` feature). Multi-screen app → `RouterProvider` + `routes!`.

3. **Compose with `element!`.** Build each component's tree. Remember function
   components use **transparent layout**: put layout props (`width`, `height`,
   `flex_direction`, `gap`, …) on the **root element you return**, not on the call
   site of a function component.

4. **Wire input and lifecycle.** Register keyboard handlers with
   `use_event_handler`; use semantic pointer hooks for mouse interactions. Get the quit callback
   from `use_exit`. Ctrl+C exits through the framework by default; applications
   that need cancellation or double-press exit can explicitly route it through
   the central event dispatcher.

5. **Verify by compiling — this is the definition of done.** Run `cargo check` with
   the features you used, read the errors, and fix them; iterate check → fix → check
   until clean. Most first-compile failures are trivial and the compiler names them
   precisely (a missing `mut` binding, a wrong prop name, an un-enabled feature).
   See *Verifying* at the end. Examples in the repo (`examples/`) are runnable
   references for any pattern.

For anything beyond the essentials on this page, open the matching reference:

| You're working on… | Read |
| --- | --- |
| Which component to use, its exact props/types | `references/components.md` |
| Hook signatures, `State`/`AtomState` methods & operators | `references/hooks.md` |
| `element!` grammar, `#[component]`, `#[derive(Props)]`, `routes!`, adapters | `references/syntax-and-macros.md` |
| Click / hover / drag, pointer capture, Button, mouse setup | `references/pointer-events.md` |
| Keyboard events, input layers, state, routing | `references/events-state-routing.md` |
| App skeleton, layout & visual polish, interactive patterns, pitfalls | `references/building-polished-uis.md` |
| Every `Palette`/`*Theme` field, custom `ComponentTheme`, preset themes | `references/theming.md` |

---

## Project setup

`ratatui-kit` ships **zero features by default**; enable what you use. An async
runtime is required — the docs and examples use Tokio.

```toml
[dependencies]
ratatui-kit = { version = "0.12", features = ["full"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
```

> **New crate:** set `edition = "2024"` in `[package]` (the crate is edition 2024,
> so an older edition is rejected). If the crate sits inside or beside another Cargo
> workspace, add an empty `[workspace]` table to its `Cargo.toml` so Cargo doesn't
> attach it to the enclosing workspace.

| Feature | Unlocks |
| --- | --- |
| `router` | `RouterProvider` / `Outlet`, `routes!`, `use_navigate` / `use_route` / `use_params` |
| `atom` | `Atom` / `AtomState` / `use_atom` (process-global state) |
| `input` | `Input` / `SearchInput` (single-line text entry) |
| `tree` | `TreeSelect` |
| `table` | `Table` (data grid: cell-grid borders, wrapping, row/column highlight, footer) |
| `virtual-list` | `VirtualList` (windowed list for huge data) |
| `textarea` | `TextArea` (multi-line text entry with soft wrap) |
| `full` | all of the above |

Core components need **no feature**: `View`, `Border`, `Center`, `Fragment`,
`Text`, `WrappedText`, `Positioned`, `Modal`, `ConfirmModal`, `AlertModal`,
`ShortcutInfoModal`, `Button`, `Select`, `MultiSelect`, `ScrollView`, `ContextProvider`.

> `TextArea` is a full redesign on `tui-input` (not tui-textarea): soft-wrapped
> display rows, display-row Up/Down/Home/End, click-to-position (cursor stops
> before the clicked cell), `on_submit` fires on Enter in single-line mode.
> Value + cursor are caller-owned: feed `value` back through `on_change`.

Standard imports — `prelude::*` for framework items, `ratatui::` for styles/layout
types, `crossterm::event::` for key types:

```rust
use ratatui_kit::{
    crossterm::event::{Event, KeyCode, KeyEventKind},
    prelude::*,
    ratatui::{
        layout::{Constraint, Direction, Flex},
        style::{Color, Style, Stylize},
        text::Line,
    },
};
```

When you build multi-span styled lines, import `Span` too (`text::{Line, Span}`)
rather than writing the fully-qualified path: `Line::from(vec![Span::styled("label ",
Style::new().dark_gray()), Span::styled(value, Style::new().yellow())])`.

---

## Minimal app (copy this as your starting point)

This compiles and runs as-is. It shows the five things every app needs: an entry
point, a component, local reactive state, an async side effect, and a keyboard
handler with a quit key.

```rust
use ratatui_kit::{
    crossterm::event::{Event, KeyCode, KeyEventKind},
    prelude::*,
    ratatui::{
        layout::{Constraint, Direction, Flex},
        style::{Style, Stylize},
        text::Line,
    },
};

#[tokio::main]
async fn main() {
    element!(App)
        .fullscreen()                 // take over the whole terminal (alt screen)
        .await
        .expect("Failed to run the application");
}

#[component]
fn App(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut count = hooks.use_state(|| 0_u64);
    let mut exit = hooks.use_exit();

    // Async side effect: writing state wakes the render loop — no manual redraw.
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            count += 1;               // State<T> overloads += and notifies on write
        }
    });

    // Standard keyboard-handler shape: destructure Key → check Press → match code.
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else { return EventResult::Ignored };
        if key.kind != KeyEventKind::Press { return EventResult::Ignored }
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => { exit(); EventResult::Consumed }
            _ => EventResult::Ignored,
        }
    });

    element!(
        Center(width: Constraint::Length(48), height: Constraint::Length(9)) {
            Border(
                flex_direction: Direction::Vertical,
                justify_content: Flex::Center,
                border_style: Style::new().cyan(),
                top_title: Line::from(" ratatui-kit ").cyan().bold().centered(),
                bottom_title: Line::from(" q quit · Ctrl+C exit ").dark_gray().centered(),
            ) {
                Text(text: Line::styled(
                    format!("Counter: {:02}", count.get()),
                    Style::new().green().bold(),
                ).centered())
            }
        }
    )
}
```

There is also a non-fullscreen mode if you want the UI to share the scrollback,
but `.fullscreen().await` is the default for an app.

---

## Mental model (the parts that change the code you write)

- **Declarative & re-created each frame.** `element!` produces a lightweight tree
  description; the framework reconciles it against the live component tree. You
  describe *what the UI should be*, never imperatively draw.

- **Reactive, not imperative.** The UI only re-renders when **reactive state**
  changes: `use_state`'s `State<T>`, a global `AtomState<T>`, async writes, or a
  terminal event. A plain local variable will **not** trigger a redraw — anything
  the user should see must live in `use_state` or an `Atom`.

- **`#[component]` + transparent layout.** A `#[component] fn` becomes a component
  whose body runs each frame and returns one root element. Function components do
  **not** occupy their own layout node — they inherit the `LayoutStyle` of the
  root element they return. So layout props belong on that returned root, not on
  the tag where you *use* the component.

- **Reconciliation preserves state by identity.** A node is reused across frames
  when its `key` **and** component type match the previous frame — and reuse means
  its hooks/state survive. Give every element in a `for` loop a **stable `key:`**.

- **Hook order must be stable.** Hooks are indexed by call order. Never put a hook
  (`use_state`, `use_future`, `use_event_handler`, `use_atom`, …) inside an `if`,
  `for`, `match`, or early `return` — call them unconditionally at the top of the
  component body, or you get a "Hook type mismatch" panic.

---

## `element!` essentials

`element!` is JSX-like. A node is `Component(prop: value, ...) { children }`. Prop
values are auto-wrapped with `.into()`, so `text: "hi"`, `top_title: Line::from(..)`
(an `Option` field — auto-`Some`), and `wrap: true` all work. Omitted props fall
back to `Default`, so **every Props type must be `Default`**.

```rust
element!(
    View(flex_direction: Direction::Vertical, gap: 1) {
        Text(text: "first line")
        Text(text: "second line", style: Style::new().dark_gray())
    }
)
```

**Control flow is first-class in the children block** — `if` / `if let` / `for` /
`match` work directly, and different branches may return different element types:

```rust
element!(
    Border(flex_direction: Direction::Vertical) {
        if items.is_empty() {
            Text(text: "Nothing here", style: Style::new().yellow())
        } else {
            for (i, item) in items.iter().enumerate() {
                View(key: i, height: Constraint::Length(1)) {   // stable key in loops
                    Text(text: item.clone())
                }
            }
        }
    }
)
```

`{ expr }` embeds any Rust expression returning an `Element`, `Option<Element>`,
`Vec<AnyElement>`, or `impl Iterator<Item = Element>`. Bridge any native Ratatui
widget with `widget(expr)` (stateless) or `stateful(widget, state)` — your escape
hatch when no built-in component fits. Full grammar: `references/syntax-and-macros.md`.

---

## Handling input

The standard handler shape (used by every example) — destructure the key, gate on
`Press`, `match` the code, return `Consumed` when you handled the key and `Ignored`
otherwise so other handlers/layers still get a chance:

```rust
hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
    let Event::Key(key) = event else { return EventResult::Ignored };
    if key.kind != KeyEventKind::Press { return EventResult::Ignored }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => { /* move down */ EventResult::Consumed }
        KeyCode::Char('k') | KeyCode::Up   => { /* move up   */ EventResult::Consumed }
        KeyCode::Char('q')                 => { exit(); EventResult::Consumed }
        _ => EventResult::Ignored,
    }
});
```

Modals and search boxes that must **capture** input (so the background stops
reacting) use an **input layer**. Prefer the built-in `ConfirmModal` / `AlertModal`
/ `ShortcutInfoModal`, which manage their own layer — you only pass `open` + a
callback. Roll your own layer only for custom modal interactions; see
`references/events-state-routing.md` for `use_input_layer` and the layering rules.

**Mouse integration.** Read [references/pointer-events.md](references/pointer-events.md)
before adding pointer behavior. Enable `RunOptions { mouse: true }` at the entry;
these APIs are core and require no feature flag. Choose the API by ownership:

- Built-in action → `Button` (left Down); release-to-activate → `use_click`.
- Region feedback/resizing → `use_hover` / `use_drag`; custom dispatch → `use_pointer`.
- Third-party/custom list → `use_hover_row(hit)`; custom scrollable content →
  `use_scrollbar(ScrollbarOptions, on_seek)` with one content unit throughout.
- ScrollView and TreeSelect scrollbars already support drag, track jump and arrow
  steps. Configure `ScrollbarStyle` once, including hover and drag feedback.

Keep hooks stable when props change; guard callbacks instead. A hand-written
`Component::update` must attach the updater's context stack before hooks.
`PointerEvent.position` and `area` use drawing coordinates; `screen_position`,
raw events, `Click.position`, and drag positions use terminal coordinates. Let
nested ScrollViews map and clip hits. Pass an explicit same-frame
`PointerOptions.layer` for editing/modal handlers. Handle drag cancellation by
clearing transient feedback. For custom lists, reject loading/empty/out-of-range
hits, release read guards before writing, and write hover state only on change.
Button does not provide keyboard focus or activation; register shortcuts for the
same business action separately.

**Custom Ctrl+C behavior.** The default remains immediate framework exit. To
implement agent cancellation, confirmation, or double-press exit, call
`SystemContext::set_auto_quit_on_ctrl_c(false)` from the root component and
register a `Global` handler for Ctrl+C. Return `Consumed` after handling it so the
event does not continue into the active layer. The exact borrow-safe pattern is in
`references/events-state-routing.md` §1.5.

**Text field that coexists with command keys.** When the user types into a field
*and* single-letter keys (`j`/`k`/Space/`q`) are commands, reach for **SearchInput**
— its `activate_key` opens an exclusive input layer while editing, so background
command keys stay live and you avoid a global Editing/Navigating mode. Build a manual
two-mode state machine, or forward every key to a raw `Input`, only when the whole
screen is a form with no competing single-letter commands.

**Shared-layer ordering.** A persistent shell/parent that shares an input layer with
child pages (or a `Select`) must return `EventResult::Ignored` for every key it
doesn't own — parents are dispatched before children in the same layer, so a blanket
`_ => Consumed` in the shell swallows the children's `j`/`k`/`Enter`. Give a child
that must pre-empt the shell `EventPriority::High`.

---

## State

- **Local:** `let mut count = hooks.use_state(|| 0);` → a `Copy` handle. Read with
  `count.get()` (for `Copy` types) or `count.read()`; write with `count.set(v)`,
  `count.write()`, or operators (`count += 1` notifies on write). Non-`Copy` data:
  `let list = hooks.use_state(Vec::new); list.write().push(x); let snap = list.read().clone();`

- **Global** (`atom` feature): declare a module-level `static`, subscribe in any
  component. Writes wake **only** the components subscribed to that atom.

  ```rust
  static SCORE: Atom<i32> = Atom::new(|| 0);

  #[component]
  fn ScoreCard(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
      let mut score = hooks.use_atom(&SCORE);   // subscribe
      // score.get(), score += 1, score.set(0) — all notify subscribers
      element!(Text(text: Line::from(format!("score: {}", score.get()))))
  }
  ```

**Binding mutability (framework API detail).** `State`/`AtomState` are `Copy` handles
whose `.set()` / `.set_no_update()` and operators (`+=` `-=` `*=` `/=`) take
`&mut self`, while `.get()` / `.read()` / `.write()` take `&self`. So a handle you
mutate must be `let mut`; defaulting every handle to `let mut` sidesteps the most
common trivial slip (an unused `mut` is only a warning). The general borrow rule
behind this is `rust-best-practices` territory — lean on that skill and on `cargo check`.

**Formatting a handle.** The handle itself is `Display`/`Debug` (`format!("{count}")`
works), but the guard from `.read()`/`.write()` is **not** — interpolating
`x.read()` into a `format!`/`Line` fails (`ReactiveRef doesn't implement Display`).
Format the handle directly, or deref the guard: `&*x.read()`, `x.read().clone()`,
or `x.read().as_str()`.

`State` and `AtomState` share the same methods/operators; full list in
`references/hooks.md`.

---

## Visual polish (principles, not a fixed style)

"Polished" here means **clear, consistent, and legible** — not a specific color
scheme. The framework gives you the tools; choose a palette and density that fit
the app's identity and the user's request. The repo's own examples gravitate
toward blue/cyan frames with `dark_gray` hint lines — treat that as *one coherent
option*, not a rule.

**Colors come from a theme, not scattered literals.** Every component derives its
styles from a shared `Palette`, and the default palette already gives a coherent
scheme — you get sensible colors without setting any style prop. Recolor the whole
app by tuning a `Palette` inside `PaletteProvider`; override one spot with a bare
`Style` prop (component theme overrides are `Option<Style>`, patched over the theme; configuration structs such as `ScrollbarStyle` use plain `Style` fields);
re-style one component type with `ThemeOverride::<FooTheme>(theme: ...)` (turbofish
required); or drive the `Palette` from an `Atom<Palette>` to switch themes at
runtime. If the user wants a named color scheme (Dracula, Nord, Catppuccin, ...)
rather than a hand-picked one, reach for the official `ratatui-kit-themes`
extension crate instead of hand-tuning `Palette` fields — see
`references/theming.md` §7. For every `Palette` field, every built-in
component's `*Theme` fields, and how to make your own component theme-aware,
see `references/theming.md`.

Principles, and the tools that achieve them:

- **Hierarchy** — separate primary from secondary with color + weight + alignment.
  Tools: `Stylize` (`.bold()`, `.green()`, `Style::new().fg(..)`), reverse-video
  highlights (`Style::new().black().on_cyan()`) for the selected row.
- **Whitespace** — let the UI breathe. Tools: `gap` between flex children,
  `margin` / `Padding`, sizing panels with `Constraint::Length`/`Fill`.
- **Alignment & framing** — fixed-width centered panels read as intentional. Tools:
  `Center`, `Border` with a `.centered()` `top_title`, `justify_content: Flex::Center`.
- **Orientation** — show users where they are and what they can press. Tools: a
  `bottom_title` hint line listing key → action (e.g. `" j/k move | Enter select | q quit "`).
- **State feedback** — distinguish loading / success / warning / error and the
  focused/selected element so the UI feels responsive. Tools: status-colored
  `Line`s, `active`/`highlight_style` props on `Select`/`MultiSelect`, `.dim()`
  backgrounds behind modals.

Concrete idioms, the standard app skeleton, and interactive-component patterns are
in `references/building-polished-uis.md`.

---

## Common pitfalls (each one is a real compile error or silent bug)

- **Hook inside `if`/`for`/`match`/early-return** → "Hook type mismatch" panic.
  Call hooks unconditionally at the top of the component.
- **A non-`mut` handle you later `.set()`/`+=`** → "cannot borrow as mutable" — the
  single most common trivial failure. Default to `let mut` on *every* `use_state` /
  `use_atom` / `use_exit` binding; an unused `mut` is just a warning.
- **`Select`/`MultiSelect` with no `default_index`** → nothing is highlighted on the
  first frame and `Enter` is a no-op until the user presses `j`/`k`. Set
  `default_index: Some(0)` whenever the first interaction is Enter-to-confirm.
- **Using a gated component without its feature** → "cannot find" errors. `Input`/
  `SearchInput` need `input`; `TreeSelect` → `tree`; `Table` → `table`; `VirtualList`
  → `virtual-list`; router items → `router`; `Atom` → `atom`. Develop with
  `--all-features` or `full`.
- **Layout props on a function component's call site** instead of its returned root
  → no effect (transparent layout). Put them on the root element the component returns.
- **`for` element without `key:`** → reconciliation reuses the wrong node; state
  leaks between rows. Use the loop index or a stable id.
- **Expecting a redraw from a plain variable** → nothing happens. Only `use_state`/
  `Atom`/async writes/events wake the render loop.
- **Storing an `InputLayer` across frames** → it's re-minted each frame; a stored
  handle goes deaf. Pass it within the same frame only.
- **`Block` is not `Send + Sync`** since Ratatui 0.30 — hold `Option<Block<'static>>`
  directly in props; don't reintroduce any `SendBlock` wrapper.
- **A `{ … }` right after a self-closing component is eaten as its children**, not a
  sibling embed. `SearchInput(props) { if x { Foo } }` makes the brace `SearchInput`'s
  *children block* — which parses `if`/`for`/`match` as first-class control flow over
  **element children**, so an `element!(…)` or arbitrary Rust expr inside yields a cryptic
  `expected identifier`. For a conditional **sibling**, use first-class control flow with
  **native** element children (no wrapping `{}`, no inner `element!(…)`/`.into_any()`):
  `SearchInput(props)` then `if x { Border(…){…} } else { TreeSelect<T>(…) }`.
- **`widget(w)` needs `for<'a> &'a w: Widget`** (renders by reference). Widgets that only
  impl `Widget` *by value* (e.g. `tui-big-text`'s `BigText`) fail with `&T: Widget is not
  satisfied`. Use a version that impls `WidgetRef` (→ `&T: Widget`), or wrap it:
  `struct W(BigText); impl Widget for &W { fn render(self, a, b){ self.0.clone().render(a,b) } }`
  then `widget(W(bt))`.

---

## Verifying

**Always** compile before declaring the task done — a clean `cargo check` is the
definition of done. Build with the features you actually used (gated modules don't
compile otherwise), read any errors, fix them, and re-check until clean:

```bash
cargo check --features full           # or your specific feature set
cargo clippy --all-targets --features full -- -D warnings
cargo run                             # run it; or `cargo run --example <name>` in the repo
```

Most first-compile failures are trivial and the compiler names them precisely — a
missing `mut` on a mutated `State` handle, a wrong prop name, or an un-enabled
feature. The repo has runtime, component, pointer-interaction, and macro UI tests.
For framework changes, run the repository's all-feature test matrix in addition
to compiling the application. If you scaffolded a binary, a successful
`cargo run` that draws the expected first frame confirms the wiring.
