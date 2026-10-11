//! Page-owned searches share the single application header.
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction};
use ratatui_kit::prelude::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tui_input::backend::crossterm::EventHandler;

#[derive(Default, Props)]
pub struct SearchInputProps {
    pub value: String,
    pub placeholder: String,
    pub label: String,
    pub shortcut: String,
    pub validate: Handler<'static, String, (bool, String)>,
    pub on_submit: Handler<'static, String, bool>,
    pub clear_on_submit: bool,
    pub clear_on_escape: bool,
    pub is_editing: bool,
    pub on_clear: Handler<'static, ()>,
    pub activate_key: Option<KeyCode>,
    pub open: bool,
    pub on_close: Handler<'static, ()>,
}
#[derive(Clone, PartialEq)]
struct SearchMetadata {
    value: String,
    placeholder: String,
    label: String,
    enabled: bool,
    open: bool,
    shortcut: String,
}
#[derive(Clone)]
struct SearchEntry {
    value: String,
    placeholder: String,
    label: String,
    enabled: bool,
    shortcut: String,
    callbacks: Arc<Mutex<SearchInputProps>>,
}
static SEARCHES: Atom<BTreeMap<u64, SearchEntry>> = Atom::new(BTreeMap::new);
static ACTIVE: Atom<Option<u64>> = Atom::new(|| None);
static NEXT: AtomicU64 = AtomicU64::new(1);

#[component]
pub fn SearchInput(
    props: &mut SearchInputProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let id = hooks
        .use_state(|| NEXT.fetch_add(1, Ordering::Relaxed))
        .get();
    let entries = hooks.use_atom(&SEARCHES);
    let mut active = hooks.use_atom(&ACTIVE);
    let callbacks = hooks
        .use_state(|| Arc::new(Mutex::new(SearchInputProps::default())))
        .read()
        .clone();
    let key = props.activate_key.unwrap_or(KeyCode::Char('s'));
    let enabled = props.is_editing;
    let metadata = SearchMetadata {
        value: props.value.clone(),
        placeholder: props.placeholder.clone(),
        label: props.label.clone(),
        enabled,
        open: props.open,
        shortcut: props.shortcut.clone(),
    };
    {
        let mut current = callbacks.lock().unwrap_or_else(|e| e.into_inner());
        if !props.validate.is_default() {
            current.validate = props.validate.take();
        }
        if !props.on_submit.is_default() {
            current.on_submit = props.on_submit.take();
        }
        if !props.on_clear.is_default() {
            current.on_clear = props.on_clear.take();
        }
        if !props.on_close.is_default() {
            current.on_close = props.on_close.take();
        }
        current.clear_on_escape = props.clear_on_escape;
        current.clear_on_submit = props.clear_on_submit;
    }
    hooks.use_effect(
        {
            let callbacks = callbacks.clone();
            let metadata = metadata.clone();
            move || {
                entries.write().insert(
                    id,
                    SearchEntry {
                        value: metadata.value.clone(),
                        placeholder: metadata.placeholder.clone(),
                        label: metadata.label.clone(),
                        enabled: metadata.enabled,
                        shortcut: if !metadata.shortcut.is_empty() {
                            metadata.shortcut.clone()
                        } else {
                            match key {
                                KeyCode::Char(c) => c.to_uppercase().to_string(),
                                KeyCode::Null => String::new(),
                                _ => format!("{key:?}"),
                            }
                        },
                        callbacks: callbacks.clone(),
                    },
                );
                if metadata.open && metadata.enabled {
                    active.set(Some(id));
                }
                if !metadata.enabled && active.get() == Some(id) {
                    active.set(None);
                }
            }
        },
        metadata,
    );
    hooks.use_on_drop(move || {
        entries.write().remove(&id);
        if active.get() == Some(id) {
            active.set(None);
        }
    });
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        if enabled
            && matches!(event, Event::Key(k) if k.kind == KeyEventKind::Press && k.code == key && !k.modifiers.intersects(crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::ALT))
        {
            active.set(Some(id));
            EventResult::Consumed
        } else {
            EventResult::Ignored
        }
    });
    element!(View(height: Constraint::Length(0), width: Constraint::Length(0)))
}

