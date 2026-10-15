# Building Polished, Idiomatic UIs

What this file covers: how to assemble a `ratatui-kit` app that looks intentional and reads cleanly — the app skeleton, layout mechanics, the framework tools that create visual polish, the interactive-component idioms, and a pitfalls checklist. Every snippet is distilled from real examples in this repository.

A note on aesthetics: this file teaches **principles and tools**, not a mandatory house style. The repository's own examples gravitate toward a blue/cyan framing with green "success" accents — that is *one* coherent option shown here as an illustrative idiom, not a rule. Pick a palette that fits your app's identity and the user's request, then apply these same principles (hierarchy, whitespace, alignment, clear focus, status feedback) with whatever colors you choose.

## Table of contents

1. App skeleton
2. Layout mechanics
3. Principles of polish (and the tools that achieve them)
4. Interactive-component idioms
5. State: local vs global vs async vs routing
6. Pitfalls checklist

---

## 1. App skeleton

### Cargo.toml

The main library ships with **zero features enabled** by default; opt in as needed. Most interactive components are feature-gated and simply will not compile unless their feature is on.

```toml
[dependencies]
# Enable `input` for Input/SearchInput; `tree` for TreeSelect; `virtual-list` for VirtualList;
# `router` for routing; `atom` for global Atom. Use `full` to turn everything on.
ratatui-kit = { version = "0.12", features = ["full"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
```

| Feature | Unlocks |
| --- | --- |
| `router` | `RouterProvider`/`Outlet`, `routes!`, `use_navigate`/`use_route`/`use_params` |
| `atom` | `Atom`/`AtomState`/`use_atom` (global state) |
| `input` | `Input`/`SearchInput` (`tui-input`) |
| `tree` | `TreeSelect` |
| `virtual-list` | `VirtualList` |
| `full` | all of the above |

`View`/`Border`/`Center`/`Fragment`/`Text`/`WrappedText`/`Positioned`/`Modal`/`ConfirmModal`/`AlertModal`/`ShortcutInfoModal`/`Select`/`MultiSelect`/`ScrollView` are **core components — no feature required**.

Two scaffolding notes:

- The crate is **edition 2024** — set `edition = "2024"` in a new crate's `Cargo.toml`.
- When scaffolding a standalone crate that may sit inside another Cargo workspace, add an empty `[workspace]` table to its `Cargo.toml` so Cargo does not attach it to the enclosing workspace.

### Standard imports + launch

Every example uses the same import shape: `prelude::*` for framework items, `ratatui::` for style/layout/text types, and `crossterm::event::` for keyboard event types.

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

#[tokio::main]
async fn main() {
    element!(App)
        .fullscreen()                 // takes over the full screen + alternate screen; a non-fullscreen mode also exists
        .await
        .expect("Failed to run the application");
}
```

### Top-level component + exit convention (copy directly)

There are two default exit paths: the closure from `use_exit()` handles your business key (by convention `q`/`Q`), and Ctrl+C exits directly through the framework. Applications that need agent cancellation, confirmation, or double-press exit can disable the automatic Ctrl+C exit and handle it as a `Global` event; use the borrow-safe pattern in `events-state-routing.md` §1.5. Every event handler has the same shape — **destructure `Event::Key` first, then check `KeyEventKind::Press`, then `match key.code`** — returning `Consumed` when handled and `Ignored` otherwise.

```rust
#[component]
fn Counter(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut count = hooks.use_state(|| 0_u64);
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            count += 1;                // State overloads `+=`; writing it triggers a re-render
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

    element!(
        Center(
            width: Constraint::Length(48),
            height: Constraint::Length(9),
        ) {
            Border(
                flex_direction: Direction::Vertical,
                justify_content: Flex::Center,
                border_style: Style::new().cyan(),
                top_title: Line::from(" ratatui-kit counter ").cyan().bold().centered(),
                bottom_title: Line::from(" q quit · Ctrl+C exit ").dark_gray().centered(),
            ){
                View(
                    flex_direction: Direction::Vertical,
                    justify_content: Flex::Center,
                    gap: 1,
                ) {
                    View(height: Constraint::Length(1)){
                        Text(text: Line::styled(
                            format!("Counter: {:02}", count.get()),
                            Style::new().green().bold(),
                        ).centered())
                    }
                    View(height: Constraint::Length(1)){
                        Text(text: Line::from("state writes wake the terminal UI").centered())
                    }
                }
            }
        }
    )
}
```

**Component-organization principles:** a function component's signature is fixed as `fn Name(mut hooks: Hooks, props: &XxxProps) -> impl Into<AnyElement<'static>>` (omit the `props` parameter when there are no props; write `_hooks: Hooks` when you do not use hooks). Child components receive data through a props struct deriving `#[derive(Default, Props)]`. A list row can be a plain helper fn returning a styled `Line` (see `todo_rows` in `examples/apps/todo_app.rs`) rendered inside a for-loop `View`, **OR** a small `#[component]` when the row owns its own state/handlers (see `ProjectRow` in the router example) — factor into a component only when the row needs hooks. For multi-page apps, the top level uses `RouterProvider` + `routes!`, a shell component holds global shortcuts, and `Outlet` marks where the page renders.

