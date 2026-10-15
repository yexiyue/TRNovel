# Theming Reference

Full mechanics for `ratatui-kit`'s always-on theme protocol: every `Palette`
field, every built-in `*Theme` type's fields, how to make your own component
theme-aware, and where to go instead of hand-picking colors. For the
quick-start mechanics (4 ways to apply a palette), see `SKILL.md`'s *Visual
polish* section or `references/building-polished-uis.md` §3 — this file is
the deep reference those point to.

## Table of contents

1. [`Palette` — every field](#1-palette--every-field)
2. [Per-component `*Theme` reference](#2-per-component-theme-reference)
3. [Writing a theme-aware custom component](#3-writing-a-theme-aware-custom-component)
4. [`use_palette` / `use_component_theme` hooks](#4-use_palette--use_component_theme-hooks)
5. [Resolve chain (why overrides layer the way they do)](#5-resolve-chain)
6. [Persisting a theme (`serde` feature)](#6-persisting-a-theme-serde-feature)
7. [Don't hand-pick colors — `ratatui-kit-themes`](#7-dont-hand-pick-colors--ratatui-kit-themes)

---

## 1. `Palette` — every field

`Palette` is `#[non_exhaustive]` — construct it via `Palette::default()` then
set fields (a struct literal won't compile, including in your own downstream
code). Every field is a plain `ratatui::style::Color`.

| Field | Meaning | Default | Consumed by |
| --- | --- | --- | --- |
| `fg` | Primary text color | `Color::Reset` | `Text`, `Select`, `MultiSelect`, `TreeSelect`, `VirtualList`, `Input`, `SearchInput`, `Table` rows, modals' body text |
| `fg_dim` | De-emphasized text (secondary, disabled) | `DarkGray` | *no built-in component reads this directly* — it's yours, and `ratatui-kit-markdown` uses it for muted text |
| `bg` | App/root background | `Reset` | *no built-in component reads this* — apply it yourself at your root (e.g. `View(style: Style::new().bg(palette.bg))`), or it's what `ratatui-kit-themes`' theme background feeds |
| `surface` | Panel/container surface, one layer above `bg` | `Reset` | *no built-in component reads this either* — reserved for panels you draw yourself; `ratatui-kit-markdown` uses it for inline-code/blockquote/diff backgrounds |
| `overlay` | Modal backdrop base color | `Reset` | *not read by the built-in `Modal`* (its `ModalTheme` hardcodes a `DIM` modifier instead — see the table below) — reserved for a custom overlay treatment |
| `accent` | Interactive/brand color: cursors, key highlights | `Cyan` | `Input`/`SearchInput` cursor bg, `Table` header/footer, `ConfirmModal`'s selected button, `ShortcutInfoModal`'s key style, `MultiSelect` selected-item marker — note `Border`'s own theme does *not* derive from `accent` (`BorderTheme.border_style` ← `border`, always; an "active" look is something you set manually, e.g. `border_style: Style::new().fg(palette.accent)`) |
| `on_accent` | Foreground drawn **on top of** `accent` or `selection` backgrounds | `Black` | `Select`/`MultiSelect`/`TreeSelect`/`Table` highlight text, `Input`/`SearchInput` cursor glyph |
| `selection` | Selected-row/item background | `Cyan` | `Select`, `MultiSelect`, `TreeSelect`, `Table` highlight background (paired with `on_accent` as the foreground) |
| `border` | Default (inactive) border color | `DarkGray` | `Border`, `Select`, `MultiSelect`, `Table`, `SearchInput` (inactive state), `ConfirmModal`, `ShortcutInfoModal` — **not** `AlertModal` (its border comes from `warning` instead, always red-flag colored) |
| `border_active` | Focused/active border color | `Cyan` | `SearchInput`'s active-editing border |
| `success` | Success semantic color | `Green` | `SearchInput` validation-success border/status |
| `warning` | Warning semantic color | `Yellow` | `Select`/`MultiSelect` empty-state text, `AlertModal` |
| `error` | Error semantic color | `Red` | `SearchInput` validation-failure border/status |
| `info` | Informational semantic color | `Blue` | *no built-in component reads this* — reserved for your own use; `ratatui-kit-markdown` uses it for links and code-block language labels |
| `placeholder` | Placeholder/hint text | `DarkGray` | `Input`, `SearchInput` |

**`bg`/`surface`/`overlay`/`fg_dim`/`info` are real fields nobody in core
consumes by default** — don't be surprised when setting `palette.bg` alone
does nothing visible. They exist as a stable, semantic vocabulary for you to
apply at your own root/panels, and for extension crates
(`ratatui-kit-markdown`'s components genuinely read `surface`, `fg_dim`, and
`info`) — setting them is not wasted, it just isn't wired into the built-in
widget set the way `accent`/`border`/`selection` are.

**`on_accent` must stay readable against *two* different backgrounds** —
`accent` (`Input`/`SearchInput` cursor: `bg(accent).fg(on_accent)`) and
`selection` (`Select`/`Table` highlight: `fg(on_accent).bg(selection)`). If
you hand-pick a custom `accent`/`selection` pair, check `on_accent`'s
contrast against *both*, not just one — the default palette's `accent` and
`selection` happen to be the same color (`Cyan`) so this never bites there,
but a custom palette with visually distinct `accent`/`selection` easily
produces a combo where `on_accent` reads fine on one and is nearly invisible
on the other.

---

## 2. Per-component `*Theme` reference

Every built-in component's default style is `T::from_palette(&palette)` for
its own `FooTheme`. All are `Clone + Default + 'static` (required by
`ComponentTheme`) and live in `prelude::*`. Use this table to know what a
`ThemeOverride::<FooTheme>` can touch without reading the source.

| Component | Theme type | Fields (derivation) |
| --- | --- | --- |
| `Text` | `TextTheme` | `style` ← `fg` |
| `Border` | `BorderTheme` | `border_style` ← `border`; `style` (interior area, blank by default) |
| `Select` | `SelectTheme` | `style` ← `fg`; `border_style` ← `border`; `highlight_style` ← `on_accent` on `selection`; `empty_style` ← `warning` |
| `MultiSelect` | `MultiSelectTheme` | `style` ← `fg`; `border_style` ← `border`; `highlight_style` ← `on_accent` on `selection` (cursor row); `selected_item_style` ← `accent` (checked marker); `empty_style` ← `warning` |
| `TreeSelect` (feature `tree`) | `TreeSelectTheme` | `style` ← `fg`; `highlight_style` ← `on_accent` on `selection` |
| `VirtualList` (feature `virtual-list`) | `VirtualListTheme` | `style` ← `fg` |
| `Input` (feature `input`) | `InputTheme` | `cursor_style` ← `accent` bg / `on_accent` fg; `placeholder_style` ← `placeholder`; `style` ← `fg` |
| `SearchInput` (feature `input`) | `SearchInputTheme` | `border_style` ← `border`; `active_border_style` ← `border_active`; `success_border_style` ← `success`; `error_border_style` ← `error`; `input_style` ← `fg`; `placeholder_style` ← `placeholder`; `cursor_style` ← `accent` bg / `on_accent` fg; `success_status_style` ← `success`; `error_status_style` ← `error` |
| `Table` (feature `table`) | `TableTheme` | `header_style`/`footer_style` ← `accent`; `row_style` ← `fg`; `highlight_style` ← `on_accent` on `selection`; `column_highlight_style`/`cell_highlight_style` (blank by default); `border_style`/`horizontal_line_style` ← `border` |
| `Modal` | `ModalTheme` | `style` — hardcoded `Modifier::DIM`, **ignores the palette entirely** (not derived from `overlay` or anything else) |
| `ConfirmModal` | `ConfirmModalTheme` | `border_style` ← `border`; `title_style`/`content_style`/`button_style` ← `fg`; `selected_button_style` ← `accent` + `BOLD` |
| `AlertModal` | `AlertModalTheme` | `border_style`/`title_style` ← `warning`; `message_style` ← `fg` |
| `ShortcutInfoModal` | `ShortcutInfoModalTheme` | `border_style` ← `border`; `title_style`/`section_title_style`/`description_style` ← `fg`; `key_style` ← `accent` |

Every style-typed prop on these components follows the same override
contract: `None` → theme, `Some(style)` → `theme.patch(style)`,
`Some(Style::reset())` → clear that slot. `ThemeOverride::<FooTheme>` injects
a whole replacement `FooTheme` for every matching component in the subtree
(see *Resolve chain* below) rather than patching individual props.

---

## 3. Writing a theme-aware custom component

Any custom component — hand-written `Component` or `#[component]` fn — can
plug into the same protocol so it recolors along with everything else instead
of carrying hardcoded `Style`s. Define your own `FooTheme` implementing
`ComponentTheme`, then read it with `use_component_theme`:

```rust
use ratatui_kit::prelude::*;
use ratatui_kit::ratatui::style::{Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BadgeTheme {
    style: Style,
}

impl ComponentTheme for BadgeTheme {
    fn from_palette(palette: &Palette) -> Self {
        Self {
            style: Style::new()
                .fg(palette.on_accent)
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD),
        }
    }
}

// `ComponentTheme` requires `Default` — write it in terms of `from_palette`
// rather than `#[derive(Default)]`, which would zero every field instead of
// giving you the same look as "no PaletteProvider in scope" (every built-in
// FooTheme follows this convention; ComponentTheme's resolve chain — §5 —
// relies on Self::default() == Self::from_palette(&Palette::default())).
impl Default for BadgeTheme {
    fn default() -> Self {
        Self::from_palette(&Palette::default())
    }
}

#[derive(Props, Default)]
struct BadgeProps {
    label: String,
}

#[component]
fn Badge(hooks: Hooks, props: &BadgeProps) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<BadgeTheme>();
    element!(Text(text: props.label.clone(), style: theme.style))
}
```

For a hand-written `Component` (not `#[component]`), use
`ComponentUpdater::use_component_theme::<T>()` inside `update` instead of the
`Hooks` version — see *hooks* below for why they're two different methods on
two different types.

---

## 4. `use_palette` / `use_component_theme` hooks

Both live on the `UseTheme` trait (function components, via `Hooks`) and as
inherent methods on `ComponentUpdater` (hand-written `Component::update`,
where you don't have a `Hooks` handle in the same way):

```rust
// In a #[component] fn:
fn use_palette(&self) -> Palette;                      // owned copy of the current Palette
fn use_component_theme<T: ComponentTheme>(&self) -> T;  // resolved FooTheme (see resolve chain)

// In a hand-written Component::update(&mut self, props, hooks, updater: &mut ComponentUpdater):
impl ComponentUpdater<'_, '_> {
    pub fn use_palette(&self) -> Palette;
    pub fn use_component_theme<T: ComponentTheme>(&self) -> T;
}
```

Both are **passive reads** — they don't themselves register a waker. A
component only re-renders when something else triggers a re-render (a
`use_state`/`Atom` write, an event, …) and *then* re-reads whatever the
current `Palette` is that frame. This is why runtime theme switching works by
writing the `Palette` into reactive state (`use_state`/`Atom`) that drives
`PaletteProvider`, not by expecting `use_palette` alone to wake anything.

`use_palette` with no `PaletteProvider` in scope returns `Palette::default()`
— never panics, never requires a provider. Same fallback logic applies inside
`use_component_theme`'s resolve chain (next section).

---

## 5. Resolve chain

Every `FooTheme` a component reads goes through the same three-step chain,
each step overriding the one before it:

1. **Explicit `ThemeOverride<FooTheme>` context**, if the component sits
   inside one — wins outright, ignores the ambient `Palette` entirely.
2. **`FooTheme::from_palette(&palette)`**, where `palette` comes from the
   nearest `PaletteProvider` (or `Palette::default()` if none).
3. **`FooTheme::default()`** — only reachable if `from_palette` itself is
   never called, which in practice means the same as step 2 with the default
   palette (see the `Default` convention in §3).

This is why `ThemeOverride::<BorderTheme>(theme: t) { Select { ... } }`
doesn't do anything to the `Select` inside it — `Select` reads
`SelectTheme`, not `BorderTheme`; a `ThemeOverride` only affects components
reading that *exact* theme type.

---

## 6. Persisting a theme (`serde` feature)

`Palette` derives `Serialize`/`Deserialize` behind the `serde` feature
(`ratatui-kit = { version = "0.12", features = ["serde", ...] }`), useful for
saving a user's chosen theme to a config file between runs:

```rust
let palette: Palette = serde_json::from_str(&saved_config)?;
element!(PaletteProvider(palette: palette) { /* ... */ })
```

This is `Palette`-only — none of the built-in `*Theme` types derive
`serde` traits (they're always cheaply re-derivable from a `Palette` via
`from_palette`, so there's nothing to persist there beyond the palette
itself, unless you're using `ThemeOverride` for a permanent per-component
customization your app wants to remember, in which case serialize your own
wrapper struct around the fields you changed).

---

## 7. Don't hand-pick colors — `ratatui-kit-themes`

Before manually tuning `Palette` fields for a nice-looking scheme, consider
the official extension crate
[`ratatui-kit-themes`](https://github.com/yexiyue/ratatui-kit-contrib/tree/main/crates/ratatui-kit-themes)
(`ratatui-kit-contrib`'s sibling to `ratatui-kit-markdown`) — it converts the
[`ratatui-themes`](https://crates.io/crates/ratatui-themes) catalog (Dracula,
Nord, Tokyo Night, Catppuccin, Gruvbox, Solarized, and more — 15 presets) into
a `Palette` you feed straight into `PaletteProvider`, no design work required:

```toml
[dependencies]
ratatui-kit-themes = "0.1"
```

```rust
use ratatui_kit_themes::{IntoKitPalette, ThemeName};

let palette = ThemeName::Dracula.into_kit_palette();
element!(PaletteProvider(palette: palette) { /* whole app */ })
```

It has its own skill (`ratatui-kit-contrib`) with the full API (theme
cycling, terminal-background mode, all 15 preset names) — install it
alongside this one if you're reaching for named presets instead of hand-tuned
colors. It's a separate, officially-maintained crate published on its own
schedule, not part of `ratatui-kit` core — the core `Palette`/`PaletteProvider`
protocol it targets is exactly what's documented in this file.