fn finish(entry: &SearchEntry, draft: &str, submit: bool) -> Result<(), String> {
    let mut callbacks = entry.callbacks.lock().unwrap_or_else(|e| e.into_inner());
    if submit {
        if !callbacks.validate.is_default() {
            let (valid, message) = (callbacks.validate)(draft.to_owned());
            if !valid {
                return Err(message);
            }
        }
        if !callbacks.on_submit.is_default() && !(callbacks.on_submit)(draft.to_owned()) {
            return Err("无法提交，请检查输入".into());
        }
        if callbacks.clear_on_submit && !callbacks.on_clear.is_default() {
            (callbacks.on_clear)(());
        }
    } else if callbacks.clear_on_escape && !callbacks.on_clear.is_default() {
        (callbacks.on_clear)(());
    }
    if !callbacks.on_close.is_default() {
        (callbacks.on_close)(());
    }
    Ok(())
}

#[derive(Default, Props)]
pub struct HeaderEditorProps {
    pub input: Option<State<tui_input::Input>>,
    pub placeholder: String,
    pub layer: Option<InputLayer>,
}

#[component]
fn HeaderEditor(props: &mut HeaderEditorProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let input = props.input.expect("header editor requires a draft");
    let size = hooks.use_previous_size();
    hooks.use_pointer(
        PointerOptions {
            layer: props.layer,
            ..Default::default()
        },
        move |event| {
            if !matches!(
                event.kind,
                PointerKind::Down(crossterm::event::MouseButton::Left)
            ) {
                return EventResult::Ignored;
            }
            let draft = input.read();
            let column = draft.visual_scroll(size.width.saturating_sub(1) as usize)
                + usize::from(event.position.x.saturating_sub(size.x));
            let mut cells = 0;
            let cursor = draft
                .value()
                .chars()
                .take_while(|ch| {
                    cells += ratatui::text::Line::from(ch.to_string()).width();
                    cells <= column
                })
                .count();
            drop(draft);
            input
                .write()
                .handle(tui_input::InputRequest::SetCursor(cursor));
            EventResult::Consumed
        },
    );
    element!(Input(input: input.read().clone(), placeholder: props.placeholder.clone()))
}

