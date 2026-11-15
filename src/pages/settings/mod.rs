//! 顶部标签、分组表单与直接操作控件。配置仍由原有 ConfigStore 保存。
mod model;

use crate::components::settings::Focus;
use crate::components::settings::{self, Edit, Editor};
use crate::{
    AppearanceConfig, BackgroundMode, ReaderDisplayConfig,
    app::navigation::UseAppNavigate,
    components::{ActionBar, ActionButton, KeyShortcutInfo, ShortcutInfoModal},
    hooks::use_reader_preferences,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use model::{Control, Key, Picker, SECTIONS};
use ratatui::{
    layout::{Constraint, Margin},
    text::Line,
    widgets::{Paragraph, Wrap},
};
use ratatui_kit::prelude::*;
use std::sync::Arc;

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
        if let Key::Reader(setting) = key {
            setting.choose(&mut config, index);
        }
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

    use_reader_preferences(&mut hooks, *reader.read());
    hooks.use_effect(
        move || {
            crate::app::ui_memory::save_cursor(
                crate::app::ui_memory::SETTINGS,
                crate::app::ui_memory::Cursor::new(current_section, cursor),
            );
        },
        (current_section, cursor),
    );

    let editor: Editor = settings::callback({
        let keymap = keymap.clone();
        let mut picker = picker;
        move |edit| {
            if let Edit::Focus(index) = edit {
                select_row(cursors, current_section, index);
                focus.set(Focus::Form);
                return;
            }
            let (index, delta) = match edit {
                Edit::Activate(i) => (i, None),
                Edit::Adjust(i, d) => (i, Some(d)),
                Edit::Focus(_) => return,
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
                        id.adjust(&mut reader.write(), delta);
                    }
                }
                Control::Toggle(value) => {
                    let next = delta.map(|d| d > 0).unwrap_or(!value);
                    if let Key::Reader(id) = row.key {
                        id.adjust(&mut reader.write(), if next { 1 } else { -1 });
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
                Control::Action if delta.is_none() => navigate.push("/theme-setting"),
                Control::Action => {}
                Control::ReadOnly => {}
            }
        }
    });
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
            if key.code == KeyCode::Char('i') {
                info_open.set(true);
                return EventResult::Consumed;
            }
            let tab = section.get();
            let state = settings::navigation::Navigation {
                tab,
                row: cursors.get()[tab],
                focus: focus.get(),
            };
            let Some(command) = settings::navigation::command(key, state, SECTIONS.len(), count)
            else {
                return EventResult::Ignored;
            };
            use settings::navigation::Command;
            match command {
                Command::Tab(value) => section.set(value),
                Command::Focus(next) => focus.set(next),
                Command::Row(value) => select_row(cursors, tab, value),
                Command::Edit(edit) => editor.call(edit),
                Command::Close => navigate.go(-1),
            }
            keyboard_revision.set(keyboard_revision.get().wrapping_add(1));
            EventResult::Consumed
        }
    });
    let picker_view = if let Some(current) = picker.read().clone() {
        let mut picker = picker;
        element!(settings::SettingsPicker(
            ..settings::PickerProps {
                colors: None,
                title: current.title,
                options: Arc::new(current.options),
                initial: current.selected,
                on_select: settings::callback(move |index| {
                    choose(current.key, index, reader, appearance, feedback);
                    picker.set(None);
                }),
                on_cancel: settings::callback(move |_| picker.set(None)),
            }
        ))
        .into_any()
    } else {
        element!(View(height:Constraint::Length(0))).into_any()
    };
    let hint = reader.read().show_hint_bar;
    let detail = if !feedback.read().is_empty() {
        feedback.read().clone()
    } else {
        rows.get(cursor)
            .map(|r| r.detail.clone())
            .unwrap_or_default()
    };
    let tabs_props = settings::TabsProps {
        labels: Arc::new(SECTIONS.iter().map(|s| (*s).to_owned()).collect()),
        selected: current_section,
        focused: focus.get() == Focus::Tabs,
        active: !overlay,
        on_select: settings::callback(move |index| {
            section.set(index);
            focus.set(Focus::Tabs);
        }),
    };
    let form_props = settings::FormProps {
        rows,
        selected: cursor,
        focused: focus.get() == Focus::Form,
        editor,
        active: !overlay,
        keyboard_revision: keyboard_revision.get(),
        identity: current_section,
    };
    element!(View(margin:Margin::new(3,1)) {
        ActionBar(menu:"全局设置") {
            ActionButton(label:"帮助",shortcut:"I",on_press:move |_|info_open.set(true))
            ActionButton(label:"返回",shortcut:"Esc",on_press:move |_|navigate.go(-1))
        }
        View(width:Constraint::Length(width.saturating_sub(6).min(88))) {
            settings::SettingsTabs(..tabs_props)
            View(height:Constraint::Length(1)){}
            settings::SettingsForm<model::Key>(..form_props)
            View(height:Constraint::Length(2)){widget(Paragraph::new(detail).style(theme.muted).wrap(Wrap{trim:false}))}
            View(height:Constraint::Length(u16::from(hint))){widget(Line::from("Tab 切换焦点 · ↑↓ 选择 · ←→ 调整 · 空格/Enter 操作 · Esc 返回").style(theme.muted))}
        }
        {picker_view}
        ShortcutInfoModal(
            key_shortcut_info:KeyShortcutInfo::new(vec![("标签栏 / 表单焦点","Tab / Shift+Tab"),("切换标签","标签栏 ←/→ 或 Ctrl+PageUp/PageDown"),("选择设置","↑/↓，Home/End，PageUp/PageDown"),("调整数值","←/→ 或点击 [-]/[+]"),("开关 / 选择 / 打开","Space / Enter 或单击控件"),("取消选择 / 返回","Esc")]),
            open:info_open.get(),on_close:move |_|info_open.set(false),
        )
    })
}