---

## 2. Layout mechanics

### The "focus panel" skeleton: Center + Border

Nearly every example uses the same shell: a `Center` with **fixed `Constraint::Length`** centers a panel of fixed width and height, wrapping a `Border` that draws a titled frame. This is the signature look of a ratatui-kit app.

```rust
Center(width: Constraint::Length(96), height: Constraint::Length(22)) {
    Border(
        flex_direction: Direction::Vertical,
        gap: 1,                              // leave 1 blank row between child regions
        border_style: Style::new().blue(),
        top_title: ...,
        bottom_title: ...,
    ) { /* … */ }
}
```

### Flex field mapping + idiomatic combinations

`LayoutStyle` fields map directly onto ratatui's `Layout`/`Constraint`: `flex_direction` (`Direction::Vertical`/`Horizontal`), `justify_content` (ratatui `Flex`; center with `Flex::Center`), `gap`, `margin` (`Margin::new(h, v)`), `offset`, and `width`/`height` (`Constraint`).

Idioms:

- **Master/detail two columns:** a horizontal `View(flex_direction: Direction::Horizontal, gap: 2)`, with a fixed-width left column `Constraint::Length(N)` and a right column `Constraint::Fill(1)` that consumes the remainder. The todo / select / input / scrollview examples all use this layout.
- **Per-row list:** each row is `View(height: Constraint::Length(1)) { Text(...) }`, with a `key:` given to each row inside a `for` loop.
- **Comfortable whitespace:** pull regions apart with `gap: 1` / `gap: 2`, center content vertically with `justify_content: Flex::Center`, and center text horizontally with `Line::...centered()`. Status/info cards conventionally use `flex_direction: Direction::Vertical, justify_content: Flex::Center` so their content sits centered inside the frame.
- **Border padding:** `padding: Padding::new(l, r, t, b)` (reserved before entering `Block::inner`); modal content commonly uses `margin: Margin::new(2, 2)`.

### What transparent layout means for function components (important)

Function components generated by `#[component]`, along with `Center`/`Fragment`, are **transparent-layout wrappers**: they do not occupy an independent layout node, and instead **inherit the `LayoutStyle` of the first child element they return**.

Consequence: **layout properties (`width`/`height`/`flex_direction`/`gap`, …) must be written on the root element your function component returns, not on the wrapper at the call site.** To make a custom component fill its parent, write `width: Constraint::Fill(1)` on the root `View`/`Border` returned inside it, rather than `MyComp(width: ...)` at the call site (unless that prop is explicitly forwarded onto the root element).

---

## 3. Principles of polish (and the tools that achieve them)

Polish is the sum of transferable principles, each backed by a concrete framework tool. None of these mandate a specific color.

### Theming — colors come from a shared palette, not scattered literals

Every built-in component reads its colors from a **theme**. A single `Palette` (semantic slots: `accent`, `border`, `selection`, `on_accent`, `success`/`warning`/`error`/`info`, `fg`/`fg_dim`, `placeholder`, …) is the one color source; each component derives its styles from it. The default palette already gives a coherent scheme (cyan accent, dim-gray borders, green/yellow/red/blue semantics, `on_accent`+`selection` highlights) — so you get the color vocabulary below for free, without setting any style prop.

Prefer tuning the palette once over hardcoding colors everywhere:

