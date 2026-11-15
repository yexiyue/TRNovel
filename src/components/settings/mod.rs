//! Common setting rows, tabs, scrollable forms and exclusive choice pickers.
pub mod view;
use crate::theme::AppChromeTheme;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Margin, Position, Rect};
use ratatui_kit::prelude::*;
use std::sync::{Arc, Mutex};
use view::{Focus, Hit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Toggle(bool),
    Number { decrease: bool, increase: bool },
    Choice,
    Action,
    ReadOnly,
}
#[derive(Clone)]
pub struct Row<K> {
    pub group: &'static str,
    pub key: K,
    pub label: String,
    pub value: String,
    pub detail: String,
    pub control: Control,
}
impl<K> Row<K> {
    pub fn map_key<L>(self, f: impl FnOnce(K) -> L) -> Row<L> {
        Row {
            key: f(self.key),
            group: self.group,
            label: self.label,
            value: self.value,
            detail: self.detail,
            control: self.control,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Edit {
    Focus(usize),
    Activate(usize),
    Adjust(usize, i32),
}
pub type Callback<T> = Arc<Mutex<Handler<'static, T>>>;
pub type Editor = Callback<Edit>;
pub fn callback<T: 'static>(f: impl FnMut(T) + Send + Sync + 'static) -> Callback<T> {
    Arc::new(Mutex::new(Handler::from(f)))
}
pub fn use_callback<T: 'static>(
    hooks: &mut Hooks,
    incoming: &mut Handler<'static, T>,
) -> Callback<T> {
    let shared = hooks
        .use_state(|| Arc::new(Mutex::new(Handler::default())))
        .read()
        .clone();
    if !incoming.is_default() {
        *shared.lock().unwrap_or_else(|e| e.into_inner()) = incoming.take();
    }
    shared
}
pub fn use_area(hooks: &mut Hooks) -> view::FrameArea {
    hooks
        .use_state(|| Arc::new(Mutex::new(Rect::default())))
        .read()
        .clone()
}

#[derive(Props)]
pub struct TabsProps {
    pub labels: Arc<Vec<String>>,
    pub selected: usize,
    pub focused: bool,
    pub active: bool,
    pub on_select: Callback<usize>,
}
#[component]
pub fn SettingsTabs(props: &TabsProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let active = props.active;
    let count = props.labels.len();
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        move |pos| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            if active {
                view::tab_at(area, count, pos)
            } else {
                None
            }
        }
    });
    hooks.use_pointer(PointerOptions::default(), {
        let on_select = props.on_select.clone();
        move |event| {
            if active
                && matches!(
                    event.kind,
                    PointerKind::Click {
                        button: MouseButton::Left,
                        ..
                    }
                )
                && let Some(index) = view::tab_at(event.area, count, event.position)
            {
                (on_select.lock().unwrap_or_else(|e| e.into_inner()))(index);
                return EventResult::Consumed;
            }
            EventResult::Ignored
        }
    });
    element!(View(height:Constraint::Length(2)){widget(view::TabsWidget{area:geometry,labels:props.labels.clone(),section:props.selected,focus:if props.focused{Focus::Tabs}else{Focus::Form},hover:if active{hover.get()}else{None},theme})})
}

