use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::BookSource;
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Padding, Widget, WidgetRef},
};
use ratatui_kit::prelude::*;
use tui_widget_list::{ListBuildContext, ListState};

use crate::{
    ThemeConfig,
    book_source::BookSourceCache,
    components::{ConfirmModal, list_select::ListSelect},
    hooks::UseThemeConfig,
};

pub struct BookSourceListItem {
    pub book_source: BookSource,
    pub selected: bool,
    /// 该书源是否已登录(per-source 状态存有有效 cookie/loginHeader)。
    pub logged_in: bool,
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

        // 书名行徽标:已登录的源显示「已登录」(成功色),仅支持登录但未登录的显示
        // 「可登录(L)」(信息色),让用户一眼看出登录状态、知道哪些可按 L 登录。
        let mut name_spans = vec![Span::styled(item.name.clone(), text_style)];
        if self.logged_in {
            name_spans.push(Span::styled(
                "  ● 已登录",
                text_style.patch(Style::new().fg(self.theme.colors.success_color)),
            ));
        } else if item.has_login() {
            name_spans.push(Span::styled(
                "  可登录(L)",
                self.theme.basic.border_info.patch(text_style),
            ));
        }
        Line::from(name_spans).centered().render(top, buf);

        Line::from(format!("网址: {}", item.url))
            .style(self.theme.basic.text.patch(text_style))
            .left_aligned()
            .render(bottom_left, buf);

        Line::from(format!("分组: {}", item.group))
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
    let book_source_cache = *hooks.use_context::<State<Option<BookSourceCache>>>();
    let mut navigate = hooks.use_navigate();
    let theme = hooks.use_theme_config();
    // 默认选中第 0 项:ListState::default() 的 selected 为 None,会导致刚进页面(未按 j/k 前)
    // 没有任何选中项,Enter/L/D 全部静默无反应。给个初始选中,既有可见光标也让快捷键即时可用。
    let state = hooks.use_state(|| {
        let mut s = ListState::default();
        s.selected = Some(0);
        s
    });
    let mut delete_modal_open = hooks.use_state(|| false);

    let book_sources = book_source_cache
        .read()
        .as_ref()
        .map(|c| c.book_sources.clone())
        .unwrap_or_default();

    // 各书源的登录状态(读 per-source 状态;TTL 过期视为未登录)。返回列表页即重算,
    // 故登录成功后再回来能看到「已登录」。
    let logged_in: Vec<bool> = book_sources
        .iter()
        .map(|s| crate::login::is_logged_in(&s.url))
        .collect();

    let book_sources_keys = book_sources.clone();
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if state.read().selected.is_some() && !delete_modal_open.get() {
                    delete_modal_open.set(true);
                } else {
                    delete_modal_open.set(false);
                }
                EventResult::Consumed
            }
            // 书源登录(loginUrl/loginUi 非空才有意义):未登录 → 进登录页;
            // 已登录 → 无需重复登录,直接进选书页(「已登录 → 直接进入下一路由」)。
            KeyCode::Char('l') | KeyCode::Char('L') => {
                if let Some(i) = state.read().selected
                    && let Some(src) = book_sources_keys.get(i).cloned()
                    && src.has_login()
                {
                    if crate::login::is_logged_in(&src.url) {
                        navigate.push_with_state("/select-books", src);
                    } else {
                        navigate.push_with_state("/book-source-login", src);
                    }
                }
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(View {
        ListSelect<BookSource>(
            items: book_sources.clone(),
            state: state,
            on_select:move |book_source: BookSource| {
                navigate.push_with_state(
                    "/select-books",
                    book_source,
                );
            },
            is_editing: !delete_modal_open.get() && props.is_editing,
            empty_message: "暂无书源，请先导入书源",
            top_title: Line::from("选择书源 (回车确认)").style(theme.basic.border_title).centered(),
            render_item:{
                let theme=theme.clone();
                let logged_in=logged_in.clone();
                move |context: &ListBuildContext| {
                    let item = &book_sources[context.index];
                    (BookSourceListItem{
                        book_source: item.clone(),
                        selected: context.is_selected,
                        logged_in: logged_in.get(context.index).copied().unwrap_or(false),
                        theme: theme.clone(),
                    }.into(), 5)
                }
            }
        )
        ConfirmModal(
            title: "警告",
            content: "确认删除该书源吗？",
            open: delete_modal_open.get() && props.is_editing,
            on_confirm:move |_| {
                let selected= state.read().selected;
                if let Some(index) = selected
                    && let Some(book_source_cache) = book_source_cache.write().as_mut()
                        && index < book_source_cache.book_sources.len() {
                            book_source_cache.book_sources.remove(index);
                            state.write().select(Some(index.saturating_sub(1)));
                        }
                delete_modal_open.set(false);
            },
            on_cancel:move |_| {
                delete_modal_open.set(false);
            }
        )
    })
}