- **Recolor the whole app** — wrap a subtree in `PaletteProvider`. `Palette` is `#[non_exhaustive]`, so build it from `Palette::default()` and set fields:
  ```rust
  let mut palette = Palette::default();
  palette.accent = Color::Rgb(94, 175, 255);
  palette.border = Color::Rgb(70, 100, 140);
  element!(PaletteProvider(palette: palette) { /* whole app */ })
  ```
- **Override one spot** — every style prop is `Option<Style>`. Pass `Some(style)` (or a bare `Style` — `element!` auto-wraps it in `Some`) to patch over the theme; `Some(Style::reset())` clears the slot to terminal default:
  ```rust
  Border(border_style: Style::new().magenta()) { /* just this border */ }
  ```
- **Re-style one component type** — inject a `FooTheme` override with `ThemeOverride`. Turbofish is required (`element!` can't infer a hand-written generic's type param):
  ```rust
  let mut t = BorderTheme::default();
  t.border_style = Style::new().green();
  element!(ThemeOverride::<BorderTheme>(theme: t) { /* Borders here are green */ })
  ```
- **Switch at runtime** — context reads are passive, so drive the `Palette` from reactive state: `static PALETTE: Atom<Palette> = Atom::new(Palette::default);`, subscribe with `hooks.use_atom(&PALETTE)`, feed `PaletteProvider(palette: palette.get())`, and `PALETTE.set(...)` re-themes on the next frame. See the `theme` example.

These four mechanics are all you need for most apps. For the complete
`Palette` field table (including which fields no built-in component actually
reads), every built-in component's `*Theme` field list, how to make your own
component theme-aware, and using a preset color scheme instead of hand-tuning
one, see `references/theming.md`.

The subsections below are the per-element polish tools; reach for a bare `Style` prop or `ThemeOverride` when one spot needs to deviate, but let the palette carry the base scheme.

### Visual hierarchy — color + weight + alignment

Use color, font weight, and alignment together so the eye lands on the right thing first. The tools: ratatui's `Stylize` chained methods (or `Style::new().xxx()`), `.bold()` for weight, and `.centered()` for alignment.

Both spellings are common and equivalent: `Style::new().cyan()` ≡ `Style::new().fg(Color::Cyan)`; `Line::from(" title ").cyan().bold()` ≡ `Line::styled(text, Style::new().green().bold())`.

To mix two styles on one line, build it from multiple spans (`Span` lives at `ratatui::text::Span`, imported alongside `Line`):

```rust
Line::from(vec![
    Span::styled("label ", Style::new().dark_gray()),
    Span::styled(value, Style::new().yellow()),
])
```

