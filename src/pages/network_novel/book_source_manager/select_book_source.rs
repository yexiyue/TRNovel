use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::BookSource;
use ratatui::{
    layout::{Constraint, Layout},
    style::Stylize,
    text::Line,
    widgets::{Block, Padding, Widget, WidgetRef},
};
use ratatui_kit::{
    AnyElement, Hooks, Props, State, UseContext, UseEvents, UseState, component, element,
    prelude::View,
};
use tui_widget_list::{ListBuildContext, ListState};

use crate::{
    ThemeConfig,
    book_source::BookSourceCache,
    components::{ConfirmModal, list_select::ListSelect},
    hooks::UseThemeConfig,
    utils::time_to_string,
};

pub struct BookSourceListItem {
    pub book_source: BookSource,
    pub selected: bool,
    pub theme: ThemeConfig,
}

impl WidgetRef for BookSourceListItem {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let item = &self.book_source;

        let block = if self.selected {
            Block::bordered()
                .padding(Padding::horizontal(2))
                .style(self.theme.selected)
        } else {
            Block::bordered().padding(Padding::horizontal(2))
        };

        let text_style = if self.selected {
            self.theme.basic.text.patch(self.theme.selected)
        } else {
            self.theme.basic.text
        };

        let inner_area = block.inner(area);
        let [top, bottom] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner_area);
        block.render(area, buf);

        let [bottom_left, bottom_right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom);

        Line::from(item.book_source_name.clone())
            .style(text_style)
            .centered()
            .render(top, buf);

        Line::from(format!("网址: {}", item.book_source_url))
            .style(self.theme.basic.text.patch(text_style))
            .left_aligned()
            .render(bottom_left, buf);

        Line::from(format!(
            "最后更新: {}",
            time_to_string(item.last_update_time).unwrap_or_default()
        ))
        .style(self.theme.basic.border_info.patch(text_style))
        .right_aligned()
        .render(bottom_right, buf);
    }
}

#[derive(Default, Props)]
pub struct SelectBookSourceProps {
    pub is_editing: bool,
}

#[component]
pub fn SelectBookSource(
    props: &SelectBookSourceProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let book_source_cache = hooks
        .use_context::<State<Option<BookSourceCache>>>()
        .clone();
    let theme = hooks.use_theme_config();
    let state = hooks.use_state(ListState::default);
    let mut delete_modal_open = hooks.use_state(|| false);

    let book_sources = book_source_cache
        .read()
        .as_ref()
        .map(|c| c.book_sources.clone())
        .unwrap_or_default();

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
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

    element!(View {
        ListSelect<BookSource>(
            items: book_sources.clone(),
            state: state,
            on_select: |_| {},
            is_editing: !delete_modal_open.get() && props.is_editing,
            empty_message: "暂无书源，请先导入书源",
            top_title: Line::from("选择书源 (回车确认)").style(
                if props.is_editing {
                    theme.highlight.not_dim()
                } else {
                    theme.basic.border_title
                }
            ).centered(),
            render_item:{
                let theme=theme.clone();
                move |context: &ListBuildContext| {
                    let item = &book_sources[context.index];
                    (BookSourceListItem{
                        book_source: item.clone(),
                        selected: context.is_selected,
                        theme: theme.clone(),
                    }.into(), 5)
                }
            }
        )
        ConfirmModal(
            title: Line::from("警告").centered().style(theme.basic.border_title),
            content: "确认删除该书源吗？",
            open: delete_modal_open.get() && props.is_editing,
            on_confirm:move |_| {
                let selected= state.read().selected.clone();
                if let Some(index) = selected {
                    if let Some(book_source_cache) = book_source_cache.write().as_mut() {
                        if index < book_source_cache.book_sources.len() {
                            book_source_cache.book_sources.remove(index);
                            state.write().select(Some(index.saturating_sub(1)));
                        }
                    }
                }
                delete_modal_open.set(false);
            },
            on_cancel:move |_| {
                delete_modal_open.set(false);
            }
        )
    })
}
