use crate::{
    AppearanceConfig, BackgroundMode,
    components::{ConfirmModal, KeyShortcutInfo, ShortcutInfoModal, list_select::ListSelect},
    theme::{AppChromeTheme, presets::ThemeChoice},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Widget, WidgetRef},
};
use ratatui_kit::prelude::*;
use tui_widget_list::ListState;

#[derive(Debug, Clone, Copy)]
struct ThemeListItem {
    choice: ThemeChoice,
    selected: bool,
    hovered: bool,
    current: bool,
    theme: AppChromeTheme,
}

impl Widget for ThemeListItem {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        self.render_ref(area, buf);
    }
}

impl WidgetRef for ThemeListItem {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let style = if self.selected {
            self.theme.selected
        } else if self.hovered {
            self.theme.text.add_modifier(Modifier::BOLD)
        } else if self.current {
            self.theme.highlight.add_modifier(Modifier::BOLD)
        } else {
            self.theme.text
        };

        let block = Block::bordered()
            .padding(Padding::horizontal(1))
            .border_style(if self.current {
                self.theme.highlight
            } else {
                self.theme.border
            })
            .style(if self.selected {
                self.theme.selected
            } else {
                Style::new()
            });

        let inner = block.inner(area);
        block.render(area, buf);

        let palette = self.choice.palette();
        let marker = if self.current { "当前" } else { "    " };
        // 中文名在前、slug 在后: 用户按名字挑, 排查配置时才看 slug。
        let title = Line::from(vec![
            Span::from(marker).style(self.theme.meta_label.patch(style)),
            Span::from("  "),
            Span::from(self.choice.display_name()).style(style),
            Span::from("  "),
            Span::from(self.choice.slug()).style(self.theme.meta_label.patch(style)),
        ]);

        // 直接铺一块底色, 让用户不选中也能看出背景/正文色的观感, 只比色块更接近实读效果。
        let preview_style = Style::new().fg(palette.fg).bg(palette.bg);
        let swatches = Line::from(vec![
            Span::from("正文").style(preview_style),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.accent)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.selection)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.success)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.warning)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.error)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.info)),
        ])
        .style(style);

        let [top, bottom] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);
        Paragraph::new(title).render(top, buf);
        Paragraph::new(swatches).render(bottom, buf);
    }
}

#[component]
pub fn ThemeSetting(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut info_modal_open = hooks.use_state(|| false);
    let mut reset_modal_open = hooks.use_state(|| false);
    let mut appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let choices = ThemeChoice::all();
    // 未知 slug(手改配置/未来版本回退)回落到应用当前实际生效的内置主题,
    // 保证「当前」标记永远指向屏幕上真正的那套配色。
    let current = {
        let config = appearance.read();
        ThemeChoice::from_slug(&config.theme_slug)
            .unwrap_or_else(|| ThemeChoice::Builtin(config.theme_name()))
    };
    let selected_index = choices
        .iter()
        .position(|choice| *choice == current)
        .unwrap_or(0);
    let list_state = hooks.use_state(move || {
        let mut state = ListState::default();
        state.selected = Some(selected_index);
        state
    });

    let mut toggle_background = move |_: ()| {
        let mut next = appearance.read().clone();
        next.background = match next.background {
            BackgroundMode::Theme => BackgroundMode::Terminal,
            BackgroundMode::Terminal => BackgroundMode::Theme,
        };
        let _ = next.save();
        appearance.set(next);
    };
    // 退出确认被关掉后,这里是唯一能重新打开它的入口。
    let mut toggle_confirm_exit = move |_: ()| {
        let mut next = appearance.read().clone();
        next.confirm_exit = !next.confirm_exit;
        let _ = next.save();
        appearance.set(next);
    };
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('i') | KeyCode::Char('I') => {
                info_modal_open.set(!info_modal_open.get());
                EventResult::Consumed
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                reset_modal_open.set(true);
                EventResult::Consumed
            }
            KeyCode::Char('t') | KeyCode::Char('T') if !reset_modal_open.get() => {
                toggle_background(());
                EventResult::Consumed
            }
            KeyCode::Char('c') | KeyCode::Char('C') if !reset_modal_open.get() => {
                toggle_confirm_exit(());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(View{
        crate::components::ActionBar(menu: "设置") {
            crate::components::ActionButton(label: if appearance.read().show_header { "顶部栏：显示" } else { "顶部栏：隐藏" }, on_press: move |_| {
                crate::components::action_bar::toggle_header(appearance);
            })
            crate::components::ActionButton(label: "切换背景模式", shortcut: "T", on_press: toggle_background)
            crate::components::ActionButton(label: if appearance.read().confirm_exit { "退出确认：开启" } else { "退出确认：关闭" }, shortcut: "C", on_press: toggle_confirm_exit)
            crate::components::ActionButton(label: "重置外观", shortcut: "D", on_press: move |_| reset_modal_open.set(true))
            crate::components::ActionButton(label: "帮助", shortcut: "I", on_press: move |_| info_modal_open.set(true))
        }
        ListSelect<ThemeChoice>(
            items: choices.clone(),
            state: list_state,
            top_title: Line::from("全局设置").style(theme.title).centered(),
            bottom_title: Line::from(format!(
                "当前: {} · 背景: {}",
                current.display_name(),
                background_label(appearance.read().background),
            ))
            .style(theme.meta_label)
            .centered(),
            render_item:{
                let choices = choices.clone();
                move |ctx:&VirtualItemContext|{
                    (ThemeListItem {
                        choice: choices[ctx.index],
                        selected: ctx.is_selected,
                        hovered: ctx.is_hovered,
                        current: choices[ctx.index] == current,
                        theme,
                    }.into(),4)
                }
            },
            is_editing: !info_modal_open.get() && !reset_modal_open.get(),
            empty_message: "暂无可用主题",
            on_select: move |choice:ThemeChoice| {
                let mut next = appearance.read().clone();
                next.theme_slug = choice.slug().to_string();
                let _ = next.save();
                appearance.set(next);
            },
        )
        ShortcutInfoModal(
            key_shortcut_info: KeyShortcutInfo::new(vec![
                ("选择下一个", "J / ▼"),
                ("选择上一个", "K / ▲"),
                ("应用主题", "Enter"),
                ("切换终端背景", "T"),
                ("退出前确认", "C"),
                ("重置外观", "D"),
            ]),
            open: info_modal_open.get(),
            on_close:{let mut open = info_modal_open; move |_: ()| open.set(false)},
        )
        ConfirmModal(
            title: "重置外观",
            content: "是否恢复默认主题、终端背景并显示顶部栏？",
            open: reset_modal_open.get(),
            on_confirm: {
                let choices = choices.clone();
                move |_| {
                    let next = AppearanceConfig::default();
                    let _ = next.save();
                    appearance.set(next.clone());
                    let target = ThemeChoice::from_slug(&next.theme_slug)
                        .unwrap_or_else(|| ThemeChoice::Builtin(next.theme_name()));
                    if let Some(index) = choices.iter().position(|choice| *choice == target) {
                        list_state.write().selected = Some(index);
                    }
                    reset_modal_open.set(false);
                }
            },
            on_cancel: move |_| {
                reset_modal_open.set(false);
            }
        )
    })
}

fn background_label(mode: BackgroundMode) -> &'static str {
    match mode {
        BackgroundMode::Theme => "主题背景",
        BackgroundMode::Terminal => "终端背景",
    }
}