#[derive(Props)]
pub struct FormProps<K: Clone + Send + Sync + Unpin + 'static> {
    pub rows: Arc<Vec<Row<K>>>,
    pub selected: usize,
    pub focused: bool,
    pub active: bool,
    pub identity: usize,
    pub keyboard_revision: u64,
    pub editor: Editor,
}
#[component]
pub fn SettingsForm<K>(props: &FormProps<K>, mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    K: Clone + Send + Sync + Unpin + 'static,
{
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
    let list = Arc::new(view::entries(&props.rows));
    let rows = props.rows.clone();
    let active = props.active;
    let mut offset = hooks.use_state(|| 0usize);
    let cursor = props.selected.min(rows.len().saturating_sub(1));
    hooks.use_effect(
        {
            let list = list.clone();
            move || {
                if area.height > 0 {
                    let next = view::reveal(&list, cursor, offset.get(), usize::from(area.height));
                    if next != offset.get() {
                        offset.set(next);
                    }
                }
            }
        },
        (
            props.identity,
            cursor,
            area.height,
            props.focused,
            props.keyboard_revision,
            list.len(),
        ),
    );
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        let list = list.clone();
        let rows = rows.clone();
        move |pos| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            if active {
                view::hit(area, &list, offset.get(), &rows, pos)
            } else {
                None
            }
        }
    });
    hooks.use_scrollbar(
        ScrollbarOptions {
            content_length: list.len(),
            viewport_length: usize::from(area.height),
            position: offset.get(),
            scrollbar: crate::theme::scrollbar_style(theme.border, theme.title),
            placement: TrackPlacement::RightInward(0),
        },
        move |line| offset.set(line),
    );
    hooks.use_pointer(PointerOptions::default(), {
        let list = list.clone();
        let rows = rows.clone();
        let editor = props.editor.clone();
        move |event| {
            if !active {
                return EventResult::Ignored;
            }
            match event.kind {
                PointerKind::Click {
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(hit) =
                        view::hit(event.area, &list, offset.get(), &rows, event.position)
                    {
                        let mut handler = editor.lock().unwrap_or_else(|e| e.into_inner());
                        handler(Edit::Focus(hit.row()));
                        match hit {
                            Hit::Decrease(i) => handler(Edit::Adjust(i, -1)),
                            Hit::Increase(i) => handler(Edit::Adjust(i, 1)),
                            Hit::Activate(i) => handler(Edit::Activate(i)),
                            Hit::Row(_) => {}
                        }
                        return EventResult::Consumed;
                    }
                }
                PointerKind::ScrollDown => {
                    offset.set(
                        (offset.get() + 3)
                            .min(list.len().saturating_sub(usize::from(event.area.height))),
                    );
                    return EventResult::Consumed;
                }
                PointerKind::ScrollUp => {
                    offset.set(offset.get().saturating_sub(3));
                    return EventResult::Consumed;
                }
                _ => {}
            }
            EventResult::Ignored
        }
    });
    element!(View(height:Constraint::Fill(1)){widget(view::FormWidget{area:geometry,rows,list,cursor,focus:if props.focused{Focus::Form}else{Focus::Tabs},offset:offset.get(),hover:if active{hover.get()}else{None},theme})})
}

