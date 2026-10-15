# Pointer interactions

Read when adding mouse actions, hover, dragging, or a mouse-driven modal. Source:
`crates/ratatui-kit/src/hooks/use_pointer.rs`, `components/button.rs`,
`input/mod.rs`; runnable composition: `examples/input/mouse.rs`.

## Entry and API choice

```rust
use ratatui_kit::{prelude::*, crossterm::event::MouseButton};
// In async main:
element!(App).fullscreen_with(RunOptions { mouse: true }).await?;
// Inline viewport:
element!(App).render_loop_with(terminal_options, RunOptions { mouse: true }).await?;
```

Mouse APIs are core, not `input`-feature gated. Capture defaults off. Pointer
registration without capture asserts in debug builds. Enabled modes are restored
on exit. Hover / track_move requests any-motion after the first relevant update,
and it stays enabled until exit. Terminal support determines whether hover works.

| Need | Prefer |
| --- | --- |
| Visible action on left Down | `Button` |
| Down + Up inside same component | `use_click` |
| Boolean hover feedback | `use_hover` |
| Left-button drag phases | `use_drag` |
| Item hover for a custom widget | `use_hover_row` |
| Rendered draggable track for custom content | `use_scrollbar` |
| Event filtering / propagation control | `use_pointer` |

Button has no keyboard activation/focus. Add separate shortcuts calling the same
business action. Button props and `ButtonTheme` are listed in `components.md`.

## Signatures and event types

```rust
fn use_click<F>(&mut self, f: F) where F: FnMut(Click) + 'static;
fn use_hover(&mut self) -> bool;
fn use_drag<F>(&mut self, f: F) where F: FnMut(DragUpdate) + 'static;
fn use_pointer<F>(&mut self, options: PointerOptions, f: F)
    where F: FnMut(PointerEvent) -> EventResult + 'static;
fn use_hover_row<T, F>(&mut self, hit: F) -> State<Option<T>>
    where T: PartialEq + Send + Sync + Unpin + 'static,
          F: Fn(Position) -> Option<T> + 'static;
fn use_scrollbar<F>(&mut self, options: ScrollbarOptions, on_seek: F)
    where F: FnMut(usize) + 'static;
```

Hook callbacks need no Send/Sync. Prop callbacks such as Button.on_press retain
`Handler<'static, ()>`'s Send + Sync requirement. Keep hook order stable,
including disabled or externally controlled components. Do not call a fallback
`use_state` only inside `Option::unwrap_or_else`; initialize the local state every
frame, then choose `props.state.unwrap_or(local)`.

- `PointerOptions { buttons: MouseButtons, track_move: bool, layer: Option<InputLayer> }`: default LEFT,
  no movement tracking; `layer: None` uses the nearest input layer. Combine sets with `.union`; Scroll ignores the button set.
- `PointerEvent { kind, position, screen_position, modifiers, area }`: no separate button field.
  Button identity is in `PointerKind::Down/Up/Drag(MouseButton)` or
  `Click { button, count }`. Other kinds: Move, Enter, Leave, ScrollUp/Down/Left/Right.
- `Click { count, button, position, modifiers }`: use_click accepts all buttons;
  300ms nearby consecutive clicks count 1, 2, 3 (capped). Each release invokes the
  callback immediately; double-click logic must account for the preceding count 1.
- `DragUpdate { phase, button, from, to, modifiers }`: use_drag is left-only.
  `DragPhase::Cancel` reports a hidden or blocked target; clear transient drag
  feedback without committing a drop. Raw pointer handlers receive
  `PointerKind::Cancel(button)` in that case.
  Phases Start/Move/End; `from` is the previous event, not the gesture origin.

PointerEvent.position and area share the component rendering coordinates, including
ScrollView content coordinates. The runtime maps screen hits through nested scroll
offsets and clips them to visible viewports. screen_position preserves absolute
terminal coordinates; raw crossterm events also retain terminal coordinates.
Click.position and DragUpdate.from/to use terminal coordinates. For a local width,
apply deltas rather than assigning `to.x` (margins/offsets otherwise introduce jumps):

