use crate::{
    AppearanceConfig, BackgroundMode,
    components::{ConfirmModal, KeyShortcutInfo, ShortcutInfoModal, list_select::ListSelect},
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Widget, WidgetRef},
};
use ratatui_kit::prelude::*;
use ratatui_kit_themes::{IntoKitPalette, ThemeName};
use tui_widget_list::{ListBuildContext, ListState};

#[derive(Debug, Clone, Copy)]
struct ThemeListItem {
    name: ThemeName,
    selected: bool,
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

        let palette = self.name.into_kit_palette();
        let marker = if self.current { "当前" } else { "    " };
        let title = Line::from(vec![
            Span::from(marker).style(self.theme.meta_label.patch(style)),
            Span::from("  "),
            Span::from(self.name.display_name()).style(style),
            Span::from("  "),
            Span::from(self.name.slug()).style(self.theme.meta_label.patch(style)),
        ]);

        let swatches = Line::from(vec![
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
    let themes = ThemeName::all();
    let current_theme = appearance.read().theme_name();
    let selected_index = themes
        .iter()
        .position(|name| *name == current_theme)
        .unwrap_or(0);
    let list_state = hooks.use_state(move || {
        let mut state = ListState::default();
        state.selected = Some(selected_index);
        state
    });

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
                let mut next = appearance.read().clone();
                next.background = match next.background {
                    BackgroundMode::Theme => BackgroundMode::Terminal,
                    BackgroundMode::Terminal => BackgroundMode::Theme,
                };
                let _ = next.save();
                appearance.set(next);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(Fragment{
        ListSelect<ThemeName>(
            items: themes.to_vec(),
            state: list_state,
            top_title: Line::from("主题设置").style(theme.title).centered(),
            bottom_title: Line::from(format!(
                "当前: {} · 背景: {} · T 切换终端背景",
                current_theme.display_name(),
                background_label(appearance.read().background),
            ))
            .style(theme.meta_label)
            .centered(),
            render_item:move|ctx:&ListBuildContext|{
                let name = themes[ctx.index];
                (ThemeListItem {
                    name,
                    selected: ctx.is_selected,
                    current: name == current_theme,
                    theme,
                }.into(),4)
            },
            is_editing: !info_modal_open.get() && !reset_modal_open.get(),
            empty_message: "暂无可用主题",
            on_select: move |name:ThemeName| {
                let mut next = appearance.read().clone();
                next.theme_slug = name.slug().to_string();
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
                ("重置外观", "D"),
            ]),
            open: info_modal_open.get(),
        )
        ConfirmModal(
            title: "重置外观",
            content: "是否重置为默认主题与背景模式？",
            open: reset_modal_open.get(),
            on_confirm: move |_| {
                let next = AppearanceConfig::default();
                let _ = next.save();
                appearance.set(next.clone());
                if let Some(index) = ThemeName::all()
                    .iter()
                    .position(|name| *name == next.theme_name())
                {
                    list_state.write().selected = Some(index);
                }
                reset_modal_open.set(false);
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
