# Macros & element! Syntax Reference

What this file covers: the `element!` declarative macro grammar, in-block control flow, embedded `{ expr }` blocks, the `widget()` / `stateful()` adapters, and the procedural macros `#[component]`, `#[derive(Props)]` / `NoProps`, `#[with_layout_style]`, and `routes!`.

All snippets assume `use ratatui_kit::prelude::*;`. The macros expand to `::ratatui_kit::...` paths; inside the library itself this works because of `extern crate self as ratatui_kit;`.

## Table of contents

1. [`element!` macro syntax](#1-element-macro-syntax)
2. [`widget(expr)` / `stateful(widget, state)` adapters](#2-widgetexpr--statefulwidget-state-adapters)
3. [`#[component]`](#3-component)
4. [`#[derive(Props)]` and `NoProps`](#4-deriveprops-and-noprops)
5. [`#[with_layout_style]`](#5-with_layout_style)
6. [`routes!` macro (`router` feature)](#6-routes-macro-router-feature)

---

## 1. `element!` macro syntax

### Basic shape: `Comp(prop: val, ...) { children }`

- The tag is a **component type path** (`View`, `Border`, `my_mod::Foo`), not a string.
- Inside `(...)` are props: comma-separated `field_name: expression` pairs. Each value is automatically wrapped in `.into()` by the macro (codegen emits `(#expr).into()`), so `text: "hi"`, `top_title: Line::from(...)` (auto `Some`), and `wrap: true` (auto `Some(true)`) all work. Unspecified fields are filled by `..Default::default()` — therefore **the props type must implement `Default`**.
- `{ ... }` is the children block; omit the whole block when there are no children.
- The macro takes a single element (or a single `widget` / `stateful` adapter). `element!(Comp)` is also valid (no props, no children).

```rust
element!(
    Border(
        flex_direction: Direction::Vertical,
        justify_content: Flex::Center,
        gap: 1,
        border_style: Style::new().cyan(),
        top_title: Line::from(" title ").cyan().bold().centered(),
    ) {
        Text(text: "hello", style: Style::new().green())
        View(height: Constraint::Length(1)) {
            Text(text: "nested child")
        }
    }
)
```

The return type is `Element<Border>`; it is typically `.into()`-ed into `AnyElement`, or for the top level you call `.fullscreen().await` / `.into_any()`.

### `key:` — element identity key

`key` is a reserved field (you cannot declare a field with the same name in a Props struct). During reconciliation it lets the runtime reuse the `InstantiatedComponent` (and preserve its state) when the key + type are unchanged across frames. Any value that is `Hash`able / can go into `ElementKey::user` works (integers, `&str`, slugs, etc.). **Elements inside a loop should carry a `key`.**

```rust
for (index, project) in PROJECTS.into_iter().enumerate() {
    ProjectRow(project: project, active: index == sel, key: project.slug)
}
// or key: index
```

### `..rest` — props spread

`..expr` splats an entire props value into the call. It **must be the last item inside the parentheses**, and when present the macro does **not** append `..Default::default()` (the spread supplies the remaining fields).

```rust
let queue_props = DeployQueueProps { state: list_state };
element!(DeployQueue(..queue_props))
// You can also mix explicit fields: Comp(active: true, ..base_props)
```

### First-class control flow in the children block

In a children position you can write `if` / `if let` / `for` / `match` directly. **Each branch body is itself a group of children.** Each branch independently `extend`s its elements into the children list, so **different branches may return different element types — no `.into_any()` unification is needed.**

`if` / `else if` / `else` (conditions need no parentheses; `if let` is the same):

```rust
if count_value % 2 == 0 {
    Text(
        text: format!("if branch: count {count_value} is even"),
        style: Style::new().green().bold(),
    )
} else {
    Border(borders: Borders::ALL, height: Constraint::Length(3)) {
        Text(
            text: format!("else branch: count {count_value} is odd"),
            style: Style::new().yellow(),
        )
    }
}

if let Some(name) = maybe_name {
    Text(text: format!("if let branch: optional name = {name}"))
} else if count_value > 4 {
    Text(text: "else if branch: no name, but count is high")
} else {
    Text(text: "else branch: no name yet")
}
```

`for` (pattern `in` iterable; give a `key` in the branch body):

```rust
for (index, (label, detail)) in ROWS.iter().enumerate() {
    Text(
        key: index,
        text: format!("{} {label:<10} {detail}", if index == selected_value { ">" } else { " " }),
        style: if index == selected_value {
            Style::new().black().on_cyan()
        } else {
            Style::new()
        },
    )
}
```

`match` (each arm body **must be wrapped in `{}`**; supports `A | B` patterns and `if` guards; commas between arms are optional):

```rust
match selected_value {
    0 => { Text(text: "match branch 0: simple Text") }
    1 => { Border(borders: Borders::LEFT, height: Constraint::Length(3)) {
        Text(text: "match branch 1: wrapped in a left border")
    } }
    2 => { Text(text: "match branch 2: cyan", style: Style::new().cyan()) }
    _ => { Text(text: "match default: every branch can have its own element type") }
}
```

### `{ expr }` — embed an arbitrary Rust expression

A brace block holds any Rust expression (multiple statements allowed), inserted at a children position. It goes through `extend_with_elements`, which accepts these return types:

- a single `Element` / `AnyElement`
- `Option<Element>` (`None` renders nothing)
- `Vec<AnyElement>` and similar element collections
- `impl Iterator<Item = Element>`

A common pattern: outside the `element!` call, `.map(...).collect()` your data into a `Vec<Line<'static>>` or `Vec<AnyElement>`, then inject it inside the children block via `{ list_lines }` / `{ items }`. Prefer the first-class `for` for simple loops; use `{ expr }` for complex or lazy expressions.

```rust
let rows: Vec<AnyElement> = data.iter()
    .map(|d| element!(Text(text: d.title.clone())).into_any())
    .collect();
element!(View { { rows } })
```

> Removed legacy syntax (writing it will fail to compile): `$widget` → use `widget(...)`; `#(expr)` children → use `{ expr }`.

> **Footgun — a `{ … }` written *immediately after a self-closing component* is consumed as that component's children block, not a sibling embed.** `SearchInput(props) { if cond { … } }` makes the brace `SearchInput`'s children — and a children block parses `if`/`for`/`match` as **first-class control flow over element children**, so an `element!(…)` or arbitrary Rust expression inside fails with a cryptic `expected identifier`. This bites hardest when mechanically migrating `#(if … )` → `{ if … }`. The fix: write a **sibling conditional with first-class control flow** — no wrapping braces, native element children, and *no* inner `element!(…)`/`.into_any()`:
>
> ```rust
> element!(View {
>     SearchInput(props)
>     if cond {
>         Border(/* … */) { Center { Text(text: "empty") } }   // native children, not element!(Border(…))
>     } else {
>         TreeSelect<T>(/* … */)
>     }
> })
> ```
>
> Reserve the `{ expr }` embed for injecting *pre-computed values* (a `Vec`, an iterator, a variable) — not for nesting `element!` calls or returning elements from control flow.

---

## 2. `widget(expr)` / `stateful(widget, state)` adapters

These bridge **any native ratatui widget** into a children position without hand-writing a `Component` (escape hatch).

- `widget(expr)` — `expr` is a value implementing ratatui's `Widget` (exactly one expression). Expands to `Element<WidgetAdapter<_>>` with prop `inner: expr`.
- `stateful(widget, state)` — bridges a `StatefulWidget`; `widget` is the widget value, `state` is its state (exactly the two items `widget, state`). Expands to `Element<StatefulWidgetAdapter<_>>` with props `inner` + `state`.

```rust
// widget: this is how the Text component bridges a Paragraph internally
let paragraph = TextParagraph::from(Paragraph::new("hi"));
element!(Fragment { widget(paragraph) })

// stateful: how Select bridges a ratatui List + ListState internally
element!(Border(...) {
    if is_empty {
        Center { Text(text: "No data") }
    } else {
        stateful(list, state)   // list: ratatui::widgets::List, state: ListState / State<ListState>
    }
})
```

Adapters may only appear in a children position of `element!` (or as the single argument of `element!()`).

> **`widget(expr)` requires `for<'a> &'a T: Widget`** — it renders the widget *by reference* each frame (and additionally `T: Clone + Unpin`). Widgets that only impl `Widget` **by value** (e.g. `tui-big-text`'s `BigText`, and historically several ratatui widgets — even the framework's own `TextParagraph` carries a hand-written `&TextParagraph: Widget` impl to satisfy this) fail with `the trait bound for<'a> &'a T: Widget is not satisfied`. Two fixes: **(a)** use a widget version that impls `WidgetRef` (which yields `&T: Widget`), or **(b)** wrap it in a newtype that impls `Widget` for the *reference* by cloning:
> ```rust
> struct ByRef<W>(W);
> impl<W: Widget + Clone> Widget for &ByRef<W> {
>     fn render(self, area: Rect, buf: &mut Buffer) { self.0.clone().render(area, buf); }
> }
> // then: widget(ByRef(big_text))
> ```
> `stateful(w, s)` likewise needs `for<'a> &'a T: StatefulWidget`.

---

## 3. `#[component]`

Rewrites `fn Foo(...) -> impl Into<AnyElement<'static>>` into a unit struct plus a `Component` impl. The function body moves into an internal `implementation`, and the element it returns becomes the component's single child.

Parameter conventions (**only these names are recognized**; order is free, both may be omitted):

- `props` or `_props`: **must be a reference** — `&FooProps` (read-only) or `&mut FooProps`. The type is this component's Props. If omitted, the Props type is `NoProps`.
- `hooks` or `_hooks`: `Hooks` (by value) or `&mut Hooks`. To call hooks (`use_state` / `use_future` / `use_event_handler` / ...) you usually write `mut hooks: Hooks`.
- Any other parameter name/shape is a compile error (`expected `props` or `hooks``).

The return type is fixed: `impl Into<AnyElement<'static>>`. Construct the returned value inside the body with `element!(...)`.

```rust
#[component]
fn Counter(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut count = hooks.use_state(|| 0_u64);
    element!(Text(text: format!("Counter: {}", count.get())))
}

#[component]
fn ProjectRow(props: &ProjectRowProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    element!(View(height: Constraint::Length(1)) {
        Text(text: props.project.name)
    })
}
```

Generic components: add type parameters to the function name (the Props carries the same parameter):

```rust
#[component]
pub fn Select<T>(props: &mut SelectProps<T>, mut hooks: Hooks)
    -> impl Into<AnyElement<'static>>
where T: Into<ListItem<'static>> + Clone + Send + Sync + 'static { ... }
```

**Transparent layout (important):** the `update` generated by `#[component]` calls `updater.set_transparent_layout(true)`. The function component therefore **does not occupy its own layout node** — it inherits the `LayoutStyle` of the root element it returns. So **layout props (`width` / `height` / `flex_direction` / `gap` / `margin` / ...) must be written on the root element returned by `element!`.** Putting them on the tag that *uses* the function component has no effect on its internal layout.

---

## 4. `#[derive(Props)]` and `NoProps`

Props is the component's attribute struct and must implement the `Props` trait — generated via `#[derive(Props)]` (the impl is an empty marker trait). Rules:

- Only **named-field structs or unit structs** are supported (tuple structs error).
- No field may be named `key` (reserved for the element identity key).
- Usually paired with `#[derive(Default)]` (`element!` uses `..Default::default()` to fill unspecified fields); when the struct is generic and cannot auto-derive, write `impl Default` by hand.

```rust
#[derive(Default, Props)]
struct ProjectRowProps {
    project: Project,
    active: bool,
}
```

**Components with no props:** simply omit the `props` parameter and the macro sets the Props type to `NoProps` — no custom struct needed:

```rust
#[component]
fn ActivityPage(_hooks: Hooks) -> impl Into<AnyElement<'static>> {
    element!(Border { Text(text: "no props here") })
}
// Call site: element!(ActivityPage)
```

`NoProps` is a library-provided empty props type (already implements `Props + Default`); when hand-writing a `Component` you can also set `type Props<'a> = NoProps;`.

---

## 5. `#[with_layout_style]`

Apply this to a Props struct (**before `#[derive(Props)]`**) to inject layout fields and generate `layout_style(&self) -> LayoutStyle`. This is the standard way for a component to gain layout capability (`View`, `Border`, etc. all use it).

- Bare `#[with_layout_style]` injects **all 7** fields: `margin: Margin`, `offset: Offset`, `width: Constraint`, `height: Constraint`, `gap: i32`, `flex_direction: Direction`, `justify_content: Flex`.
- With arguments you can select a subset: `#[with_layout_style(margin, offset, width, height)]` (only those 7 names are valid; anything else errors).

```rust
#[with_layout_style]
#[derive(Default, Props)]
pub struct ViewProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    // margin / offset / width / height / gap / flex_direction / justify_content injected by the macro
}
```

Wire it to the layout node inside `Component::update`:

```rust
fn update(&mut self, props: &mut Self::Props<'_>, _hooks: Hooks, updater: &mut ComponentUpdater) {
    updater.set_layout_style(props.layout_style());
    updater.update_children(&mut props.children, None);
}
```

After that, these fields can be passed as ordinary props in `element!`: `View(flex_direction: Direction::Vertical, gap: 1, width: Constraint::Fill(1)) { ... }`.

---

## 6. `routes!` macro (`router` feature)

Declares a route tree and returns `Vec<Route>` (handed to `RouterProvider`). Syntax: `"path" => Component` or `"path" => Component(prop: val)`, optionally followed by `{ child routes... }`, with **commas separating entries at the top level and at each level**.

`RouterProviderProps.routes` is typed `Routes`; the conversion is automatic only because `element!` wraps prop values in `.into()` and `Routes: From<Vec<Route>>`. If you build the props OUTSIDE `element!` (or pass `routes!` output to a function expecting `Routes`), call `.into()` on the `routes!` output yourself.

- The left side is a **path string literal**; dynamic segments `:slug` are supported (compiled into matching regex at construction time).
- The right side reuses the `element!` "head" (type + optional props), but `{}` is **nested child routes, not children**.
- `key:` is **forbidden** on route elements (route identity is determined by the path; writing one errors).
- Where a nested page renders is determined by the `Outlet` in the parent component.

```rust
let routes = routes! {
    "/" => AppShell {
        "/" => OverviewPage,
        "/projects" => ProjectsPage,
        "/projects/:slug" => ProjectDetailPage,
        "/activity" => ActivityPage,
    },
};

element!(RouterProvider(routes: routes, index_path: "/"))
    .fullscreen()
    .await?;
```

The parent component uses `Outlet` as a placeholder for the nested page; hooks fetch navigation / params / route state:

```rust
#[component]
fn AppShell(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut navigate = hooks.use_navigate();   // navigate.push("/projects") / .back() / .replace(...) / .push_with_state(path, state)
    element!(View { Outlet })                  // Outlet renders the currently matched child route
}

#[component]
fn ProjectDetailPage(hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let slug = hooks.use_params().get("slug").cloned().unwrap_or_default();
    // hooks.try_use_route_state::<RouteNotice>() retrieves state carried by push_with_state
    element!(Text(text: format!("slug = {slug}")))
}
```

---

Relevant source: `crates/ratatui-kit-macros/src/` (`element.rs` / `component.rs` / `props.rs` / `router.rs` / `with_layout_style.rs` / `adapter.rs` / `utils.rs` / `lib.rs`). Real usage in `examples/core/control_flow.rs`, `examples/start/counter.rs`, `examples/routing/router.rs`, `examples/advanced/custom_widget.rs`, and `crates/ratatui-kit/src/components/{view.rs,border.rs,text.rs,select.rs,fragment.rs}`.
