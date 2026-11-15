//! 顶部标签、分组表单与直接操作控件。配置仍由原有 ConfigStore 保存。
mod model;
mod view;

use crate::{
    AppearanceConfig, BackgroundMode, ReaderDisplayConfig,
    app::navigation::UseAppNavigate,
    components::{ActionBar, ActionButton, KeyShortcutInfo, ShortcutInfoModal},
    hooks::{DebounceOptions, UseDebounceEffect},
    pages::read_novel::settings::adjust,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use model::{Control, Key, Picker, SECTIONS};
use ratatui::{
    layout::{Constraint, Margin},
    text::Line,
    widgets::{Paragraph, Wrap},
};
use ratatui_kit::prelude::*;
use std::sync::{Arc, Mutex};
use view::{Focus, Hit};

fn use_area(hooks: &mut Hooks) -> view::FrameArea {
    hooks
        .use_state(|| Arc::new(Mutex::new(ratatui::layout::Rect::default())))
        .read()
        .clone()
}

type Editor = Arc<Mutex<Handler<'static, Edit>>>;
#[derive(Clone, Copy)]
enum Edit {
    Activate(usize),
    Adjust(usize, i32),
}

fn select_row(mut cursors: State<[usize; 5]>, section: usize, index: usize) {
    let mut current = cursors.get();
    current[section] = index;
    cursors.set(current);
}
fn save_appearance(
    mut appearance: AtomState<AppearanceConfig>,
    next: AppearanceConfig,
    mut feedback: State<String>,
) {
    match next.save() {
        Ok(()) => {
            appearance.set(next);
            feedback.set(String::new());
        }
        Err(error) => feedback.set(format!("保存失败：{error}")),
    }
}
fn choose(
    key: Key,
    index: usize,
    mut reader: AtomState<ReaderDisplayConfig>,
    appearance: AtomState<AppearanceConfig>,
    feedback: State<String>,
) {
    if key == Key::Background {
        let mut next = appearance.read().clone();
        next.background = if index == 0 {
            BackgroundMode::Theme
        } else {
            BackgroundMode::Terminal
        };
        save_appearance(appearance, next, feedback);
    } else {
        let mut config = *reader.read();
        model::choose_reader(&mut config, key, index);
        reader.set(config);
    }
}

#[component]
pub fn Settings(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let (width, _) = hooks.use_terminal_size();
    let appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let reader = hooks.use_atom(&crate::state::READER_DISPLAY);
    let keymap = hooks.use_atom(&crate::state::KEYMAP).read().clone();
    let restored = crate::app::ui_memory::restored_cursor(crate::app::ui_memory::SETTINGS);
    let initial_section = restored
        .map(|c| c.index.min(SECTIONS.len() - 1))
        .unwrap_or(0);
    let mut section = hooks.use_state(move || initial_section);
    let cursors = hooks.use_state(move || {
        let mut a = [0; 5];
        a[initial_section] = restored.map(|c| c.offset).unwrap_or(0);
        a
    });
    let mut focus = hooks.use_state(|| Focus::Tabs);
    let mut keyboard_revision = hooks.use_state(|| 0u64);
    let mut info_open = hooks.use_state(|| false);
    let picker = hooks.use_state(|| None::<Picker>);
    let feedback = hooks.use_state(String::new);
    let mut navigate = hooks.use_app_navigate();
    let current_section = section.get();
    let rows = Arc::new(model::rows(
        current_section,
        &reader.read(),
        &appearance.read(),
        &keymap,
    ));
    let cursor = cursors.get()[current_section].min(rows.len().saturating_sub(1));
    hooks.use_effect(
        move || select_row(cursors, current_section, cursor),
        (current_section, cursor),
    );

    let saved = hooks.use_state(|| crate::state::READER_DISPLAY.get());
    hooks.use_debounce_effect(
        move || {
            let config = *reader.read();
            if config != saved.get() {
                let mut saved = saved;
                tokio::task::spawn_blocking(move || {
                    if config.save().is_ok() {
                        saved.set(config);
                    }
                });
            }
        },
        *reader.read(),
        DebounceOptions::default(),
    );
    hooks.use_on_drop(move || {
        let config = *reader.read();
        if config != saved.get() {
            let _ = config.save();
        }
    });
    hooks.use_effect(
        move || {
            crate::app::ui_memory::save_cursor(
                crate::app::ui_memory::SETTINGS,
                crate::app::ui_memory::Cursor::new(current_section, cursor),
            );
        },
        (current_section, cursor),
    );

    let editor: Editor = Arc::new(Mutex::new(Handler::from({
        let keymap = keymap.clone();
        let mut picker = picker;
        move |edit| {
            let (index, delta) = match edit {
                Edit::Activate(i) => (i, None),
                Edit::Adjust(i, d) => (i, Some(d)),
            };
            // Read current atoms for rapid repeated clicks; all read guards end before mutation.
            let fresh = model::rows(current_section, &reader.read(), &appearance.read(), &keymap);
            let Some(row) = fresh.get(index) else {
                return;
            };
            match row.control {
                Control::Number { decrease, increase } => {
                    if let Some(delta) = delta
                        && ((delta < 0 && decrease) || (delta > 0 && increase))
                        && let Key::Reader(id) = row.key
                    {
                        adjust(reader, id, delta);
                    }
                }
                Control::Toggle(value) => {
                    let next = delta.map(|d| d > 0).unwrap_or(!value);
                    if let Key::Reader(id) = row.key {
                        adjust(reader, id, if next { 1 } else { -1 });
                    } else {
                        let mut config = appearance.read().clone();
                        match row.key {
                            Key::Header => config.show_header = next,
                            Key::ConfirmExit => config.confirm_exit = next,
                            _ => {}
                        }
                        save_appearance(appearance, config, feedback);
                    }
                }
                Control::Choice => {
                    let choice = model::picker(row.key, &reader.read(), &appearance.read());
                    if let Some(mut choice) = choice {
                        if let Some(delta) = delta {
                            choice.selected = (choice.selected as i32 + delta)
                                .clamp(0, choice.options.len().saturating_sub(1) as i32)
                                as usize;
                            choose(row.key, choice.selected, reader, appearance, feedback);
                        } else {
                            picker.set(Some(choice));
                        }
                    }
                }
                Control::Action => navigate.push("/theme-setting"),
                Control::ReadOnly => {}
            }
        }
    })));
    let overlay = info_open.get() || picker.read().is_some();
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let editor = editor.clone();
        let count = rows.len();
        move |event| {
            if overlay {
                return EventResult::Ignored;
            }
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            let tab = section.get();
            let item = cursors.get()[tab].min(count.saturating_sub(1));
            let switch = match key.code {
                KeyCode::Left | KeyCode::Char('h') if focus.get() == Focus::Tabs => Some(-1),
                KeyCode::Right | KeyCode::Char('l') if focus.get() == Focus::Tabs => Some(1),
                KeyCode::PageUp if key.modifiers.contains(KeyModifiers::CONTROL) => Some(-1),
                KeyCode::PageDown if key.modifiers.contains(KeyModifiers::CONTROL) => Some(1),
                _ => None,
            };
            if let Some(delta) = switch {
                section.set((tab as i32 + delta).rem_euclid(SECTIONS.len() as i32) as usize);
                return EventResult::Consumed;
            }
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => focus.set(if focus.get() == Focus::Tabs {
                    Focus::Form
                } else {
                    Focus::Tabs
                }),
                KeyCode::Down | KeyCode::Char('j') => {
                    if focus.get() == Focus::Tabs {
                        focus.set(Focus::Form);
                    } else {
                        select_row(cursors, tab, (item + 1).min(count.saturating_sub(1)));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') if focus.get() == Focus::Form => {
                    select_row(cursors, tab, item.saturating_sub(1))
                }
                KeyCode::Home => {
                    if focus.get() == Focus::Tabs {
                        section.set(0);
                    } else {
                        select_row(cursors, tab, 0);
                    }
                }
                KeyCode::End => {
                    if focus.get() == Focus::Tabs {
                        section.set(SECTIONS.len() - 1);
                    } else {
                        select_row(cursors, tab, count.saturating_sub(1));
                    }
                }
                KeyCode::PageDown if focus.get() == Focus::Form => {
                    select_row(cursors, tab, (item + 5).min(count.saturating_sub(1)))
                }
                KeyCode::PageUp if focus.get() == Focus::Form => {
                    select_row(cursors, tab, item.saturating_sub(5))
                }
                KeyCode::Left | KeyCode::Char('h') if focus.get() == Focus::Form => {
                    (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Adjust(item, -1))
                }
                KeyCode::Right | KeyCode::Char('l') if focus.get() == Focus::Form => {
                    (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Adjust(item, 1))
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if focus.get() == Focus::Tabs {
                        focus.set(Focus::Form);
                    } else {
                        (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Activate(item));
                    }
                }
                KeyCode::Esc => navigate.go(-1),
                KeyCode::Char('i') => info_open.set(true),
                _ => return EventResult::Ignored,
            }
            if focus.get() == Focus::Form {
                keyboard_revision.set(keyboard_revision.get().wrapping_add(1));
            }
            EventResult::Consumed
        }
    });
    let picker_layer = hooks.use_input_layer(picker.read().is_some(), true);
    let picker_height = picker
        .read()
        .as_ref()
        .map(|p| p.options.len() as u16 + 2)
        .unwrap_or(2);
    let hint = reader.read().show_hint_bar;
    let detail = if !feedback.read().is_empty() {
        feedback.read().clone()
    } else {
        rows.get(cursor)
            .map(|r| r.detail.clone())
            .unwrap_or_default()
    };
    element!(View(margin:Margin::new(3,1)) {
        ActionBar(menu:"全局设置") {
            ActionButton(label:"帮助",shortcut:"I",on_press:move |_|info_open.set(true))
            ActionButton(label:"返回",shortcut:"Esc",on_press:move |_|navigate.go(-1))
        }
        View(width:Constraint::Length(width.saturating_sub(6).min(88))) {
            View(height:Constraint::Length(1)){widget(Line::from("设置").style(theme.title))}
            View(height:Constraint::Length(1)){}
            SettingsTabs(..SettingsTabsProps {section,focus,active:!overlay})
            View(height:Constraint::Length(1)){}
            SettingsBody(..SettingsBodyProps {rows,section:current_section,cursors,focus,editor,active:!overlay,keyboard_revision:keyboard_revision.get()})
            View(height:Constraint::Length(2)){widget(Paragraph::new(detail).style(theme.muted).wrap(Wrap{trim:false}))}
            View(height:Constraint::Length(u16::from(hint))){widget(Line::from("Tab 切换焦点 · ↑↓ 选择 · ←→ 调整 · 空格/Enter 操作 · Esc 返回").style(theme.muted))}
        }
        Modal(open:picker.read().is_some(),layer:Some(picker_layer),width:Constraint::Length(32),height:Constraint::Length(picker_height)) {
            ChoicePanel(..ChoicePanelProps {picker,feedback})
        }
        ShortcutInfoModal(
            key_shortcut_info:KeyShortcutInfo::new(vec![("标签栏 / 表单焦点","Tab / Shift+Tab"),("切换标签","标签栏 ←/→ 或 Ctrl+PageUp/PageDown"),("选择设置","↑/↓，Home/End，PageUp/PageDown"),("调整数值","←/→ 或点击 [-]/[+]"),("开关 / 选择 / 打开","Space / Enter 或单击控件"),("取消选择 / 返回","Esc")]),
            open:info_open.get(),on_close:move |_|info_open.set(false),
        )
    })
}