```rust
hooks.use_drag(move |d| {
    if d.phase == DragPhase::Move {
        let delta = i32::from(d.to.x) - i32::from(d.from.x);
        split.set((i32::from(split.get()) + delta).clamp(10, 60) as u16);
    }
});
```

## Delivery rules that affect composition

- Consuming Down in a hit-tested pointer handler captures that button. Subsequent
  Drag/Up go exclusively to the captor before Global/layer handlers, even outside
  its region. Up releases; unmount or inactive layer invalidates capture.
- Otherwise Global handlers run first. Active-layer z order precedes priority.
  Equal-priority mouse delivery uses reverse draw order (child/later sibling
  first); keyboard delivery keeps registration order.
- `Consumed` stops propagation for hit events; `Ignored` continues it.
  Enter/Move/Leave observers all receive updates independently; their Consumed
  result requests repaint without suppressing another observer. use_click consumes
  press/release. For custom pass-through use_pointer and return Ignored.
- use_pointer synthesizes Click after Up if it can pair the press. Consuming Down
  ensures capture preserves the release delivery; release outside does not click.
- Hover respects active input layers and updates even if a Global consumer stops
  Moved delivery. Unchanged hover + unconsumed Moved can skip redraw; write State
  for visible changes rather than relying on every move redrawing.
- Hit testing uses the last draw's frame-stamped rectangle. Hits are transformed and
  clipped through every ancestor scroll viewport. Nested ScrollViews do not hand wheel events outward
  at their limits. Pointer input does not establish keyboard focus.

## Built-in component mouse behaviors

All interactive built-ins register pointer handlers **only when the app
enabled mouse capture** (conditional registration is stable across frames
because `mouse_enabled` never changes at runtime), and wheel/click follow
the `active` gate just like keyboard keys. TreeSelect and VirtualList keep
visual hover available when navigation is handled by a business wrapper:

| Component | Hover | Click | Wheel |
| --- | --- | --- | --- |
| Select / MultiSelect | row highlight (`hover_style`, default surface bg + BOLD; cursor row and checked rows keep their own styles) | select+confirm / toggle check | move selection |
| Table | row highlight via draw-recorded row rects (wrap heights & separators exact) | row select+confirm | move selection |
| TreeSelect | row highlight via post-render buffer patch (`hover_style`; exact row table from `flatten` + `TreeItem::height` + `get_offset`, multi-line items included) | leaf = select+confirm, branch = select / click-again toggles expand | viewport scroll |
| VirtualList | caller-drawn via `render_item` (`VirtualItemContext::is_hovered` is fed by the component) | hit-tested select+confirm | move selection |
| ScrollView | scrollbar thumb | scrollbar drag / track jump | scroll |
| Input | — | `on_seek(char index)` positions the cursor (caller-owned state) | — |
| SearchInput | — | activates editing when idle; positions the cursor while editing | — |
| TextArea (`textarea` feature) | — | positions the cursor on the clicked wrapped row (before the clicked cell) | — |
| ConfirmModal / AlertModal / ShortcutInfoModal | — | buttons fire on Down; unhandled clicks hit the scrim closer (cancel / close / close) | — |

TreeSelect's optional `scrollbar: Some(ScrollbarStyle::default())` is draggable
even with `active: false`; dragging only moves the viewport, without selecting
or confirming nodes. Scrollbars share `hover_style` and `drag_style` thumb
feedback across TreeSelect, ScrollView, and `use_scrollbar`.

`Button` fires on left Down with built-in hover emphasis. List row→index
mapping for `Select`/`MultiSelect` uses actual ListItem heights and excludes
borders and trailing blank rows. `VirtualList` uses `tui_widget_list` hit-testing
for variable heights. When wiring your own handlers around these, remember the **match
scrutinee read-guard deadlock**: `match state.read().hit_test(..) { ..
state.write() .. }` holds the read guard through the whole match — collect
into a `let` first.

