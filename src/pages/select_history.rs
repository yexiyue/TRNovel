use crate::{
    History, HistoryItem, ThemeConfig,
    components::{ConfirmModal, KeyShortcutInfo, ShortcutInfoModal, list_select::ListSelect},
    hooks::UseThemeConfig,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::Stylize,
    text::{Line, Span, Text},
    widgets::{Block, Padding, Paragraph, Widget, WidgetRef},
};
use ratatui_kit::prelude::*;
use tui_widget_list::{ListBuildContext, ListState};

pub struct ListItem {
    pub history: HistoryItem,
    pub selected: bool,
    pub theme: ThemeConfig,
}

impl Widget for ListItem {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        self.render_ref(area, buf);
    }
}

impl WidgetRef for ListItem {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let block = if self.selected {
            Block::bordered()
                .padding(Padding::horizontal(0))
                .style(self.theme.selected)
        } else {
            Block::bordered().padding(Padding::horizontal(0))
        };

        let [top, bottom] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(block.inner(area));
        block.render(area, buf);

        let [bottom_left, bottom_right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom);

        let text_color = if self.selected {
            self.theme.basic.text.patch(self.theme.selected)
        } else {
            self.theme.basic.text
        };

        match &self.history {
            HistoryItem::Local(item) => {
                Paragraph::new(Text::from(vec![
                    Line::from(item.title.clone()),
                    Line::from(item.current_chapter.clone()).centered(),
                ]))
                .style(text_color)
                .render(top, buf);

                Span::from("本地小说")
                    .style(self.theme.basic.border_info.patch(text_color))
                    .render(bottom_left, buf);

                Text::from(format!(
                    "{:.2}% {}",
                    item.percent,
                    item.last_read_at.format("%Y-%m-%d %H:%M:%S")
                ))
                .style(self.theme.basic.border_info.patch(text_color))
                .right_aligned()
                .render(bottom_right, buf);
            }
            HistoryItem::Network(item) => {
                Paragraph::new(Text::from(vec![
                    Line::from(item.title.clone()),
                    Line::from(item.current_chapter.clone()).centered(),
                ]))
                .style(text_color)
                .render(top, buf);

                Span::from(format!("书源：{}", item.book_source))
                    .style(self.theme.basic.border_info.patch(text_color))
                    .render(bottom_left, buf);

                Text::from(format!(
                    "{:.2}% {}",
                    item.percent,
                    item.last_read_at.format("%Y-%m-%d %H:%M:%S")
                ))
                .style(self.theme.basic.border_info.patch(text_color))
                .right_aligned()
                .render(bottom_right, buf);
            }
        };
    }
}

#[component]
pub fn SelectHistory(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let history = hooks.use_context::<State<Option<History>>>();

    let mut delete_modal_open = hooks.use_state(|| false);
    let mut info_modal_open = hooks.use_state(|| false);

    let state = hooks.use_state(ListState::default);

    let history = hooks.use_memo(
        || {
            let new_history = History::load().ok();
            *history.write() = new_history;
            *history
        },
        (),
    );

    let histories = history
        .read()
        .clone()
        .map(|h| h.histories.clone())
        .unwrap_or_default();

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(!info_modal_open.get());
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    if state.read().selected.is_some() && !delete_modal_open.get() {
                        delete_modal_open.set(true);
                    } else {
                        delete_modal_open.set(false);
                    }
                }
                _ => {}
            }
        }
    });

    element!(Fragment{
        ListSelect<(String,HistoryItem)>(
            state: state,
            is_editing: !delete_modal_open.get() && !info_modal_open.get(),
            items: histories.clone(),
            top_title: Line::from("历史记录").centered().style(theme.basic.border_title),
            bottom_title: Line::from(
                format!(
                    "{}/{} 条",
                    state.read().selected.unwrap_or(0)+1,
                    histories.len())
                )
                .style(theme.basic.border_info.not_dim()),
            render_item: {
                let theme=theme.clone();
                move |context:&ListBuildContext| {
                    let (_, item) = &histories[context.index];
                    (
                        ListItem {
                            history: item.clone(),
                            selected: context.is_selected,
                            theme: theme.clone(),
                        }.into(),
                        5,
                    )
                }
            },
            empty_message: "暂无历史记录",
        )
        ConfirmModal(
            title: Line::from("警告").centered().style(theme.basic.border_title),
            content: "确认删除该历史记录吗？",
            open: delete_modal_open.get(),
            on_confirm:move |_| {
                let selected= state.read().selected;
                if let Some(index) = selected
                    && let Some(histories) = history.write().as_mut()
                        && index < histories.histories.len() {
                            histories.histories.remove(index);
                            state.write().select(Some(index.saturating_sub(1)));
                        }
                delete_modal_open.set(false);
            },
            on_cancel:move |_| {
                delete_modal_open.set(false);
            }
        )
        ShortcutInfoModal(
            key_shortcut_info: {
                let data = if delete_modal_open.get() {
                    vec![
                        ("确认删除", "Y"),
                        ("取消删除", "N"),
                        ("切换确定/取消", "◄ / ►"),
                        ("确认选中", "Enter"),
                    ]
                } else {
                    vec![
                        ("选择下一个", "J / ▼"),
                        ("选择上一个", "K / ▲"),
                        ("取消选择", "H / ◄"),
                        ("确认选择", "L / ► / Enter"),
                        ("删除选中的历史记录", "D"),
                    ]
                };
                KeyShortcutInfo::new(data)
            },
            open: info_modal_open.get(),
        )
    })
}
