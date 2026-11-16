//! Reader settings modal. Preference definitions and edits live in the pure model.
pub(crate) mod model;
use crate::{
    components::settings::{self, Control, Edit, Focus},
    pages::read_novel::visible_lines,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyEventKind};
use model::{Group, Setting, rows};
use ratatui::{
    layout::{Constraint, Direction, Margin},
    text::Line,
};
use ratatui_kit::prelude::*;
use std::sync::Arc;

#[derive(Props, Default)]
pub struct ReaderSettingsModalProps {
    pub open: bool,
    pub layer: Option<InputLayer>,
    pub on_close: Handler<'static, ()>,
    pub viewport_lines: Option<usize>,
    pub is_editing: bool,
}
#[component]
pub fn ReaderSettingsModal(
    props: &mut ReaderSettingsModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let (width, height) = hooks.use_terminal_size();
    let mut display = hooks.use_atom(&crate::state::READER_DISPLAY);
    let config = *display.read();
    let mut tab = hooks.use_state(|| 0usize);
    let mut indices = hooks.use_state(|| [0usize; 4]);
    let mut focus = hooks.use_state(|| Focus::Tabs);
    let mut revision = hooks.use_state(|| 0u64);
    let mut picker = hooks.use_state(|| None::<Setting>);
    let close = settings::use_callback(&mut hooks, &mut props.on_close);
    let groups = Group::ALL;
    let current_tab = tab.get().min(groups.len() - 1);
    let items = Arc::new(
        rows(&config)
            .into_iter()
            .filter(|r| r.key.group() == groups[current_tab])
            .collect::<Vec<_>>(),
    );
    let selected = indices.get()[current_tab].min(items.len().saturating_sub(1));
    let open = props.open;
    let active = open && props.is_editing && picker.get().is_none();
    let own_layer = hooks.use_input_layer(open && props.layer.is_none(), true);
    let layer = props.layer.unwrap_or(own_layer);
    hooks.use_effect(
        move || {
            if open {
                focus.set(Focus::Tabs);
                revision.set(revision.get().wrapping_add(1));
            }
        },
        open,
    );
    let editor = settings::callback({
        move |edit| {
            let (index, delta) = match edit {
                Edit::Focus(i) => {
                    let mut values = indices.get();
                    values[current_tab] = i;
                    indices.set(values);
                    focus.set(Focus::Form);
                    return;
                }
                Edit::Activate(i) => (i, None),
                Edit::Adjust(i, d) => (i, Some(d)),
            };
            let config = *display.read();
            let fresh = rows(&config);
            let Some(row) = fresh
                .iter()
                .filter(|r| r.key.group() == groups[current_tab])
                .nth(index)
            else {
                return;
            };
            match row.control {
                Control::Toggle(value) => row.key.adjust(
                    &mut display.write(),
                    if delta.map(|d| d > 0).unwrap_or(!value) {
                        1
                    } else {
                        -1
                    },
                ),
                Control::Number { decrease, increase } => {
                    if let Some(d) = delta
                        && ((d < 0 && decrease) || (d > 0 && increase))
                    {
                        row.key.adjust(&mut display.write(), d);
                    }
                }
                Control::Choice => {
                    if let Some(d) = delta {
                        if let Some((options, index)) = row.key.choices(&config) {
                            let index = (index as i32 + d)
                                .clamp(0, options.len().saturating_sub(1) as i32)
                                as usize;
                            let mut next = config;
                            row.key.choose(&mut next, index);
                            display.set(next);
                        }
                    } else {
                        picker.set(Some(row.key));
                    }
                }
                Control::Action if delta.is_none() => row.key.activate(&mut display.write()),
                _ => {}
            }
        }
    });
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let editor = editor.clone();
        let close = close.clone();
        let count = items.len();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if !active || key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            let current_tab = tab.get();
            let state = settings::navigation::Navigation {
                tab: current_tab,
                row: indices.get()[current_tab],
                focus: focus.get(),
            };
            let Some(command) = settings::navigation::command(key, state, groups.len(), count)
            else {
                return EventResult::Ignored;
            };
            use settings::navigation::Command;
            match command {
                Command::Tab(index) => tab.set(index),
                Command::Focus(next) => focus.set(next),
                Command::Row(index) => {
                    let mut cursors = indices.get();
                    cursors[current_tab] = index;
                    indices.set(cursors);
                }
                Command::Edit(edit) => editor.call(edit),
                Command::Close => close.call(()),
            }
            revision.set(revision.get().wrapping_add(1));
            EventResult::Consumed
        }
    });
    if !open {
        return element!(View(height:Constraint::Length(0))).into_any();
    }
    let picker_view = if let Some(id) = picker.get()
        && let Some((options, initial)) = id.choices(&config)
    {
        let title = id.label().to_owned();
        element!(settings::SettingsPicker(
            ..settings::PickerProps {
                colors: None,
                title,
                options: Arc::new(options),
                initial,
                on_select: settings::callback(move |index| {
                    let mut config = *display.read();
                    id.choose(&mut config, index);
                    display.set(config);
                    picker.set(None);
                }),
                on_cancel: settings::callback(move |_| picker.set(None))
            }
        ))
        .into_any()
    } else {
        element!(View(height:Constraint::Length(0))).into_any()
    };
    let visible = props
        .viewport_lines
        .unwrap_or_else(|| visible_lines(height, &config, config.inline_title()));
    let detail = items
        .get(selected)
        .map(|r| {
            if r.key == Setting::PageOverlap {
                format!(
                    "{} · 当前每页滚动 {} 行",
                    r.detail,
                    config.page_step(visible)
                )
            } else {
                r.detail.clone()
            }
        })
        .unwrap_or_default();
    let tabs_props = settings::TabsProps {
        labels: Arc::new(groups.iter().map(|s| s.label().to_owned()).collect()),
        selected: current_tab,
        focused: focus.get() == Focus::Tabs,
        active,
        on_select: settings::callback(move |index| {
            tab.set(index);
            focus.set(Focus::Tabs);
        }),
    };
    let form_props = settings::FormProps {
        rows: items,
        selected,
        focused: focus.get() == Focus::Form,
        active,
        identity: current_tab,
        keyboard_revision: revision.get(),
        editor,
    };
    element!(Modal(open:true,layer:Some(layer),width:Constraint::Length(width.saturating_sub(4).min(92)),height:Constraint::Percentage(85)){
        Border(border_style:theme.border){
            View(margin:Margin::new(1,1)){
                View(height:Constraint::Length(1),flex_direction:Direction::Horizontal){View(width:Constraint::Fill(1)){widget(Line::from("阅读设置").style(theme.title))}settings::SettingsButton(..settings::ButtonProps{label:"关闭".into(),on_press:close})}
                settings::SettingsTabs(..tabs_props)
                settings::SettingsForm<Setting>(..form_props)
                View(height:Constraint::Length(2)){Text(text:detail,style:theme.muted,wrap:true)}
                View(height:Constraint::Length(u16::from(config.show_hint_bar))){widget(Line::from("Tab 切换焦点 · ↑↓ 选择 · ←→ 调整 · 空格/Enter 操作 · Esc 关闭").style(theme.muted))}
            }
        }
        {picker_view}
    }).into_any()
}