## Scrollbar dragging (ScrollView)

ScrollView scrollbars are draggable out of the box when mouse capture is on
(grab the thumb to drag; click the track to jump the thumb to the pointer).
The drag handler registers only when the app enabled mouse capture, and it is
NOT gated by `active` — grabbing the thumb is an explicit scroll intent.
Geometry mapping is recorded per render and shared by ScrollView and the custom
scrollbar hook. `ScrollbarStyle` supplies styles, optional symbols, and an
`arrows` switch; rendering and hit geometry use that same configuration.
Drag survives leaving the track via pointer capture (main-axis coordinate only).

### ScrollbarStyle field types

These are configuration fields, not optional component style overrides:

| Field | Type | Default |
| --- | --- | --- |
| `style`, `thumb_style`, `track_style` | `Style` | Default style |
| `hover_style` | `Style` | BOLD modifier |
| `drag_style` | `Style` | BOLD + REVERSED modifiers |
| `symbols` | `Option<ratatui::symbols::scrollbar::Set<'a>>` | None; orientation defaults |
| `arrows` | `bool` | true |

```rust
let scrollbar = ScrollbarStyle {
    hover_style: Style::new().fg(palette.accent),
    drag_style: Style::new().fg(palette.on_accent).bg(palette.accent),
    ..Default::default()
};
```

## Custom scrollbars: the `use_scrollbar` hook

For fully custom-rendered content that `ScrollView` can't wrap, the
`use_scrollbar` hook gives you a rendered, draggable scrollbar in one call:
place it on the component whose area the track hugs, feed it item units
(`content_length` = total, `viewport_length` = visible, `position` = current
offset), and write the sought
offset back in `on_seek` (the reactive state handle must be mutable):

```rust
hooks.use_scrollbar(
    ScrollbarOptions {
        content_length: total_lines,
        viewport_length: viewport_lines,
        position: current_offset,
        placement: TrackPlacement::RightInward(1), // default; 4 edges, both axes
        scrollbar: ScrollbarStyle::default(),       // styles, symbols, arrows
    },
    move |target| scroll_target.set(target),
);
```

The hook renders a ratatui `Scrollbar` each frame (skipped when content fits)
and registers grab-drag / track-jump pointer handling only when mouse capture
is on. Thumb math uses the same internal geometry as ScrollView — both track ends
map exactly to `0` / `content_length - viewport_length`.

## Hover-row tracking for custom widgets: the `use_hover_row` hook

