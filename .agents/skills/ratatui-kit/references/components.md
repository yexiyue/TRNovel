# Built-in Components Reference

What this file covers: every built-in `ratatui-kit` component you can write inside the `element!` macro — its purpose, required feature flag, whether it acts as a layout node, its exact Props fields (with types and defaults), and a minimal `element!` usage snippet. This is the source of truth for generating correct component code; a wrong type or omitted prop will not compile.

All built-in components are re-exported from `prelude::*` (`use ratatui_kit::prelude::*;`).

## Layout fields and `element!` conventions (read first)

Layout fields are injected by the `#[with_layout_style]` attribute macro. When a component is a "layout component", it carries some or all of these seven fields, with these **exact** types:

| Field | Type |
| --- | --- |
| `flex_direction` | `ratatui::layout::Direction` |
| `justify_content` | `ratatui::layout::Flex` |
| `gap` | `i32` |
| `margin` | `ratatui::layout::Margin` |
| `offset` | `ratatui::layout::Offset` |
| `width` | `ratatui::layout::Constraint` |
| `height` | `ratatui::layout::Constraint` |

`#[with_layout_style]` with no arguments injects **all seven**. The parameterized form `#[with_layout_style(margin, offset, width, height)]` injects only the subset listed in the parentheses (the others are unavailable as props on that component).

Two `element!` macro conveniences used throughout this reference:

- **`Option<T>` props accept a bare `T`** — the macro auto-wraps it in `Some(...)`. So a prop typed `Option<Line<'static>>` can be passed `top_title: Line::from(" title ")`.
- **`From`-converting props** — several props (e.g. `TextParagraph<'static>`) accept any type with a `From` impl, so you can pass `&str`, `String`, `Line`, etc. directly. These are noted per-field below.

### Style props are theme-backed `Option<Style>` (read this)

