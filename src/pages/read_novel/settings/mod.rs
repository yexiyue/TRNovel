//! Reading preferences share the same form controls and metadata with global settings.
pub(crate) mod rows;
use crate::{
    Frameline, ReaderDisplayConfig, TocRatio,
    components::settings::{self, Control, Edit, view::Focus},
    pages::read_novel::visible_lines,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Margin},
    text::Line,
};
use ratatui_kit::prelude::*;
use std::sync::Arc;
pub(crate) const TOC_ROW: usize = 10 + cfg!(feature = "tts") as usize;
pub(crate) const GLOBAL_HINT_ROW: usize = TOC_ROW + 1;
pub(crate) const BLANK_ROW: usize = TOC_ROW + 2;

pub(crate) fn choices(config: &ReaderDisplayConfig, id: usize) -> Option<(Vec<String>, usize)> {
    match id {
        6 => Some((
            Frameline::ALL
                .iter()
                .map(|v| v.label().to_owned())
                .collect(),
            Frameline::ALL
                .iter()
                .position(|v| *v == config.frameline)
                .unwrap_or(0),
        )),
        TOC_ROW => Some((
            TocRatio::ALL.iter().map(|v| v.label().to_owned()).collect(),
            TocRatio::ALL
                .iter()
                .position(|v| *v == config.toc_ratio)
                .unwrap_or(0),
        )),
        _ => None,
    }
}
pub(crate) fn choose_config(config: &mut ReaderDisplayConfig, id: usize, index: usize) {
    match id {
        6 => {
            if let Some(v) = Frameline::ALL.get(index) {
                config.frameline = *v;
            }
        }
        TOC_ROW => {
            if let Some(v) = TocRatio::ALL.get(index) {
                config.toc_ratio = *v;
            }
        }
        _ => {}
    }
}

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
    let mut picker = hooks.use_state(|| None::<usize>);
    let close = settings::use_callback(&mut hooks, &mut props.on_close);
    let mut groups = vec!["排版", "翻页", "界面"];
    if cfg!(feature = "tts") {
        groups.push("听书");
    }
    let groups = Arc::new(groups);
    let current_tab = tab.get().min(groups.len() - 1);
    let items = Arc::new(
        rows::rows(&config)
            .into_iter()
            .filter(|r| r.group == groups[current_tab])
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
        let groups = groups.clone();
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
            let fresh = rows::rows(&config);
            let Some(row) = fresh
                .iter()
                .filter(|r| r.group == groups[current_tab])
                .nth(index)
            else {
                return;
            };
            match row.control {
                Control::Toggle(value) => adjust(
                    display,
                    row.key,
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
                        adjust(display, row.key, d);
                    }
                }
                Control::Choice => {
                    if let Some(d) = delta {
                        if let Some((options, index)) = choices(&config, row.key) {
                            let index = (index as i32 + d)
                                .clamp(0, options.len().saturating_sub(1) as i32)
                                as usize;
                            let mut next = config;
                            choose_config(&mut next, row.key, index);
                            display.set(next);
                        }
                    } else {
                        picker.set(Some(row.key));
                    }
                }
                _ => {}
            }
        }
    });
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let editor = editor.clone();
        let groups = groups.clone();
        let close = close.clone();
        let count = items.len();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if !active || key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            let t = tab.get();
            let i = indices.get()[t].min(count.saturating_sub(1));
            let mut next = i;
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => focus.set(if focus.get() == Focus::Tabs {
                    Focus::Form
                } else {
                    Focus::Tabs
                }),
                KeyCode::Left | KeyCode::Char('h') if focus.get() == Focus::Tabs => {
                    tab.set((t + groups.len() - 1) % groups.len())
                }
                KeyCode::Right | KeyCode::Char('l') if focus.get() == Focus::Tabs => {
                    tab.set((t + 1) % groups.len())
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if focus.get() == Focus::Tabs {
                        focus.set(Focus::Form);
                    } else {
                        next = (i + 1).min(count.saturating_sub(1));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') if focus.get() == Focus::Form => {
                    next = i.saturating_sub(1)
                }
                KeyCode::Home if focus.get() == Focus::Form => next = 0,
                KeyCode::End if focus.get() == Focus::Form => next = count.saturating_sub(1),
                KeyCode::PageDown if focus.get() == Focus::Form => {
                    next = (i + 5).min(count.saturating_sub(1))
                }
                KeyCode::PageUp if focus.get() == Focus::Form => next = i.saturating_sub(5),
                KeyCode::Left | KeyCode::Char('h') => {
                    (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Adjust(i, -1))
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Adjust(i, 1))
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if focus.get() == Focus::Tabs {
                        focus.set(Focus::Form);
                    } else {
                        (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Activate(i));
                    }
                }
                KeyCode::Esc => (close.lock().unwrap_or_else(|e| e.into_inner()))(()),
                _ => return EventResult::Ignored,
            }
            let mut values = indices.get();
            values[t] = next;
            indices.set(values);
            revision.set(revision.get().wrapping_add(1));
            EventResult::Consumed
        }
    });
    if !open {
        return element!(View(height:Constraint::Length(0))).into_any();
    }
    let picker_view = if let Some(id) = picker.get()
        && let Some((options, initial)) = choices(&config, id)
    {
        let title = describe_setting(&config, id, 0)
            .0
            .trim_end_matches(':')
            .to_owned();
        element!(settings::SettingsPicker(
            ..settings::PickerProps {
                colors: None,
                title,
                options: Arc::new(options),
                initial,
                on_select: settings::callback(move |index| {
                    let mut config = *display.read();
                    choose_config(&mut config, id, index);
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
            if r.key == 0 {
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
    element!(Modal(open:true,layer:Some(layer),width:Constraint::Length(width.saturating_sub(4).min(92)),height:Constraint::Percentage(85)){
        Border(border_style:theme.border){
            View(margin:Margin::new(1,1)){
                View(height:Constraint::Length(1),flex_direction:Direction::Horizontal){View(width:Constraint::Fill(1)){widget(Line::from("阅读设置").style(theme.title))}settings::SettingsButton(..settings::ButtonProps{label:"关闭".into(),on_press:close})}
                settings::SettingsTabs(..settings::TabsProps{labels:Arc::new(groups.iter().map(|s|(*s).to_owned()).collect()),selected:current_tab,focused:focus.get()==Focus::Tabs,active,on_select:settings::callback(move|index|{tab.set(index);focus.set(Focus::Tabs);})})
                settings::SettingsForm<usize>(..settings::FormProps{rows:items,selected,focused:focus.get()==Focus::Form,active,identity:current_tab,keyboard_revision:revision.get(),editor})
                View(height:Constraint::Length(2)){Text(text:detail,style:theme.muted,wrap:true)}
                View(height:Constraint::Length(u16::from(config.show_hint_bar))){widget(Line::from("Tab 切换焦点 · ↑↓ 选择 · ←→ 调整 · 空格/Enter 操作 · Esc 关闭").style(theme.muted))}
            }
        }
        {picker_view}
    }).into_any()
}
/// 条目序号 → (标签, 当前值文案)。新增条目只改这里和 `adjust`,不必再动渲染树。
pub(crate) fn describe_setting(
    config: &crate::ReaderDisplayConfig,
    row: usize,
    page_step: usize,
) -> (String, String) {
    let on_off = |value: bool| if value { "开" } else { "关" }.to_string();
    match row {
        // 用户无法凭「2」想象效果,但看到「每页滚动 28 行」就懂了;终端极矮把
        // 步长压到 1 时也能一眼看见。
        0 => (
            "翻页重叠:".to_string(),
            format!("{} 行(每页滚动 {} 行)", config.page_overlap, page_step),
        ),
        1 => ("显示标题:".to_string(), on_off(config.show_title)),
        2 => (
            "段间空行:".to_string(),
            if config.paragraph_gap == 0 {
                "0 行(段落相接)".to_string()
            } else {
                format!("{} 行", config.paragraph_gap)
            },
        ),
        3 => (
            "首行缩进:".to_string(),
            if config.first_line_indent == 0 {
                "0 字(不缩进)".to_string()
            } else {
                format!("{} 字", config.first_line_indent)
            },
        ),
        4 => (
            "每行字数:".to_string(),
            if config.lines_per_row == 0 {
                "跟随窗口".to_string()
            } else {
                // 一个汉字两列:写清列数,用户不必自己换算「30 字为什么这么窄」。
                format!(
                    "{} 字({} 列)",
                    config.lines_per_row,
                    config.lines_per_row * 2
                )
            },
        ),
        5 => (
            "左右留白:".to_string(),
            if config.h_margin == 0 {
                "0 列(贴边)".to_string()
            } else {
                format!("{} 列", config.h_margin)
            },
        ),
        6 => ("边框:".to_string(), config.frameline.label().to_string()),
        7 => ("边框标题:".to_string(), on_off(config.show_frameline_title)),
        8 => ("框底提示:".to_string(), on_off(config.show_hint_bar)),
        9 => ("↑/↓ 翻页:".to_string(), on_off(config.arrows_page)),
        GLOBAL_HINT_ROW => ("页底提示:".to_string(), on_off(config.show_global_hint)),
        BLANK_ROW => ("翻页留白:".to_string(), on_off(config.allow_page_blank)),
        #[cfg(feature = "tts")]
        10 => ("跟随朗读:".to_string(), on_off(config.follow_tts)),
        TOC_ROW => (
            "目录占宽:".to_string(),
            config.toc_ratio.label().to_string(),
        ),
        _ => (String::new(), String::new()),
    }
}

/// 加减一个单位。方向用 i32 而非 bool,是因为多数条目真的有多档(0..=4 行空行),
/// 只有开关型才把它折成真假。
pub(crate) fn adjust(display: AtomState<crate::ReaderDisplayConfig>, row: usize, delta: i32) {
    adjust_config(&mut display.write(), row, delta);
}

pub(crate) fn adjust_config(config: &mut crate::ReaderDisplayConfig, row: usize, delta: i32) {
    match row {
        0 => {
            if delta > 0 {
                config.increase_page_overlap();
            } else {
                config.decrease_page_overlap();
            }
        }
        1 => config.show_title = delta > 0,
        2 => {
            if delta > 0 {
                config.increase_paragraph_gap();
            } else {
                config.decrease_paragraph_gap();
            }
        }
        3 => {
            if delta > 0 {
                config.increase_first_line_indent();
            } else {
                config.decrease_first_line_indent();
            }
        }
        4 => {
            if delta > 0 {
                config.increase_lines_per_row();
            } else {
                config.decrease_lines_per_row();
            }
        }
        5 => {
            if delta > 0 {
                config.increase_h_margin();
            } else {
                config.decrease_h_margin();
            }
        }
        6 => {
            config.frameline = if delta > 0 {
                config.frameline.next()
            } else {
                config.frameline.previous()
            };
        }
        7 => config.show_frameline_title = delta > 0,
        8 => config.show_hint_bar = delta > 0,
        9 => config.arrows_page = delta > 0,
        GLOBAL_HINT_ROW => config.show_global_hint = delta > 0,
        BLANK_ROW => config.allow_page_blank = delta > 0,
        #[cfg(feature = "tts")]
        10 => config.follow_tts = delta > 0,
        TOC_ROW => config.toc_ratio = config.toc_ratio.shift(if delta > 0 { 1 } else { -1 }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_rows_cover_every_item_exactly_once() {
        let mut rows = rows::rows(&crate::ReaderDisplayConfig::default())
            .iter()
            .map(|r| r.key)
            .collect::<Vec<_>>();
        rows.sort_unstable();
        assert_eq!(rows, (0..=BLANK_ROW).collect::<Vec<_>>());
    }

    #[test]
    fn every_setting_has_a_label_and_value() {
        let config = crate::ReaderDisplayConfig::default();
        for index in 0..=BLANK_ROW {
            let (label, value) = describe_setting(&config, index, 28);
            assert!(!label.is_empty(), "条目 {index} 没有标签");
            assert!(!value.is_empty(), "条目 {index} 没有值文案");
        }
    }
}
