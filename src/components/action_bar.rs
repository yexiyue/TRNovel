use crate::theme::AppChromeTheme;
use ratatui::layout::{Constraint, Direction};
use ratatui_kit::prelude::*;

#[derive(Clone, Default)]
struct HoverHint {
    owner: u64,
    text: String,
}
static NEXT_HINT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static HINT: Atom<HoverHint> = Atom::new(HoverHint::default);

#[derive(Default, Props)]
pub struct ActionButtonProps {
    pub label: String,
    pub shortcut: String,
    pub disabled: bool,
    pub layer: Option<InputLayer>,
    pub on_press: Handler<'static, ()>,
}

/// One-row action, with its shortcut shown in a temporary overlay on hover.
#[component]
pub fn ActionButton(
    props: &mut ActionButtonProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let menu_group = hooks
        .try_use_context::<super::action_menu::MenuGroup>()
        .map(|group| group.0.clone());
    let entries = hooks.use_atom(&super::action_menu::ENTRIES);
    let callback = hooks
        .use_state(|| std::sync::Arc::new(std::sync::Mutex::new(Handler::<'static, ()>::default())))
        .read()
        .clone();
    // Local hover renders may reuse props whose handler was already taken.
    if !props.on_press.is_default() {
        *callback.lock().unwrap_or_else(|e| e.into_inner()) = props.on_press.take();
    }
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut hint = hooks.use_atom(&HINT);
    let owner = hooks
        .use_state(|| NEXT_HINT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
        .get();
    let mut hovered = hooks.use_state(|| false);
    let disabled = props.disabled;
    let label = props.label.clone();
    let description = if props.shortcut.is_empty() {
        label.clone()
    } else {
        format!("{} · 快捷键 {}", label, props.shortcut)
    };
    let description = if disabled {
        format!("{description} · 当前不可用")
    } else {
        description
    };
    hooks.use_effect(
        move || {
            if hovered.get() {
                hint.set(HoverHint {
                    owner,
                    text: description.clone(),
                });
            } else if hint.read().owner == owner {
                hint.set(HoverHint::default());
            }
        },
        (
            hovered.get(),
            props.label.clone(),
            props.shortcut.clone(),
            disabled,
        ),
    );
    hooks.use_on_drop(move || {
        if hint.read().owner == owner {
            hint.set(HoverHint::default());
        }
    });
    let registration = (
        menu_group.clone(),
        props.label.clone(),
        props.shortcut.clone(),
        disabled,
    );
    hooks.use_effect(
        {
            let callback = callback.clone();
            let registration = registration.clone();
            move || {
                if let (Some(group), label, shortcut, disabled) = &registration {
                    entries.write().insert(
                        owner,
                        super::action_menu::MenuEntry {
                            group: group.clone(),
                            label: label.clone(),
                            shortcut: shortcut.clone(),
                            disabled: *disabled,
                            callback: callback.clone(),
                        },
                    );
                } else if entries.read().contains_key(&owner) {
                    entries.write().remove(&owner);
                }
            }
        },
        registration,
    );
    hooks.use_on_drop(move || {
        if entries.read().contains_key(&owner) {
            entries.write().remove(&owner);
        }
    });
    let in_menu = menu_group.is_some();
    hooks.use_pointer(
        PointerOptions {
            track_move: true,
            layer: props.layer,
            ..Default::default()
        },
        move |event| match event.kind {
            PointerKind::Enter => {
                hovered.set(true);
                EventResult::Consumed
            }
            PointerKind::Leave => {
                hovered.set(false);
                EventResult::Consumed
            }
            PointerKind::Down(crossterm::event::MouseButton::Left) if !disabled && !in_menu => {
                callback.lock().unwrap_or_else(|e| e.into_inner())(());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        },
    );
    let width = ratatui::text::Line::from(label.clone())
        .width()
        .saturating_add(2)
        .min(20) as u16;
    element!(View(width: Constraint::Length(if in_menu { 0 } else { width }), height: Constraint::Length(if in_menu { 0 } else { 1 })) {
        Text(text: format!(" {label} "), style: if disabled { theme.meta_label.dim() } else if hovered.get() { theme.highlight.reversed() } else { theme.title })
    })
}

#[derive(Default, Props)]
pub struct ActionBarProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub menu: String,
}

#[component]
pub fn ActionBar<'a>(
    props: &mut ActionBarProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let size = hooks.use_previous_size();
    let (terminal_width, _) = hooks.use_terminal_size();
    if !props.menu.is_empty() {
        let name = &props.menu;
        let children = std::mem::take(&mut props.children);
        return element!(View(height: Constraint::Length(0), width: Constraint::Length(0)) {
            ContextProvider(value: Context::owned(super::action_menu::MenuGroup(name.clone()))) { {children} }
        }).into_any();
    }
    let width = if size.width > 0 {
        size.width
    } else {
        terminal_width
    };
    let per_row = actions_per_row(width);
    let children = std::mem::take(&mut props.children);
    let rows = children.len().div_ceil(per_row);
    let mut groups: Vec<Vec<AnyElement<'a>>> = Vec::new();
    for (index, child) in children.into_iter().enumerate() {
        if index % per_row == 0 {
            groups.push(Vec::new());
        }
        if let Some(group) = groups.last_mut() {
            group.push(child);
        }
    }
    element!(View(height: Constraint::Length(rows as u16)) {
        for (index, group) in groups.into_iter().enumerate() {
            View(key: index, height: Constraint::Length(1), flex_direction: Direction::Horizontal) { {group} }
        }
    }).into_any()
}

fn actions_per_row(width: u16) -> usize {
    usize::from(width / 20).max(1)
}

#[component]
pub fn ActionHint(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let hint = hooks.use_atom(&HINT);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let text = hint.read().text.clone();
    let menu_open = super::action_menu::menu_is_open(&mut hooks);
    element!(Modal(open: !text.is_empty() && !menu_open, style: ratatui::style::Style::default().remove_modifier(ratatui::style::Modifier::DIM), blocks_lower: Some(false), placement: Placement::TopLeft, offset: ratatui::layout::Offset { x: 0, y: 1 }, width: Constraint::Length((ratatui::text::Line::from(text.clone()).width() + 2) as u16), height: Constraint::Length(1)) {
        Text(text: text, style: theme.meta_label)
    })
}

/// Share one semantic action handler between a keyboard binding and visible buttons.
pub fn action_callback<T, F>(
    handler: &std::sync::Arc<std::sync::Mutex<F>>,
    action: T,
) -> Handler<'static, ()>
where
    T: Copy + Send + Sync + 'static,
    F: FnMut(T) -> EventResult + Send + Sync + 'static,
{
    let handler = handler.clone();
    Handler::from(move |_| {
        handler.lock().unwrap_or_else(|e| e.into_inner())(action);
    })
}

/// Both header visibility controls persist through the same configuration path.
pub fn toggle_header(mut appearance: AtomState<crate::AppearanceConfig>) {
    let mut next = appearance.read().clone();
    next.show_header = !next.show_header;
    let _ = next.save();
    appearance.set(next);
}