Every `*_style` / style prop listed below (Border's `border_style` / `style`, Select's `highlight_style`, Table's `header_style`, …) is actually **`Option<Style>`, defaulting to the component's theme** — the per-component types below still write `Style` for brevity, so **read every style prop as `Option<Style>`**. This applies to component theme-override props, not configuration structs: `ScrollbarStyle` fields are plain `Style`. Behavior:

- Omit it or pass `None` → use the theme.
- Pass a bare `Style` (auto-wrapped to `Some`) → patch over the theme (your fields win, the rest stay themed).
- Pass `Some(Style::reset())` → clear that slot to the terminal default.

Colors come from a **shared `Palette`** (semantic slots: `accent`, `border`, `selection`, `on_accent`, `success`/`warning`/`error`/`info`, `fg`/`fg_dim`, `placeholder`), and each component derives its styles from it via a `FooTheme` (`BorderTheme`, `SelectTheme`, `TableTheme`, …). So instead of setting style props everywhere:

- **Recolor everything** — `PaletteProvider(palette: p) { … }` (`Palette` is `#[non_exhaustive]`; build from `Palette::default()` and set fields).
- **Re-style one component type** — `ThemeOverride::<BorderTheme>(theme: t) { … }` (turbofish required; `element!` can't infer a hand-written generic's type param).
- **Runtime switching** — put the `Palette` in an `Atom<Palette>` driving `PaletteProvider` (context reads are passive; a state write re-themes on the next frame).

`Palette`, `ComponentTheme`, `PaletteProvider`, `ThemeOverride`, `use_palette` / `use_component_theme`, and every `FooTheme` are in `prelude::*`. For each component's exact `*Theme` fields (what `ThemeOverride::<FooTheme>` can actually touch), see `references/theming.md` §2.

---

## View

- **Purpose:** basic flex layout container; wraps and arranges child components.
- **Feature:** core (no gate).
- **Layout component:** **yes** (`#[with_layout_style]`, all 7 layout fields).
- **Props** (`ViewProps<'a>`, `Default`):
  - `children: Vec<AnyElement<'a>>`
  - plus all 7 layout fields.

```rust
View(flex_direction: Direction::Vertical, gap: 1) {
    View(height: Constraint::Length(1)) {
        Text(text: Line::from("a"))
    }
    View(height: Constraint::Length(1)) {
        Text(text: Line::from("b"))
    }
}
```

---

## Fragment

- **Purpose:** a render-free transparent container for returning/wrapping multiple root elements (like `React.Fragment`).
- **Feature:** core.
- **Layout component:** no (calls `set_transparent_layout(true)`; takes no independent layout node).
- **Props** (`FragmentProps<'a>`, `Default`):
  - `children: Vec<AnyElement<'a>>`

```rust
Fragment {
    Text(text: "line one")
    Text(text: "line two")
}
```

---

## Border

- **Purpose:** draws a border, title, and padding around content.
- **Feature:** core.
- **Layout component:** **yes** (`#[with_layout_style]`, all 7 layout fields).
- **Props** (`BorderProps<'a>`, custom `Default`: `borders` defaults to `Borders::ALL`):
  - `padding: ratatui::widgets::Padding`
  - `border_style: ratatui::style::Style`
  - `borders: ratatui::widgets::Borders` (default `Borders::ALL`)
  - `border_set: ratatui::symbols::border::Set<'static>`
  - `style: ratatui::style::Style`
  - `children: Vec<AnyElement<'a>>`
  - `top_title: Option<ratatui::text::Line<'static>>` (pass a bare `Line`)
  - `bottom_title: Option<ratatui::text::Line<'static>>` (pass a bare `Line`)
  - plus all 7 layout fields.

```rust
Border(
    flex_direction: Direction::Vertical,
    border_style: Style::new().cyan(),
    top_title: Line::from(" title ").cyan().bold().centered(),
    bottom_title: Line::from(" q quit ").dark_gray().centered(),
) {
    Text(text: Line::from("Hello, World!").green().bold().centered())
}
```

---

## Text

- **Purpose:** renders a single block of text (backed by ratatui `Paragraph`) with styling, alignment, scroll, and soft wrapping.
- **Feature:** core.
- **Layout component:** no (a `#[component]` function component → transparent layout; put layout props on the enclosing `View`/`Border`).
- **Props** (`TextProps`, `Default`):
  - `text: TextParagraph<'static>` — accepts `&str` / `String` / `Line` / ratatui `Text` / `Paragraph` directly (all via `From`).
  - `style: ratatui::style::Style`
  - `alignment: ratatui::layout::Alignment`
  - `scroll: ratatui::layout::Position`
  - `wrap: Option<bool>` — `Some(trim)` enables soft wrapping; pass a bare `bool`.

```rust
Text(text: Line::from(format!("Counter: {:02}", count.get())).green().bold().centered())
// or with wrapping
Text(text: status_view, alignment: Alignment::Center, wrap: true)
```

---

## WrappedText

- **Purpose:** pre-wraps text to `wrap_width` and exposes the resulting real line count as its own layout height — ideal inside a `ScrollView` for long bodies, logs, or documents.
- **Feature:** core.
- **Layout component:** **yes** (`#[with_layout_style]`, all 7 layout fields; but `auto_height` defaults to overriding `height` with the wrapped line count).
- **Props** (`WrappedTextProps`, `Default`):
  - `text: String` (pass `&str` / `String` directly)
  - `style: ratatui::style::Style`
  - `alignment: ratatui::layout::Alignment`
  - `scroll: ratatui::layout::Position`
  - `wrap_width: Option<u16>` — width used for wrapping/height calc; if omitted, falls back to a `Length` `width` or the constant 80.
  - `break_words: Option<bool>` (default `true`)
  - `auto_height: Option<bool>` (default `true`; sets height to the line count)
  - plus all 7 layout fields.

```rust
ScrollView(flex_direction: Direction::Vertical, scroll_view_state: scroll_state) {
    WrappedText(text: BODY, wrap_width: 72, style: Style::new().white())
}
```

---

## Center

- **Purpose:** centers child content horizontally and vertically within given width/height constraints.
- **Feature:** core.
- **Layout component:** no (transparent layout; internally expands to three nested `View`s using `Flex::Center`). Note it has only `width`/`height` and is **not** a `#[with_layout_style]` component.
- **Props** (`CenterProps<'a>`, `Default`):
  - `width: ratatui::layout::Constraint`
  - `height: ratatui::layout::Constraint`
  - `children: Vec<AnyElement<'a>>`

```rust
Center(width: Constraint::Length(42), height: Constraint::Length(7)) {
    Border(/* ... */) { Text(text: Line::from("Hello")) }
}
```

---

## Positioned

- **Purpose:** absolutely positions child content into a specified rectangle, optionally clearing that area first.
- **Feature:** core.
- **Layout component:** no (occupies a 0×0 layout node; positions via `x/y/width/height`).
- **Props** (`PositionedProps<'a>`, `Default`):
  - `clear: bool` (default `false`; whether to `Clear` the area before rendering)
  - `x: u16`
  - `y: u16`
  - `width: u16`
  - `height: u16`
  - `children: Vec<AnyElement<'a>>`

```rust
// from inside Input, drawing the cursor
Positioned(x: cursor_x, y: cursor_y, width: 1u16, height: 1u16) {
    widget(Span::from(" ").style(props.cursor_style))
}
```

---

## ContextProvider

- **Purpose:** injects a context value into the subtree (dependency injection / scoped config) for `hooks.use_context::<T>()` to find upward; the nearest provider wins.
- **Feature:** core.
- **Layout component:** no (transparent layout).
- **Props** (`ContextProviderProps<'a>`, `Default`):
  - `children: Vec<AnyElement<'a>>`
  - `value: Option<Context<'a>>` (typically `Context::owned(my_data)`)

```rust
ContextProvider(value: Context::owned(settings)) {
    ChildThatReadsContext
}
```

---

## Modal

- **Purpose:** a modal popup with a backdrop, placement/sizing, and input-layer integration.
- **Feature:** core.
- **Layout component:** **partial** (`#[with_layout_style(margin, offset, width, height)]` — only those 4 layout fields; no `gap`/`flex_direction`/`justify_content`). It occupies 0×0; the popup rect is computed in the `draw` phase from `placement` + `width`/`height`.
- **Props** (`ModalProps<'a>`, `Default`):
  - `children: Vec<AnyElement<'a>>`
  - `style: ratatui::style::Style`
  - `placement: Placement` — enum: `Top` / `TopLeft` / `TopRight` / `Bottom` / `BottomLeft` / `BottomRight` / `Center` (default) / `Left` / `Right`.
  - `open: bool`
  - `layer: Option<InputLayer>` — if the parent already called `use_input_layer`, pass that handle to reuse it; otherwise `None` lets the Modal open its own exclusive layer.
  - `blocks_lower: Option<bool>` (`None` is treated as `true`; truncates lower layers)
  - plus the 4 layout fields `margin`/`offset`/`width`/`height`.

**Positioning is relative to the entire terminal buffer, not the parent panel.** In `draw` the popup rect is computed from `drawer.buffer_mut().area()` (the whole screen), then `margin`/`offset` and the `placement` flex split are applied. So `placement: Center` is **screen-centered** regardless of where the `Modal` tag sits in the component tree — you do not need to mount it at the root, and nesting it inside a sub-panel does not confine it to that panel.

```rust
let layer = hooks.use_input_layer(modal_open.get(), true);
// ...
Modal(
    open: modal_open.get(),
    layer: Some(layer),
    width: Constraint::Length(68),
    height: Constraint::Length(12),
    style: Style::new().dim(),
) {
    Border(top_title: Line::from(" content ").yellow().centered()) {
        Text(text: Line::from("body"), alignment: Alignment::Center)
    }
}
```

---

## ConfirmModal

- **Purpose:** a confirmation popup with input mutual-exclusion (confirm/cancel buttons; opens its own exclusive input layer internally).
- **Feature:** core (`#[component]` function component).
- **Layout component:** no.
- **Props** (`ConfirmModalProps`, custom `Default`):
  - `open: bool`
  - `title: ratatui::text::Line<'static>`
  - `content: TextParagraph<'static>` (pass `&str` / `String` / `Line` directly)
  - `confirm_text: String`
  - `cancel_text: String`
  - `on_confirm: Handler<'static, ()>`
  - `on_cancel: Handler<'static, ()>`
  - `width: ratatui::layout::Constraint`
  - `height: ratatui::layout::Constraint`
  - `style: ratatui::style::Style` (default `Style::default().dim()` — the **dimmed backdrop** behind the popup; pass `Style::default()` if you do **not** want the screen dimmed)
  - `border_style: ratatui::style::Style`
  - `title_style: ratatui::style::Style`
  - `content_style: ratatui::style::Style`
  - `button_style: ratatui::style::Style`
  - `selected_button_style: ratatui::style::Style` (default `fg(Cyan)` + `BOLD` — the highlight for the currently-active button)
- **Built-in keys:** `Enter` activates the **currently-highlighted** button; **the default highlight is the Cancel button** (so a bare `Enter` on first open fires `on_cancel`). Left/Right/Tab/BackTab move the highlight between Cancel and Confirm. `y`/`Y` **always** confirm and `n`/`N`/`Esc` **always** cancel, regardless of which button is highlighted.

```rust
ConfirmModal(
    open: confirm_open.get(),
    width: Constraint::Length(70),
    height: Constraint::Length(10),
    title: Line::from("Delete release?"),
    content: format!("Remove {selected_label} from the queue?"),
    confirm_text: "Delete".to_string(),
    cancel_text: "Keep".to_string(),
    on_confirm: move |_: ()| { confirm_open.set(false); },
    on_cancel: move |_: ()| { confirm_open.set(false); },
)
```

---

## AlertModal

- **Purpose:** an alert popup with input mutual-exclusion (single close action).
- **Feature:** core (`#[component]`).
- **Layout component:** no.
- **Props** (`AlertModalProps`, custom `Default`):
  - `open: bool`
  - `title: ratatui::text::Line<'static>`
  - `message: TextParagraph<'static>` (pass `&str` / `String` / `Line` directly)
  - `close_hint: Option<ratatui::text::Line<'static>>` (pass a bare `Line`)
  - `close_keys: Vec<crossterm::event::KeyCode>` (default `[Esc, Enter]`)
  - `on_close: Handler<'static, ()>`
  - `width: ratatui::layout::Constraint`
  - `height: ratatui::layout::Constraint`
  - `style: ratatui::style::Style` (default `Style::default().dim()` — the **dimmed backdrop**; pass `Style::default()` to leave the screen un-dimmed)
  - `border_style: ratatui::style::Style` (default `fg(Yellow)`)
  - `title_style: ratatui::style::Style` (default `fg(Yellow)`)
  - `message_style: ratatui::style::Style`
  - `padding: ratatui::widgets::Padding` (default `Padding::uniform(1)`)

```rust
AlertModal(
    open: alert_open.get(),
    width: Constraint::Length(76),
    height: Constraint::Length(8),
    title: Line::from("Workspace is current"),
    message: format!("{selected_label} is already synchronized."),
    close_hint: Line::from("Enter / Esc").centered(),
    padding: Padding::new(2, 2, 1, 1),
    on_close: move |_: ()| { alert_open.set(false); },
)
```

---

## ShortcutInfoModal

- **Purpose:** a keyboard-shortcut help popup with input mutual-exclusion (grouped + scrollable).
- **Feature:** core (`#[component]`).
- **Layout component:** no.
- **Props** (`ShortcutInfoModalProps`, custom `Default`):
  - `open: bool`
  - `title: ratatui::text::Line<'static>`
  - `sections: Vec<ShortcutInfoSection>` — `ShortcutInfoSection { title: String, items: Vec<ShortcutInfo> }`; `ShortcutInfo { description: String, keys: String }`, constructible from `(&str, &str)`.
  - `close_hint: Option<ratatui::text::Line<'static>>`
  - `close_keys: Vec<crossterm::event::KeyCode>` (default `[Esc, 'i', 'I']`)
  - `on_close: Handler<'static, ()>`
  - `width: ratatui::layout::Constraint`
  - `height: ratatui::layout::Constraint`
  - `style: ratatui::style::Style`
  - `border_style: ratatui::style::Style`
  - `title_style: ratatui::style::Style`
  - `section_title_style: ratatui::style::Style`
  - `description_style: ratatui::style::Style`
  - `key_style: ratatui::style::Style`

```rust
ShortcutInfoModal(
    open: shortcuts_open.get(),
    width: Constraint::Length(78),
    height: Constraint::Length(13),
    title: Line::from("Shortcut reference"),
    close_hint: Line::from("j/k scroll | Esc / i close").centered(),
    sections: vec![
        ShortcutInfoSection::new("Navigation", [("Move down", "j / Down"), ("Move up", "k / Up")]),
    ],
    on_close: move |_: ()| { shortcuts_open.set(false); },
)
```

---

## Select

- **Purpose:** a single-selection list with keyboard interaction (j/k/Up/Down/Home/End/Enter), wrapped in a `Border`.
- **Feature:** core (`#[component]`, generic over `T`).
- **Layout component:** **partial** (`#[with_layout_style(margin, offset, width, height)]`; no gap/flex_direction/justify_content).
- **Generic bound:** `T: Into<ListItem<'static>> + Clone + Send + Sync + 'static`.
- **Props** (`SelectProps<T>`, custom `Default`):
  - `items: Vec<T>`
  - `on_select: Handler<'static, T>`
  - `state: Option<State<ratatui::widgets::ListState>>`
  - `top_title: Option<ratatui::text::Line<'static>>`
  - `bottom_title: Option<ratatui::text::Line<'static>>`
  - `active: bool` (default `true`)
  - `default_index: Option<usize>`
  - `empty_message: TextParagraph<'static>` (default `"No data"`)
  - `highlight_symbol: Option<&'static str>`
  - `style: ratatui::style::Style`
  - `border_style: ratatui::style::Style`
  - `highlight_style: ratatui::style::Style` (default `fg(Black).bg(Cyan)` — the visible row-highlight bar; a non-empty style, override only if you want different colors)
  - `empty_style: ratatui::style::Style`
  - `empty_width: ratatui::layout::Constraint`
  - `empty_height: ratatui::layout::Constraint`
  - plus `margin`/`offset`/`width`/`height`.

**`default_index` defaults to `None`** — so on the first frame **no row is highlighted** and pressing `Enter` is a no-op (`on_select` does not fire) until the user moves the cursor with `j`/`k`/arrows. Set `default_index: Some(0)` (or any valid index) to pre-select a row on load. This is **required** whenever the first interaction is Enter-to-confirm. (The same applies to `MultiSelect`.)

The generic type parameter must be written explicitly, e.g. `Select<&'static str>`:

```rust
Select<&'static str>(
    width: Constraint::Length(34),
    items: items,
    top_title: Line::from(" environment ").centered(),
    default_index: Some(1),
    highlight_symbol: "> ",
    highlight_style: Style::new().fg(Color::Black).bg(Color::Green),
    empty_message: "No environments",
    on_select: move |item: &'static str| { selected.set(item); },
)
```

---

## MultiSelect

- **Purpose:** a multi-selection list with keyboard interaction (j/k move, Space toggle, Enter submit).
- **Feature:** core (`#[component]`, generic over `T`).
- **Layout component:** **partial** (`#[with_layout_style(margin, offset, width, height)]`).
- **Generic bound:** `T: Into<ListItem<'static>> + Clone + Send + Sync + 'static`.
- **Props** (`MultiSelectProps<T>`, custom `Default`):
  - `items: Vec<T>`
  - `on_change: Handler<'static, Vec<T>>` (fires on every toggle)
  - `on_select: Handler<'static, Vec<T>>` (fires on Enter submit)
  - `state: Option<State<ratatui::widgets::ListState>>`
  - `selected: Option<State<std::collections::HashSet<usize>>>`
  - `top_title: Option<ratatui::text::Line<'static>>`
  - `bottom_title: Option<ratatui::text::Line<'static>>`
  - `active: bool` (default `true`)
  - `default_index: Option<usize>`
  - `empty_message: TextParagraph<'static>` (default `"No data"`)
  - `highlight_symbol: Option<&'static str>`
  - `style` / `border_style` / `highlight_style` / `selected_item_style` / `empty_style`: all `ratatui::style::Style` (`highlight_style` defaults to `fg(Black).bg(Cyan)` — the cursor-row bar; `selected_item_style` defaults to `fg(Cyan)` — the marker for toggled-on rows)
  - `empty_width: ratatui::layout::Constraint`
  - `empty_height: ratatui::layout::Constraint`
  - plus `margin`/`offset`/`width`/`height`.

```rust
MultiSelect<&'static str>(
    width: Constraint::Length(36),
    items: items,
    default_index: Some(0),
    highlight_symbol: "> ",
    selected_item_style: Style::new().fg(Color::Yellow).bold(),
    empty_message: "No checks",
    on_change: move |items: Vec<&'static str>| { selected_count.set(items.len()); },
    on_select: move |items: Vec<&'static str>| { /* submit */ },
)
```

---

## ScrollView

- **Purpose:** a scrollable view container whose subtree is ordinary ratatui-kit layout; content may overflow the viewport, with configurable scrollbars.
- **Feature:** core (hand-written `Component`).
- **Layout component:** **yes** (`#[with_layout_style]`, all 7 layout fields).
- **Props** (`ScrollViewProps<'a>`, custom `Default`):
  - `children: Vec<AnyElement<'a>>`
  - `scrollbars: Scrollbars<'static>`
  - `state: Option<State<ScrollViewState>>` (if omitted, managed internally; **orthogonal to `active`** — passing it does NOT disable built-in scrolling)
  - `block: Option<ratatui::widgets::Block<'static>>`
  - `active: bool` (default `true`; gates built-in keyboard/mouse scrolling, like the other selection components)
  - plus all 7 layout fields.
- **Related types:**
  - `ScrollViewState` — create with `use_state(ScrollViewState::default)`; methods: `handle_event(&event) -> bool`, `offset()`/`set_offset()`, `scroll_*`, `size()`/`page_size()`, `is_at_bottom()`, `scroll_to_visible(y, height)`.
  - `Scrollbars<'a> { vertical_scrollbar_visibility, horizontal_scrollbar_visibility: ScrollbarVisibility, vertical_scrollbar, horizontal_scrollbar: Scrollbar<'a>, over_border: bool }`. `over_border` (default `true`): with a bordered block, draw the scrollbar on the border ring vs inset inside it.
  - `ScrollbarVisibility`: `Automatic` (default) / `Always` / `Never`.

```rust
let scroll_state = hooks.use_state(ScrollViewState::default);
ScrollView(
    flex_direction: Direction::Vertical,
    state: scroll_state,
    scrollbars: Scrollbars {
        vertical_scrollbar_visibility: ScrollbarVisibility::Always,
        horizontal_scrollbar_visibility: ScrollbarVisibility::Never,
        over_border: true,
        ..Default::default()
    },
    block: Block::bordered().border_style(Style::new().cyan()),
) {
    for (index, row) in DOC_ROWS.into_iter().enumerate() {
        View(key: index, height: Constraint::Length(1)) { Text(text: row.line()) }
    }
}
```

---

## Input

- **Purpose:** a pure-rendering component for a single-line text input (renders `tui_input::Input` state + cursor); key handling is forwarded by the parent handler — the component itself does not process events.
- **Feature:** `input` (`#[cfg(feature = "input")]`).
- **Layout component:** no (`#[component]`; wrap it in a `Border` for the frame/height).
- **Props** (`InputProps`, `Default`, also `Debug + Clone`):
  - `input: tui_input::Input`
  - `cursor_style: ratatui::style::Style`
  - `placeholder: String`
  - `placeholder_style: ratatui::style::Style`
  - `style: ratatui::style::Style`
  - `hide_cursor: bool`

```rust
let input = hooks.use_state(tui_input::Input::default);
// in a handler: input.write().handle_event(&event);
// (requires `use tui_input::backend::crossterm::EventHandler`)
Border(height: Constraint::Length(3)) {
    Input(
        input: input.read().clone(),
        cursor_style: Style::new().bg(Color::Yellow),
        placeholder: "Type a note".to_string(),
        placeholder_style: Style::new().dark_gray(),
        style: Style::new().white(),
        hide_cursor: false,
    )
}
```

---

## SearchInput

- **Purpose:** a single-line search box with local input mutual-exclusion (press `s` by default to enter edit mode, opening an exclusive input layer; Enter submits, Esc cancels; built-in validation/status text).
- **Feature:** `input` (`#[cfg(feature = "input")]`).
- **Layout component:** **partial** (`#[with_layout_style(margin, offset, width)]` — only `margin`/`offset`/`width`, no `height`; height is fixed internally to `Length(3)`).
- **Props** (`SearchInputProps`, custom `Default`):
  - `value: String` (externally controlled value)
  - `placeholder: String`
  - `is_editing: bool` (default `true`; whether edit mode is allowed)
  - `activate_key: crossterm::event::KeyCode` (default `Char('s')`)
  - `on_change: Handler<'static, String>`
  - `on_submit: Handler<'static, String, bool>` (return `false` to prevent closing)
  - `on_clear: Handler<'static, ()>`
  - `validate: Handler<'static, String, (bool, String)>` (returns `(is_valid, status_text)`)
  - `clear_on_submit: bool`
  - `clear_on_escape: bool`
  - `border_style` / `active_border_style` / `success_border_style` / `error_border_style` / `input_style` / `placeholder_style` / `cursor_style` / `success_status_style` / `error_status_style`: all `ratatui::style::Style`
  - plus `margin`/`offset`/`width`.

```rust
SearchInput(
    width: Constraint::Fill(1),
    value: query.read().to_string(),
    placeholder: "Press s to search commands".to_string(),
    on_change: move |next: String| query.set(next),
    on_submit: move |value: String| { /* ... */ true },
    validate: move |value: String| (true, format!("{} matches", count(&value))),
    clear_on_escape: true,
)
```

---

## TreeSelect

- **Purpose:** a tree-selection component (backed by `tui-tree-widget`), with optional built-in keyboard interaction (h/l collapse, j/k move, Space toggle, Enter select).
- **Feature:** `tree` (`#[cfg(feature = "tree")]`, generic over `T`).
- **Layout component:** **partial** (`#[with_layout_style(margin, offset, width, height)]`).
- **Generic bound:** `T: Sync + Send + Clone + Eq + Hash + 'static` (the `Component` impl additionally requires `Unpin`).
- **Props** (`TreeSelectProps<T>`, custom `Default`):
  - `state: Option<State<tui_tree_widget::TreeState<T>>>`
  - `items: Vec<tui_tree_widget::TreeItem<'static, T>>`
  - `active: bool` (default `false` — built-in keyboard interaction disabled)
  - `default_selection: Vec<T>` (default selected path, e.g. `vec!["components", "select"]`)
  - `on_select: Handler<'static, T>`
  - `scrollbar: Option<ratatui::widgets::Scrollbar<'static>>`
  - `style: ratatui::style::Style`
  - `highlight_style: ratatui::style::Style`
  - `highlight_symbol: &'static str` (default `""`)
  - `node_closed_symbol: &'static str` (default `"▶ "`)
  - `node_open_symbol: &'static str` (default `"▼ "`)
  - `node_no_children_symbol: &'static str` (default `"  "`)
  - `block: Option<ratatui::widgets::Block<'static>>`
  - plus `margin`/`offset`/`width`/`height`.

Import the helper types via `use ratatui_kit::components::tui_tree_widget::{TreeItem, TreeState};`. Because `prelude::*` re-exports `components::*`, the path `use ratatui_kit::prelude::tui_tree_widget::{TreeItem, TreeState};` works just as well (consistent with how `tui_input` is imported elsewhere):

```rust
let tree_state = hooks.use_state(TreeState::<&'static str>::default);
TreeSelect<&'static str>(
    width: Constraint::Length(40),
    state: tree_state,
    active: true,
    items: demo_items(),
    default_selection: vec!["components", "select"],
    block: Block::bordered().title_top(Line::from(" component map ").centered()),
    highlight_symbol: "> ",
    highlight_style: Style::new().fg(Color::Black).bg(Color::Green),
    on_select: move |id: &'static str| { submitted.set(format!("selected {id}")); },
)
```

---

## VirtualList

- **Purpose:** a virtualized list (backed by `tui-widget-list`) that renders only visible items — suitable for tens of thousands of rows; built-in keyboard interaction (j/k, Home/End, Enter).
- **Feature:** `virtual-list` (`#[cfg(feature = "virtual-list")]`, generic over `W: Widget`).
- **Layout component:** **partial** (`#[with_layout_style(margin, offset, width, height)]`).
- **Props** (`VirtualListProps<W>`, custom `Default`; `W: Widget + 'static`):
  - `state: Option<State<tui_widget_list::ListState>>`
  - `item_count: usize`
  - `render_item: RenderVirtualItem<'static, W>` — converted via `From` from a closure `Fn(&ListBuildContext) -> (W, u16)`; returns `(widget, main-axis size)`.
  - `active: bool` (default `true`)
  - `default_index: Option<usize>`
  - `on_select: Handler<'static, usize>`
  - `scroll_axis: tui_widget_list::ScrollAxis` (default `Vertical`)
  - `scroll_direction: tui_widget_list::ScrollDirection` (default `Forward`)
  - `style: ratatui::style::Style`
  - `block: Option<ratatui::widgets::Block<'static>>`
  - `scroll_padding: u16`
  - `infinite_scrolling: bool` (default `true`)
  - plus `margin`/`offset`/`width`/`height`.

Import the helper types via `use ratatui_kit::components::tui_widget_list::{ListBuildContext, ListState};`. Since `prelude::*` re-exports `components::*`, `use ratatui_kit::prelude::tui_widget_list::{ListBuildContext, ListState};` also works (consistent with how `tui_input` is imported elsewhere):

```rust
let list_state = hooks.use_state(ListState::default);
VirtualList<Line<'static>>(
    width: Constraint::Length(42),
    state: list_state,
    item_count: 10_000,
    default_index: Some(42),
    block: Block::bordered().title_top(Line::from(" build log ").centered()),
    scroll_padding: 2u16,
    infinite_scrolling: false,
    render_item: |context: &ListBuildContext| {
        let style = if context.is_selected { Style::new().on_green() } else { Style::new() };
        (Line::styled(format!("row {:05}", context.index + 1), style), 1u16)
    },
    on_select: move |index: usize| { submitted.set(format!("selected row {}", index + 1)); },
)
```

---

## Table

- **Purpose:** a generic, data-driven table rendered from scratch (not a wrapper around `ratatui::widgets::Table`). Owns cell-grid borders, CJK/emoji-aware cell wrapping, responsive column hiding, a footer row, and row/column/cell highlighting. Built-in keyboard interaction (j/k rows, Home/End, Enter; Left/Right columns when `column_navigation`).
- **Feature:** `table` (`#[cfg(feature = "table")]`, generic over `T`).
- **Layout component:** **partial** (`#[with_layout_style(margin, offset, width, height)]`).
- **Generic bound:** `T: Clone + Send + Sync + Unpin + 'static`.
- **Props** (`TableProps<T>`, custom `Default`):
  - `columns: Vec<TableColumn>` — `TableColumn::new(header: impl Into<Line>, width: Constraint)`, then `.alignment(TableCellAlignment)` / `.min_table_width(u16)` (hide the column below that table width).
  - `rows: Vec<T>` — your own row data.
  - `render_row: Option<RenderTableRow<T>>` — `RenderTableRow<T> = Arc<dyn Fn(&T, bool) -> Vec<TableCell> + Send + Sync>`; receives `(row, is_selected)`, returns one `TableCell` per column. `TableCell::new(impl Into<Line>)`, then `.style(Style)` / `.alignment(TableCellAlignment)`.
  - `footer: Vec<TableCell>` (default empty = no footer) — a summary row aligned to the columns.
  - `state: Option<State<TableState>>`
  - `active: bool` (default `true` — built-in keyboard interaction on)
  - `default_index: Option<usize>`
  - `on_select: Handler<'static, T>` (fires on `Enter`)
  - `block: Option<ratatui::widgets::Block<'static>>`
  - `header_style: Style` (default `fg(Cyan)`), `footer_style: Style` (default `fg(Cyan)`), `row_style: Style`
  - `highlight_style: Style` (selected row; default `fg(Black).bg(Cyan)`)
  - `column_highlight_style: Style` (selected column; default empty), `cell_highlight_style: Style` (row/column intersection; default empty)
  - `highlight_symbol: Option<&'static str>` (default `Some("▶ ")`)
  - `highlight_spacing: HighlightSpacing` (default `WhenSelected`; `Always` / `WhenSelected` / `Never`) — reserves a leading gutter so the symbol never clips the first column.
  - `column_navigation: bool` (default `false`) — when `active`, Left/Right (h/l) move the selected column.
  - `column_spacing: u16` (default `1`, only used by `TableBorderMode::None`)
  - `wrap_mode: TableWrapMode` (default `Wrap`; `Wrap` / `Truncate`)
  - `border_mode: TableBorderMode` (default `Outer`; `None` / `Outer` / `Grid` — `Grid` draws a full cell grid)
  - `border_style: Style` / `horizontal_line_style: Style` (default `fg(DarkGray)`)
  - `cell_padding: u16` (default `1`)
  - `header_separator: bool` (default `true`), `footer_separator: bool` (default `true`), `row_separator: bool` (default `false`) — separators only take a visible line in `Grid` mode.
  - plus `margin`/`offset`/`width`/`height`.
- **`TableState`** tracks the selected row (`selected` / `select` / `next` / `previous` / `select_first` / `select_last`) and selected column (`selected_column` / `select_column` / `next_column` / `previous_column`); the selected column is an index into the **full** column list. Import via `use ratatui_kit::prelude::*;`.
- **Auto height:** when `height` is left at the default, the component estimates the rendered height (wrapping + grid + footer). Wrap a tall table in a `ScrollView` and either let the built-in keys select while the page drives scrolling, or set `active: false` and drive selection externally from the page (see `examples/components/table.rs`).

```rust
let table_state = hooks.use_state(TableState::default);
Table<Deployment>(
    width: Constraint::Length(80),
    state: table_state,
    active: true,
    default_index: Some(0),
    columns: vec![
        TableColumn::new("Service", Constraint::Length(20)),
        TableColumn::new("Latency", Constraint::Length(9)).alignment(TableCellAlignment::Right),
    ],
    rows: deployments,
    render_row: Some(std::sync::Arc::new(|d: &Deployment, _selected| vec![
        TableCell::new(d.service),
        TableCell::new(format!("{}ms", d.latency)).alignment(TableCellAlignment::Right),
    ])),
    footer: vec![TableCell::new("2 services"), TableCell::new("avg 30ms").alignment(TableCellAlignment::Right)],
    border_mode: TableBorderMode::Grid,
    highlight_symbol: "▶ ",
    highlight_style: Style::new().bg(Color::Rgb(45, 55, 95)),
    column_highlight_style: Style::new().bg(Color::Rgb(70, 55, 25)),
    column_navigation: true,
    on_select: move |d: Deployment| { submitted.set(d.service.to_string()); },
)
```

---

## RouterProvider

- **Purpose:** provides routing context + history management for the app (multi-page, nested routes, dynamic params).
- **Feature:** `router` (`#[cfg(feature = "router")]`).
- **Layout component:** no.
- **Props** (`RouterProviderProps`, `Default`):
  - `routes: Routes` — the `routes!` macro expands to a `Vec<Route>`, which `element!` auto-converts to `Routes` via its `.into()` on prop assignment (there is an `impl From<Vec<Route>> for Routes`), so you pass the macro result directly.
  - `index_path: String` (home path; pass `&str`/`String` directly)
  - `history_length: Option<usize>` (default `10`; pass a bare `usize`)
  - `state: Option<RouteState>` (pass a bare `RouteState`)
- Children access routing via `hooks.use_navigate()` (`push`/`replace`/`back`/`forward`/`push_with_state`), `hooks.use_params()`, `hooks.try_use_route_state::<T>()`, etc.

```rust
let routes = routes! {
    "/" => AppShell {
        "/" => OverviewPage,
        "/projects/:slug" => ProjectDetailPage,
        "/projects" => ProjectsPage,
    },
};
RouterProvider(routes: routes, index_path: "/")
```

---

## Outlet

- **Purpose:** the nested-route outlet; renders the matching child-route component for the current path (like React Router's `<Outlet/>`). Declaration order matters: static-prefix routes should come before dynamic routes.
- **Feature:** `router` (`#[cfg(feature = "router")]`).
- **Layout component:** no.
- **Props:** `NoProps` (no fields; pass nothing).

```rust
// place inside the parent route component's layout
View(width: Constraint::Fill(1)) {
    Outlet
}
```

---

## Adapters (bridging native ratatui widgets)

`WidgetAdapter<T>` / `StatefulWidgetAdapter<T>` are exported from `components::adapter`, but you **usually do not write them by component name**. The `element!` macro provides dedicated sugar to embed native ratatui widgets:

- `widget(expr)` — render a stateless widget (requires `for<'a> &'a T: Widget` + `Clone + Unpin`); uses `WidgetAdapter<T>` internally.
- `stateful(widget, state)` — render a stateful widget (`state: State<T::State>`); uses `StatefulWidgetAdapter<T>` internally.

**Feature:** core. **Layout component:** no. The Props structs `WidgetAdapterProps<T> { inner: T }` and `StatefulWidgetAdapterProps<T> { inner: T, state: State<T::State> }` are normally macro-generated, not hand-written.

```rust
// stateless
Fragment { widget(paragraph) }
// stateful (state is State<ListState>)
stateful(list, state)
```

You may also hand-write a `Component` and call `drawer.render_widget(...)` / `drawer.render_stateful_widget(...)` directly in its `draw` method to render a native widget (see `DeployQueue` in `examples/advanced/custom_widget.rs`), bypassing the adapter components.

---

Source paths: components directory `crates/ratatui-kit/src/components/` (exports and feature gating in `mod.rs`); layout-field type definitions in `crates/ratatui-kit-macros/src/with_layout_style.rs`; feature definitions in `crates/ratatui-kit/Cargo.toml`; idiomatic usage in `examples/`.

## Button (core)

Mouse-driven action: left Down invokes `Handler<'static, ()>` and consumes the
press; no built-in keyboard activation. Props: `label: String`, `on_press`,
`variant: ButtonVariant` (Primary/Secondary/Danger), `disabled: bool`,
`width: Option<u16>` (label display width + 4), `style: Option<Style>`. Height
is 3 rows. `ButtonTheme` slots: primary/secondary/danger/disabled styles and
`hover_modifier`. Disabled passes through input and suppresses visible hover.
Enable capture even when the mounted Button is disabled: its hooks remain
registered to preserve call order. See [pointer-events.md](pointer-events.md).