The color vocabulary below is **what the default `Palette` already encodes** (and what the repository's examples gravitate toward) — treat it as the themed baseline, not as rules you must set by hand. To shift the whole scheme, tune the `Palette` (see *Theming* above) rather than restyling each element.

| Purpose | Idiomatic color/style (examples) | Where it appears |
| --- | --- | --- |
| Main panel border/title | `.blue()` or `.cyan()` | every example's outer frame |
| Title text | `.cyan().bold().centered()` / `.blue().bold().centered()` | pervasive |
| Bottom shortcut hint | `.dark_gray().centered()` | every example |
| Primary value / success | `.green().bold()` | counter, score |
| Selected-row highlight | `Style::new().black().on_cyan()` (or `.on_green()`) | todo/modal/router rows |
| Done / de-emphasized | `.dark_gray()` or `.dim()` | completed todos, placeholder text |
| Warning | `.yellow()` | empty state, confirm dialog |
| Error | `.red()` | validation failure |
| Cursor | `Style::new().bg(Color::Yellow)` | Input/SearchInput |

### Consistent alignment — centered titles, fixed-width panels

Border titles are uniformly passed as a `Line` and centered with `.centered()`, with a single leading and trailing space (` title `) so the title breathes against the frame.

```rust
Border(
    border_style: Style::new().blue(),
    top_title: Line::from(" todo app ").blue().bold().centered(),
    bottom_title: Line::from(" a add | j/k move | Space toggle | f filter | d delete | q quit ")
        .dark_gray().centered(),
) { /* … */ }
```

### Status feedback — a bottom shortcut row, and a clear loading/success/warn/error vocabulary

A strong convention: every panel's `bottom_title` is a single `dark_gray`, centered line listing shortcuts, separated by ` | ` or ` · `, in `key action` form.

```
" j/k move | Enter choose | e empty | q quit "
" q quit · Ctrl+C exit "
" + increase | - decrease | r reset | Esc back "
```

For dynamic status, give success/warn/error states distinct colors so feedback reads at a glance:

- **Async status trio:** loading → `.yellow()`, error → `.red()`, ready → `.green()`.
- **Validation border states** (built into `SearchInput`, themed from the palette by default): `border_style` / `active_border_style` (accent) / `success_border_style` (success) / `error_border_style` (error), corresponding to idle / focused / valid / invalid. Each is an `Option<Style>` override on top of `SearchInputTheme`.

### Clear focus / selected states — `>` marker + reverse video

Make the focused row unmistakable: a `>` prefix marker plus a reverse-video highlight (`black().on_cyan()`); unselected rows use the normal color; completed/disabled rows use `dark_gray()`. Dim the backdrop behind a modal with `Modal(style: Style::new().dim())` so the dialog pops.

If you gate the selected-row highlight behind a focus/mode flag (e.g. highlight only when the list is focused), the list shows **no cursor** in the other mode — usually undesirable. Prefer always rendering a selected-row marker, optionally **dimming** (not removing) it when the list is unfocused.

```rust
let style = if index == cursor {
    Style::new().black().on_cyan()      // selected
} else if todo.done {
    Style::new().dark_gray()            // completed, de-emphasized
} else {
    Style::new()
};
Line::styled(format!("{marker} {checkbox} {}", todo.title), style)
```

---

## 4. Interactive-component idioms

### Select / MultiSelect

Pass your data as `items` (for the generic component, write `Select<&'static str>(...)`). **The component handles `j/k` navigation itself**; your code only wires callbacks. Keyboard conventions: `j/k` to move, `Enter` to choose, `Space` to toggle (MultiSelect). Color props: `highlight_symbol: "> "`, `highlight_style: Style::new().fg(Color::Black).bg(Color::Green)`, plus `empty_style`/`empty_message` for the empty state. MultiSelect distinguishes `on_change` (draft changed) from `on_select` (Enter committed), and has `selected_item_style` to mark checked items. `default_index: Some(0)` selects the first row on load; **omit it and no row is highlighted** until `j`/`k` is pressed (and `Enter` does nothing). Set it whenever the first interaction is Enter-to-confirm.

**Choosing Input vs SearchInput:** when a text field must coexist with single-key list commands (`j`/`k`/Space/`q`), prefer **SearchInput** — its `activate_key` opens an exclusive input layer while editing, so background command keys stay live and you avoid a global Editing/Navigating mode. Reach for the raw **Input** only when the whole screen is a form with no competing single-letter commands (you forward keys to it yourself, which consumes them while the field is focused).

### Input (controlled)

`Input` only renders the `tui_input::Input` state; **keyboard events are forwarded by the page handler**. Store the input in `use_state(tui_input::Input::default)`, call `input.write().handle_event(&event)` inside your handler, and intercept special keys (Esc to clear/exit, Enter to submit) separately. Requires `use prelude::tui_input::backend::crossterm::EventHandler`. Props: `placeholder` / `placeholder_style` / `cursor_style: Style::new().bg(Color::Yellow)` / `hide_cursor: false`.

### SearchInput (built-in input layer)

A higher-level component that **brings its own exclusive input layer**: pressing `activate_key` (default `KeyCode::Char('s')`; the todo app overrides it to `KeyCode::Char('a')`) enters input mode, during which background `j/k` is suppressed. It is controlled: `value` + `on_change`. Callbacks: `on_submit` (returns `bool`; `false` means validation failed, stay in input mode), `on_clear`, and `validate` (returns `(bool, String)` giving validity plus a hint). Toggles: `clear_on_submit` / `clear_on_escape`. Color it with the validation-border quartet above plus `cursor_style`.

```rust
SearchInput(
    width: Constraint::Fill(1),
    value: draft.read().to_string(),
    placeholder: "Press a to add a task".to_string(),
    activate_key: KeyCode::Char('a'),
    on_change: move |next: String| draft.set(next),
    on_submit: move |value: String| {
        let title = value.trim().to_string();
        if title.len() < 3 {
            status.set("task title needs at least 3 chars".to_string());
            return false;        // false keeps the user in input mode
        }
        // … push the new task, move the cursor, set status …
        true
    },
    validate: move |value: String| {
        let len = value.trim().chars().count();
        if len == 0 {
            (true, "type a task".to_string())
        } else if len < 3 {
            (false, "too short".to_string())
        } else {
            (true, "Enter adds task".to_string())
        }
    },
    clear_on_submit: true,
    clear_on_escape: true,
    border_style: Style::new().cyan(),
    active_border_style: Style::new().yellow(),
    success_border_style: Style::new().green(),
    error_border_style: Style::new().red(),
    cursor_style: Style::new().bg(Color::Yellow),
)
```

### Modal + input layer (the standard hand-rolled mutual-exclusion pattern)

Modal keys not leaking to the background depends on `InputLayer`. **The handler and the `Modal` must share the same layer:**

```rust
let layer = hooks.use_input_layer(modal_open.get(), true);   // (open, blocks_lower)
hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, move |event| {
    // handle modal keys; uniformly return Consumed (swallow all keys so the background gets none)
    EventResult::Consumed
});
element!(
    Modal(open: modal_open.get(), layer: Some(layer), style: Style::new().dim()) {
        Border(margin: Margin::new(2, 2), border_style: Style::new().yellow(), ...) { /* content */ }
    }
)
```

- The `layer` handle is **rebuilt every frame — do not store it in State.**
- Forgetting `layer: Some(layer)` → the Modal opens a fresh layer that cuts off the parent layer, and your handler receives no events.
- For simple dialogs, prefer the built-in `ConfirmModal` / `AlertModal` / `ShortcutInfoModal` (each wires its own input layer; you only supply `open` + callbacks + content + style props such as `on_confirm`/`on_cancel`, `title_style`, `selected_button_style`). Hand-roll `use_input_layer` only when the modal has complex interaction inside it.
- Components placed inside the Modal subtree can use `EventScope::Current`; a parent component that wants to control the Modal's keys uses `EventScope::Layer(layer)`.

`State`/`AtomState` handles are `Copy`, so the **same** handle can be captured into several independent `move` closures (the event handler, `on_confirm`, `on_cancel`) without cloning — bind it `let mut x` once at the top and move copies in.

`ConfirmModal` as used in the todo app:

```rust
ConfirmModal(
    open: pending_index.is_some(),
    width: Constraint::Length(72),
    height: Constraint::Length(10),
    title: Line::from("Delete task?"),
    content: format!("Remove {pending_title}?"),
    confirm_text: "Delete".to_string(),
    cancel_text: "Keep".to_string(),
    style: Style::new().dim(),
    border_style: Style::new().yellow(),
    title_style: Style::new().yellow().bold(),
    button_style: Style::new().gray(),
    selected_button_style: Style::new().yellow().bold(),
    on_confirm: move |_: ()| { /* … perform delete, update cursor + status … */ },
    on_cancel: move |_: ()| { /* … set status to "delete canceled" … */ },
)
```

### ScrollView

Use when content is taller than the viewport. Two modes: **automatic** — omit `scroll_view_state`, and a built-in handler processes `j/k` / `PageUp/Down` / `Home/End`; **controlled** — pass `scroll_view_state: hooks.use_state(ScrollViewState::default)`, drive it yourself in the handler with `scroll_state.write().handle_event(&event)`, and read `offset()`. The subtree is ordinary layout; **give each row a fixed `height: Constraint::Length(1)` + `key:`**. Scrollbars: `scroll_bars: ScrollBars { vertical_scrollbar_visibility: ScrollbarVisibility::Always, horizontal...: Never, ..Default::default() }`; the viewport frame is passed as a ratatui `block: Block::bordered().title(...).border_style(...)`.

### List = a `for` loop over rows

ratatui-kit has no "List" component; **a list is a `for` loop rendering per-row `View`/`Text`** (or use `VirtualList` / `Select`). Make each row a `Text` with a `key:`, and reverse-video the selected row. For long body text inside a `ScrollView`, use `WrappedText(text, wrap_width, style)` — it turns wrapped lines into real layout height so the scrollbar/paging lands correctly (a plain `Text(wrap: true)` only soft-wraps and does not report height).

### Control flow (first-class inside `element!`)

Inside an `element!` block you can write `if` / `if let` / `else if` / `for` / `match` directly, and each branch may return a different element type; `{ expr }` embeds an expression returning `Option` / `Vec` / `Element` / `impl Iterator<Item = Element>`. **Every child element in a `for` loop must be given a stable `key:`** (use the loop index or a unique data id).

```rust
if rows.is_empty() {
    View(height: Constraint::Fill(1), justify_content: Flex::Center) {
        Text(text: Line::from("No tasks in this filter").yellow(), alignment: Alignment::Center)
    }
} else {
    for (index, row) in rows.into_iter().enumerate() {
        View(height: Constraint::Length(1), key: index) { Text(text: row) }
    }
}
```

---

## 5. State: local vs global vs async vs routing

- **Local** `use_state`: independent per component, released on unmount. `State<T>` is `Copy`, with `.get()`/`.set()`/`.read()`/`.write()`, and overloads arithmetic operators: `count += 1`, `selected -= 1` trigger a re-render directly. Keep transient input drafts local.
- **Global** `Atom` (`atom` feature): `static FOCUS: Atom<String> = Atom::new(|| "...".into());`; subscribe with `let focus = hooks.use_atom(&FOCUS);` inside a component and write via `focus.set(...)` / `score += 1`. **Only components that subscribe to that atom are woken.** Put committed application state in an Atom to share it across pages/components.
- **Async state** `use_async_state(|| async {...}, dep)`: refetches when `dep` changes; returns `result.loading` / `result.data` / `result.error`, and **stale data stays visible while refreshing** (use the three-color status line above).
- **Routing** `use_navigate` (`.push` / `.replace` / `.back` / `.forward` / `.push_with_state`), `use_params` (dynamic segments like `:slug`), `try_use_route_state::<T>()` (optional RouteState). The shell component holds global navigation keys; pages render through `Outlet`.

---

## 6. Pitfalls checklist

- **Hook call order must be stable:** `use_state` / `use_future` / `use_event_handler` / `use_atom`, etc. are indexed by call order. **Never put them inside `if` / `for` / `match`**, or subsequent frames misalign and panic. Always call them unconditionally at the top level of the component body.
- **No feature → won't compile:** using `Input`/`SearchInput` (`input`), `TreeSelect` (`tree`), `VirtualList` (`virtual-list`), routing (`router`), or `Atom` (`atom`) without enabling the matching feature yields "symbol not found" errors. During development use `--all-features` or `full`.
- **`Block` is no longer `Send + Sync` after ratatui 0.30** (it carries shadow effects). Hold `Option<Block<'static>>` directly in props; do not bring back the old `SendBlock` wrapper.
- **Transparent layout puts layout props in the wrong place:** function components / `Center` / `Fragment` do not occupy an independent layout node and inherit the first child's `LayoutStyle`. Layout properties (`width`/`height`/`flex_direction`/`gap`) must go on the **root element** the component returns, not on the wrapper at the call site.
- **The `InputLayer` handle is rebuilt every frame — do not store it in `State`:** next frame its id is no longer in the active layer stack. Only pass it to a handler or child component within the same frame.
- **A Modal's handler and component must share the same `layer`:** if `Modal(layer: Some(layer))` is missing or wrong, the Modal opens a new layer that cuts off the parent layer and your handler receives no events.
- **`for`-loop children must have a `key:`:** reconciliation reuses the previous frame's node by `ElementKey` + component `TypeId`. **Same key, same type → hooks/state are preserved**; an unstable key (e.g. using changing content as the key) causes state to misalign or be lost. List items conventionally use the loop index or a unique data id.
- **Wrong `EventResult` swallows or drops keys:** return `Consumed` when handled (stops propagation), `Ignored` when not (lets later handlers on the same layer, or lower layers, continue). To let a built-in ScrollView coexist with same-layer shortcuts, the automatic handler returns `Ignored` for keys it does not recognize.
- **Only writes trigger a re-render:** the UI is not imperatively redrawn — `State`/`AtomState`/async writes wake the render loop via a Waker. Mutating a plain variable does not repaint; all visible state must live in `use_state` / `Atom`.
- **`z-order` beats `priority`:** an upper layer's `Normal` fires before a lower layer's `High`. Don't try to win a modal's keys by raising a background component's priority — use `InputLayer`'s `blocks_lower`.
- **The `textarea` feature is retired** (`tui-textarea` has no ratatui-0.30-compatible release); do not reference it.