#[derive(Default, Props)]
pub struct HeaderSearchProps {
    pub chrome_layer: Option<InputLayer>,
    pub back_shortcut: String,
    pub back_disabled: bool,
    pub on_back: Handler<'static, ()>,
}
#[component]
pub fn HeaderSearch(
    props: &mut HeaderSearchProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let back = hooks
        .use_state(|| Arc::new(Mutex::new(Handler::<'static, ()>::default())))
        .read()
        .clone();
    if !props.on_back.is_default() {
        *back.lock().unwrap_or_else(|e| e.into_inner()) = props.on_back.take();
    }
    let (terminal_width, _) = hooks.use_terminal_size();
    let show_header = hooks.use_atom(&crate::state::APPEARANCE).read().show_header;
    let mut previous_header = hooks.use_state(move || show_header);
    let mut informed = hooks.use_state(|| false);
    let mut recovery_hint = hooks.use_state(|| false);
    hooks.use_effect(
        move || {
            if previous_header.get() && !show_header && !informed.get() {
                informed.set(true);
                recovery_hint.set(true);
            }
            previous_header.set(show_header);
        },
        show_header,
    );
    let hint_visible = recovery_hint.get();
    hooks.use_async_effect(
        async move {
            if hint_visible {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                recovery_hint.set(false);
            }
        },
        hint_visible,
    );
    let mut menu_request = hooks.use_atom(&super::action_menu::HEADER_MENU_REQUEST);
    let menu_open = super::action_menu::menu_is_open(&mut hooks);
    hooks.use_event_handler(EventScope::Current, EventPriority::High, move |event| {
        if !show_header && matches!(event, Event::Key(key) if key.kind == KeyEventKind::Press && key.code == KeyCode::F(10)) {
            menu_request.set(Some(super::action_menu::MenuRequest::First));
            EventResult::Consumed
        } else { EventResult::Ignored }
    });
    let back_label = if terminal_width < 60 {
        "返回"
    } else {
        "‹ 返回"
    };
    let back_width = (ratatui::text::Line::from(back_label).width() + 2) as u16;
    let navigation_gap = if terminal_width < 60 { 1u16 } else { 2u16 };
    let entries = hooks.use_atom(&SEARCHES);
    let mut active = hooks.use_atom(&ACTIVE);
    let entry = active.get().and_then(|id| entries.read().get(&id).cloned());
    let mut draft = hooks.use_state(tui_input::Input::default);
    let mut error = hooks.use_state(String::new);
    hooks.use_effect(
        {
            let entry = entry.clone();
            move || {
                error.set(String::new());
                if let Some(entry) = entry {
                    draft.set(tui_input::Input::new(entry.value));
                }
            }
        },
        active.get(),
    );
    let layer = hooks.use_input_layer(entry.is_some(), true);
    let back_layer = entry.as_ref().map(|_| layer);
    let navigation_entry = entry.clone();
    let navigation_label = if entry.is_some() {
        "取消"
    } else {
        back_label
    };
    let navigation_shortcut = if entry.is_some() {
        "Esc".to_string()
    } else {
        props.back_shortcut.clone()
    };
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let entry = entry.clone();
        move |event| {
            let Some(entry) = &entry else {
                return EventResult::Ignored;
            };
            match event {
                Event::Key(k)
                    if k.kind == KeyEventKind::Press
                        && matches!(k.code, KeyCode::Enter | KeyCode::Esc) =>
                {
                    match finish(entry, draft.read().value(), k.code == KeyCode::Enter) {
                        Ok(()) => active.set(None),
                        Err(message) => error.set(message),
                    }
                    EventResult::Consumed
                }
                Event::Paste(text) => {
                    error.set(String::new());
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        draft
                            .write()
                            .handle(tui_input::InputRequest::InsertChar(ch));
                    }
                    EventResult::Consumed
                }
                Event::Key(_) => {
                    error.set(String::new());
                    draft.write().handle_event(&event);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });
    let searches = entries
        .read()
        .iter()
        .map(|(&id, entry)| {
            let label = if entry.label.is_empty() {
                "⌕ 搜索".to_string()
            } else {
                format!("⌕ {}", entry.label)
            };
            let width = (ratatui::text::Line::from(label.clone()).width() + 2).min(20) as u16;
            (id, label, width, entry.enabled, entry.shortcut.clone())
        })
        .collect::<Vec<_>>();
    let start = back_width + navigation_gap + searches.iter().map(|item| item.2).sum::<u16>();
    let searching = entry.is_some();
    let header = element!(View(flex_direction: Direction::Horizontal, height: Constraint::Length(1)) {
        super::ActionButton(label: navigation_label, shortcut: navigation_shortcut, disabled: entry.is_none() && props.back_disabled, layer: back_layer, on_press: move |_| {
            if let Some(entry) = &navigation_entry {
                if finish(entry, draft.read().value(), false).is_ok() { active.set(None); }
            } else { back.lock().unwrap_or_else(|e| e.into_inner())(()); }
        })
        View(width: Constraint::Length(navigation_gap))
        if let Some(entry) = entry {
            View(width: Constraint::Fill(1)) { HeaderEditor(input: Some(draft), placeholder: entry.placeholder.clone(), layer: layer) }
            if !error.read().is_empty() { Text(text: error.read().clone(), style: ratatui::style::Style::new().fg(ratatui::style::Color::Red)) }
            super::ActionButton(label: "确定", shortcut: "Enter", layer: layer, on_press: { let entry = entry.clone(); move |_| { match finish(&entry, draft.read().value(), true) { Ok(()) => active.set(None), Err(message) => error.set(message) } } })
        } else {
            for (id, label, _, enabled, shortcut) in searches {
                super::ActionButton(key: id, label: label, shortcut: shortcut, disabled: !enabled, on_press: move |_| active.set(Some(id)))
            }
            super::action_menu::AppMenus(start_x: start)
        }
    }).into_any();
    if show_header || searching || menu_open || menu_request.get().is_some() {
        element!(Modal(open: true, layer: props.chrome_layer, blocks_lower: Some(false), placement: Placement::TopLeft,
            width: Constraint::Percentage(100), height: Constraint::Length(1),
            style: ratatui::style::Style::default().remove_modifier(ratatui::style::Modifier::DIM)) { {header} }).into_any()
    } else {
        let corner_width = if hint_visible {
            terminal_width.min(43)
        } else {
            3
        };
        element!(Modal(open: true, layer: props.chrome_layer, blocks_lower: Some(false), placement: Placement::TopLeft,
            offset: ratatui::layout::Offset { x: terminal_width.saturating_sub(corner_width) as i32, y: 0 },
            width: Constraint::Length(corner_width), height: Constraint::Length(1),
            style: ratatui::style::Style::default().remove_modifier(ratatui::style::Modifier::DIM)) {
            View(flex_direction: Direction::Horizontal) {
            if hint_visible { View(width: Constraint::Fill(1)) { Text(text: "F10 或右上角 ⋯ 打开菜单") } }
            super::ActionButton(label: "⋯", shortcut: "F10 打开菜单", on_press: move |_| menu_request.set(Some(super::action_menu::MenuRequest::More)))
            }
        }).into_any()
    }
}