#[derive(Props)]
struct SettingsTabsProps {
    section: State<usize>,
    focus: State<Focus>,
    active: bool,
}
#[component]
fn SettingsTabs(props: &SettingsTabsProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let active = props.active;
    let mut section = props.section;
    let mut focus = props.focus;
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        move |position| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            if active {
                view::tab_at(area, position)
            } else {
                None
            }
        }
    });
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if active
            && matches!(
                event.kind,
                PointerKind::Click {
                    button: MouseButton::Left,
                    ..
                }
            )
            && let Some(tab) = view::tab_at(event.area, event.position)
        {
            section.set(tab);
            focus.set(Focus::Tabs);
            return EventResult::Consumed;
        }
        EventResult::Ignored
    });
    element!(View(height:Constraint::Length(2)){widget(view::TabsWidget{area:geometry,section:section.get(),focus:focus.get(),hover:if active{hover.get()}else{None},theme})})
}

#[derive(Props)]
struct SettingsBodyProps {
    rows: Arc<Vec<model::Row>>,
    section: usize,
    cursors: State<[usize; 5]>,
    focus: State<Focus>,
    editor: Editor,
    active: bool,
    keyboard_revision: u64,
}
#[component]
fn SettingsBody(props: &SettingsBodyProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
    let list = Arc::new(view::entries(&props.rows));
    let rows = props.rows.clone();
    let mut offset = hooks.use_state(|| 0usize);
    let mut focus = props.focus;
    let cursors = props.cursors;
    let section = props.section;
    let active = props.active;
    let cursor = cursors.get()[section].min(rows.len().saturating_sub(1));
    hooks.use_effect(
        {
            let list = list.clone();
            move || {
                if area.height == 0 {
                    return;
                }
                let next = view::reveal(&list, cursor, offset.get(), usize::from(area.height));
                if next != offset.get() {
                    offset.set(next);
                }
            }
        },
        (
            section,
            cursor,
            area.height,
            focus.get(),
            props.keyboard_revision,
        ),
    );
    let hover = hooks.use_hover_row({
        let list = list.clone();
        let rows = rows.clone();
        let geometry = geometry.clone();
        move |position| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            if active {
                view::hit(area, &list, offset.get(), &rows, position)
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
                        select_row(cursors, section, hit.row());
                        focus.set(Focus::Form);
                        let edit = match hit {
                            Hit::Decrease(i) => Some(Edit::Adjust(i, -1)),
                            Hit::Increase(i) => Some(Edit::Adjust(i, 1)),
                            Hit::Activate(i) => Some(Edit::Activate(i)),
                            Hit::Row(_) => None,
                        };
                        if let Some(edit) = edit {
                            (editor.lock().unwrap_or_else(|e| e.into_inner()))(edit);
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
    element!(View(height:Constraint::Fill(1)){widget(view::FormWidget{area:geometry,rows,list,cursor,focus:focus.get(),offset:offset.get(),hover:if active{hover.get()}else{None},theme})})
}

#[derive(Props)]
struct ChoicePanelProps {
    picker: State<Option<Picker>>,
    feedback: State<String>,
}
#[component]
fn ChoicePanel(props: &ChoicePanelProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let mut picker = props.picker;
    let feedback = props.feedback;
    let reader = hooks.use_atom(&crate::state::READER_DISPLAY);
    let appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let current = picker.read().clone().unwrap_or_default();
    let count = current.options.len();
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        move |pos| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            picker_row(area, pos, count)
        }
    });
    let mut commit = move |index| {
        let current = picker.read().clone();
        if let Some(current) = current
            && index < current.options.len()
            && let Some(key) = current.key
        {
            choose(key, index, reader, appearance, feedback);
            picker.set(None);
        }
    };
    hooks.use_event_handler(EventScope::Current, EventPriority::High, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        let selected = picker.read().as_ref().map(|p| p.selected).unwrap_or(0);
        let next = match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(selected.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => Some((selected + 1).min(count.saturating_sub(1))),
            KeyCode::Home => Some(0),
            KeyCode::End => Some(count.saturating_sub(1)),
            KeyCode::Enter | KeyCode::Char(' ') => {
                commit(selected);
                None
            }
            KeyCode::Esc => {
                picker.set(None);
                None
            }
            _ => return EventResult::Ignored,
        };
        if let Some(next) = next
            && let Some(current) = picker.write().as_mut()
        {
            current.selected = next;
        }
        EventResult::Consumed
    });
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if matches!(
            event.kind,
            PointerKind::Click {
                button: MouseButton::Left,
                ..
            }
        ) && let Some(index) = picker_row(event.area, event.position, count)
        {
            commit(index);
            return EventResult::Consumed;
        }
        EventResult::Ignored
    });
    hooks.use_event_handler_with_options(
        EventScope::Current,
        EventPriority::Low,
        EventOptions { hit_test: false },
        {
            let geometry = geometry.clone();
            move |event| {
                if let Event::Mouse(mouse) = event
                    && mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && !geometry
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
                {
                    picker.set(None);
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
        },
    );
    element!(View {widget(view::PickerWidget{area:geometry,title:current.title,options:current.options,selected:current.selected,hover:hover.get(),theme})})
}
fn picker_row(
    area: ratatui::layout::Rect,
    pos: ratatui::layout::Position,
    count: usize,
) -> Option<usize> {
    let inner = area.inner(Margin::new(1, 1));
    if !inner.contains(pos) {
        return None;
    }
    let row = usize::from(pos.y - inner.y);
    (row < count).then_some(row)
}