#[derive(Props)]
pub struct ChoiceListProps {
    pub colors: Option<Arc<Vec<ratatui::style::Color>>>,
    pub title: String,
    pub options: Arc<Vec<String>>,
    pub initial: usize,
    pub on_select: Callback<usize>,
    pub on_cancel: Callback<()>,
    pub dismiss_outside: bool,
}
#[component]
pub fn SettingsChoiceList(
    props: &ChoiceListProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
    let count = props.options.len();
    let last = count.saturating_sub(1);
    let initial = props.initial.min(last);
    let mut selected = hooks.use_state(move || initial);
    let mut offset = hooks.use_state(|| 0usize);
    hooks.use_effect(
        move || {
            selected.set(initial);
            offset.set(0);
        },
        (props.options.clone(), initial),
    );
    hooks.use_effect(
        move || {
            let height = usize::from(area.height.saturating_sub(2)).max(1);
            let value = selected.get();
            let current = offset.get();
            let next = if value < current {
                value
            } else if value >= current + height {
                value + 1 - height
            } else {
                current
            };
            offset.set(next.min(count.saturating_sub(height)));
        },
        (selected.get(), count, area.height),
    );
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        move |pos| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            choice_at(area, pos, count, offset.get())
        }
    });
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let commit = props.on_select.clone();
        let cancel = props.on_cancel.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => selected.set((selected.get() + 1).min(last)),
                KeyCode::Up | KeyCode::Char('k') => selected.set(selected.get().saturating_sub(1)),
                KeyCode::Home => selected.set(0),
                KeyCode::End => selected.set(last),
                KeyCode::PageDown => selected.set((selected.get() + 5).min(last)),
                KeyCode::PageUp => selected.set(selected.get().saturating_sub(5)),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if count > 0 {
                        (commit.lock().unwrap_or_else(|e| e.into_inner()))(selected.get());
                    }
                }
                KeyCode::Esc => (cancel.lock().unwrap_or_else(|e| e.into_inner()))(()),
                _ => return EventResult::Ignored,
            }
            EventResult::Consumed
        }
    });
    hooks.use_scrollbar(
        ScrollbarOptions {
            content_length: count,
            viewport_length: usize::from(area.height.saturating_sub(2)),
            position: offset.get(),
            scrollbar: crate::theme::scrollbar_style(theme.border, theme.title),
            ..Default::default()
        },
        move |value| offset.set(value),
    );
    hooks.use_pointer(PointerOptions::default(), {
        let commit = props.on_select.clone();
        move |event| {
            match event.kind {
                PointerKind::Click {
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(index) = choice_at(event.area, event.position, count, offset.get())
                    {
                        (commit.lock().unwrap_or_else(|e| e.into_inner()))(index);
                        return EventResult::Consumed;
                    }
                }
                PointerKind::ScrollDown => {
                    selected.set((selected.get() + 1).min(last));
                    return EventResult::Consumed;
                }
                PointerKind::ScrollUp => {
                    selected.set(selected.get().saturating_sub(1));
                    return EventResult::Consumed;
                }
                _ => {}
            }
            EventResult::Ignored
        }
    });
    hooks.use_event_handler_with_options(
        EventScope::Current,
        EventPriority::Low,
        EventOptions { hit_test: false },
        {
            let geometry = geometry.clone();
            let cancel = props.on_cancel.clone();
            let dismiss = props.dismiss_outside;
            move |event| {
                if dismiss
                    && let Event::Mouse(mouse) = event
                    && mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && !geometry
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .contains(Position::new(mouse.column, mouse.row))
                {
                    (cancel.lock().unwrap_or_else(|e| e.into_inner()))(());
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
        },
    );
    element!(View{widget(view::PickerWidget{colors:props.colors.clone(),area:geometry,title:props.title.clone(),options:props.options.as_ref().clone(),selected:selected.get(),offset:offset.get(),hover:hover.get(),theme})})
}
pub fn choice_at(area: Rect, pos: Position, count: usize, offset: usize) -> Option<usize> {
    let inner = area.inner(Margin::new(1, 1));
    if !inner.contains(pos) {
        return None;
    }
    let index = offset + usize::from(pos.y - inner.y);
    (index < count).then_some(index)
}

#[derive(Props)]
pub struct PickerProps {
    pub colors: Option<Arc<Vec<ratatui::style::Color>>>,
    pub title: String,
    pub options: Arc<Vec<String>>,
    pub initial: usize,
    pub on_select: Callback<usize>,
    pub on_cancel: Callback<()>,
}
#[component]
pub fn SettingsPicker(props: &PickerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let layer = hooks.use_input_layer(true, true);
    let (_, height) = hooks.use_terminal_size();
    let rows = (props.options.len() as u16 + 2)
        .min(height.saturating_sub(2))
        .max(3);
    element!(Modal(open:true,layer:Some(layer),width:Constraint::Length(40),height:Constraint::Length(rows)){
        SettingsChoiceList(..ChoiceListProps{colors:props.colors.clone(),title:props.title.clone(),options:props.options.clone(),initial:props.initial,on_select:props.on_select.clone(),on_cancel:props.on_cancel.clone(),dismiss_outside:true})
    })
}

#[derive(Props)]
pub struct ButtonProps {
    pub label: String,
    pub on_press: Callback<()>,
}
#[component]
pub fn SettingsButton(props: &ButtonProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let hovered = hooks.use_hover();
    let on_press = props.on_press.clone();
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if matches!(
            event.kind,
            PointerKind::Click {
                button: MouseButton::Left,
                ..
            }
        ) {
            (on_press.lock().unwrap_or_else(|e| e.into_inner()))(());
            return EventResult::Consumed;
        }
        EventResult::Ignored
    });
    let text = format!("[{}]", props.label);
    let width = ratatui::text::Line::from(text.as_str()).width() as u16;
    element!(View(width:Constraint::Length(width),height:Constraint::Length(1)){widget(ratatui::text::Line::styled(text,if hovered{theme.selected}else{theme.highlight}))})
}