Components that drive a list widget directly (e.g. `tui_widget_list` without
the framework's `VirtualList`) can get Select-style hover tracking in one
call: provide a hit closure mapping a pointer position to a row identifier,
read the returned `State` from the render path:

```rust
let hover = hooks.use_hover_row(move |pos| {
    let hit = state.read().hit_test(pos.x, pos.y); // collect before matching!
    match hit {
        Some(Hit::Item(index)) if !loading && index < item_count => Some(index),
        _ => None,
    }
});
// in your builder closure: hover.get() == Some(context.index)
```

The hook registers a `track_move` pointer stream only when mouse capture is
on, updates the State on Enter/Move (writes only on change — mouse moves are
frequent), clears it on Leave, and gates nothing else (put `active`/count
gating inside the hit closure by returning `None`).

**Hand-written `Component::update` gotcha**: pointer hooks read
`SystemContext` through the component context stack. `#[component]` fn
components get it automatically; a hand-written `Component` must bind it
first or `pointer_enabled` returns false and `use_hover_row`/`use_pointer`
cannot resolve the runtime. Direct pointer hooks diagnose missing context;
managed hooks may skip registration:

```rust
fn update(&mut self, props: &mut Self::Props<'_>, mut hooks: Hooks,
          updater: &mut ComponentUpdater) {
    let mut hooks = hooks.with_context_stack(updater.component_context_stack());
    let hover = hooks.use_hover_row(/* .. */);
    // ..
}
```

## Modal dismissal

Each frame declare `let layer = hooks.use_input_layer(open, true)`, register the
closer with `EventScope::Layer(layer)`, and pass `Modal(layer: Some(layer))`.
Use Low priority for a scrim closer with `EventOptions { hit_test: false }`.
Buttons inside consume Down first; unhandled scrim/panel blank clicks close.
Disabled buttons pass through and can reach the closer. Add a panel pointer
handler consuming Down if panel blanks should keep the modal open. Mount it as
its own component so its hit area matches the panel; do not attach the handler
to the whole root screen. Keep layer handles within the current frame.

Stationary pointers are checked after layout changes: moving, scrolling, resizing,
or opening an exclusive input layer updates Enter/Leave and use_hover. Managed
row hover also rechecks changed content without requiring another mouse movement.
Callbacks should only update hover state when its value changes.


## Integration lessons and migration to 0.12

1. A ratatui `Scrollbar` only paints. Use ScrollView, TreeSelect's `scrollbar`, or
   `use_scrollbar` for interaction. `ScrollbarStyle` replaces opaque widget props
   so symbols, arrows, geometry and feedback have one source. Arrow taps step one
   content unit; track taps jump; thumb grabs capture. Feedback precedence is drag
   then hover, both layered over the thumb style. Two panes register independently.
2. Supply matching units for content length, visible length, offset and on_seek.
   The valid range is `0..=content_length.saturating_sub(viewport_length)`.
   Tree viewport offset uses visible flattened item indices, not wrapped line
   counts; the framework handles whole multi-line item fitting. Dragging a tree
   scrollbar with `active: false` does not select or confirm a chapter.
3. Move manual screen-to-content offset code out of component handlers.
   PointerEvent.position/area are already drawing coordinates; use screen_position
   only for terminal-relative actions. Ordinary hits clip; capture can continue
   outside the viewport. Click and use_drag positions remain screen coordinates.
4. Loading or empty widgets can retain old hit tables. Reject those views and
   bounds-check indices against current data before selection or confirmation.
   Compute `let hit = state.read().hit_test(...)` before matching and mutating;
   a match scrutinee's guard survives all branches.
5. Register hooks unconditionally even if a callback such as Input.on_seek is
   absent. Initialize fallback state every frame before choosing an external
   state. Dynamic callback, active and loading props must not shift hook slots.
6. Create InputLayer each update. `PointerOptions.layer: Some(layer)` assigns
   a SearchInput/editing handler to its explicit layer; None follows Current.
   Changing exclusivity cancels captures and background hover. Handle Cancel
   separately from End/Up: it clears temporary state without committing an action.
7. Use `use_hover_row` for controlled item hover. It rechecks the latest mapping
   after drawing, so stationary pointers follow scroll/content changes. Public
   use_pointer sends geometry-driven changes, not unconditional per-frame Move.
   Keep hit mappers side-effect free and update hover state only on change.
8. ScrollView content/viewport/bar visibility resolve together. Main-axis
   Percentage/Ratio/Fill refer to the original block.inner; cross-axis constraints
   refer to the final viewport. The offset clamps before child drawing. Border
   mode needs real RIGHT and BOTTOM borders; padding/title space is not a border.

Before calling an integration complete, exercise independent panes, nested
viewports, scroll endpoints, drag outside/release, modal open/close mid-drag,
stationary layout movement/resize, multiline item heights, and loading/empty views.
Build with the application's actual features. For framework changes also run
all-feature workspace tests/Clippy/docs and the default-feature library tests.
Runnable references: `mouse`, `custom_scrollbar`, `tree_select`, `search_input`,
`textarea`; the existing runtime/component regression tests cover these contracts.
