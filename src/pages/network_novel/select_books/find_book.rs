use crate::{
    components::{WarningModal, list_select::ListSelect, search_input::SearchInput},
    errors::Errors,
    hooks::UseInitState,
    pages::network_novel::book_detail::BookDetailState,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::{BookList, BookListItem, Engine};
use ratatui::{
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Widget, WidgetRef, Wrap},
};
use ratatui_kit::prelude::*;
use tui_widget_list::{ListBuildContext, ListState};

#[derive(Props)]
pub struct FindBooksProps {
    pub engine: State<Option<Engine>>,
    pub current_explore: Option<super::ExploreListItem>,
    pub is_editing: bool,
}

#[component]
pub fn FindBooks(props: &FindBooksProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut filter_text = hooks.use_state(String::default);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut page = hooks.use_state(|| 1);
    let page_size = hooks.use_state(|| 20);
    let list_state = hooks.use_state(ListState::default);
    let mut navigate = hooks.use_navigate();
    let is_editing = props.is_editing;

    // books 先于事件处理器定义,使翻页处理能读「是否到头」(has_more/total_pages 双信号)。
    let (books, loading, error) = hooks.use_effect_state(
        {
            // 该 block 每次渲染都求值,**只能捕获值、绝不能在此 spawn 取页**:
            // 在此 `tokio::spawn(explore)` 会绕过 use_async_effect 的 deps 控制,每帧开一次
            // 浏览器(render explore 时系统栏狂闪)。explore/search 一律在下面 async 体内 await,
            // 由 use_async_effect 按 deps 仅在依赖变化时触发一次。
            let engine = props.engine.read().clone();
            // 入口身份是「标题 + 变量」(不再是固定 URL):取页时把整个 entry 交给 explore,
            // 由 explore.page.request 用入口变量 + {{page}} 生成取页 URL。
            let entry = props.current_explore.as_ref().map(|e| e.0.clone());
            let page = page.get();
            let page_size = page_size.get();
            let filter_text = filter_text.read().clone();
            async move {
                let Some(engine) = engine else {
                    return Ok::<BookList, Errors>(BookList::default());
                };
                let res = if filter_text.is_empty() {
                    match entry {
                        Some(entry) => engine.explore(&entry, page, page_size).await?,
                        None => return Ok(BookList::default()),
                    }
                } else {
                    engine.search(&filter_text, page, page_size).await?
                };
                list_state.write().select(Some(0));
                Ok(res)
            }
        },
        (
            props.current_explore.clone(),
            filter_text.read().clone(),
            props.engine.read().is_some(),
            page.get(),
        ),
    );

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press || !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('h') | KeyCode::Left if page.get() > 1 => {
                page.set(page.get() - 1);
                EventResult::Consumed
            }
            KeyCode::Char('l') | KeyCode::Right => {
                // 到头停翻:has_more==Some(false)(list-has-more)或已达 total_pages
                //(render-dual-source)→ 不再 +page;两信号有其一即可,都无则不限制(现状)。
                let at_end = {
                    let bl = books.read();
                    bl.as_ref().and_then(|b| b.has_more) == Some(false)
                        || bl
                            .as_ref()
                            .and_then(|b| b.total_pages)
                            .is_some_and(|m| page.get() >= m)
                };
                if !at_end {
                    page.set(page.get() + 1);
                }
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    hooks.use_effect(
        || {
            page.set(1);
        },
        props.current_explore.clone(),
    );

    element!(View{
        SearchInput(
            is_editing: is_editing,
            placeholder: "按s键搜索书籍, 按tab键切换频道",
            on_submit: move |text| {
                filter_text.set(text);
                page.set(1);
                true
            },
            clear_on_escape: true,
            on_clear: move |_| {
                filter_text.set(String::default());
            },
        )
        ListSelect<BookListItem>(
            items: books.read().as_ref().map(|b| b.items.clone()).unwrap_or_default(),
            top_title: Line::from(
                if let Some(explore)= &props.current_explore{
                    format!("选择书籍 ({})",explore.0.title)
                }else{
                    "选择书籍".to_string()
                }
            ).style(theme.title).centered(),
            bottom_title: {
                let books_g = books.read();
                let count = books_g.as_ref().map(|b| b.items.len()).unwrap_or(0);
                if count > 0 {
                    // 有 total_pages(render-dual-source)显「第 N / M 页」,否则「第 N 页」。
                    let label = match books_g.as_ref().and_then(|b| b.total_pages) {
                        Some(m) => format!("第 {} / {} 页", page.get(), m),
                        None => format!("第 {} 页", page.get()),
                    };
                    Line::from(
                        format!("{label}, {}/{}", list_state.read().selected.unwrap_or(0)+1, count)
                    ).centered().style(theme.meta_label)
                } else {
                    Line::from("暂无书籍").centered().style(theme.meta_label)
                }
            },
            is_editing: props.is_editing,
            empty_message: "暂无书籍，请切换频道，或者搜索",
            loading: loading.get(),
            loading_tip: if filter_text.read().is_empty() {
                "加载中..."
            } else {
                "搜索中..."
            },
            render_item: move |context:&ListBuildContext| {
                let list=books.read().as_ref().map(|b| b.items.clone()).unwrap_or_default();
                (FindBookItem {
                    book_list_item: list[context.index].clone(),
                    selected: context.is_selected,
                    theme,
                }.into(),8)
            },
            state: list_state,
            on_select: {
                let engine = props.engine.read().clone();
                move |item:BookListItem| {
                    if let Some(engine)=&engine{
                        navigate.push_with_state(
                            "/book-detail",
                            BookDetailState::new(item,engine.clone()),
                        );
                    }
                }
            },
        )
        WarningModal(
            // Display(非 Debug):让底层精心写的中文提示(渲染失败/未拦截到/浏览器不可用)直达用户。
            tip: error.read().as_ref().map(|e| e.to_string()).unwrap_or_default(),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
    })
}

pub struct FindBookItem {
    pub book_list_item: BookListItem,
    pub selected: bool,
    pub theme: AppChromeTheme,
}

impl WidgetRef for FindBookItem {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let item = &self.book_list_item;

        let block = if self.selected {
            Block::bordered()
                .padding(Padding::horizontal(2))
                .style(self.theme.selected)
        } else {
            Block::bordered().padding(Padding::horizontal(2))
        };
        let inner_area = block.inner(area);

        block.render(area, buf);

        let text_style = if self.selected {
            self.theme.text.patch(self.theme.selected)
        } else {
            self.theme.text
        };

        let mut text = vec![];

        if !item.info.name.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("名称：").style(self.theme.meta_label),
                    Span::from(&item.info.name),
                ])
                .style(text_style),
            );
        }
        if !item.info.author.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("作者：").style(self.theme.meta_label),
                    Span::from(&item.info.author),
                ])
                .style(text_style),
            );
        }

        if !item.info.kind.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("类型：").style(self.theme.meta_label),
                    Span::from(&item.info.kind),
                ])
                .style(text_style),
            );
        }

        if !item.info.last_chapter.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("最新章节：").style(self.theme.meta_label),
                    Span::from(&item.info.last_chapter),
                ])
                .style(text_style),
            );
        }

        if !item.info.word_count.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("字数：").style(self.theme.meta_label),
                    Span::from(&item.info.word_count),
                ])
                .style(text_style),
            );
        }

        if !item.info.intro.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("简介：").style(self.theme.meta_label),
                    Span::from(&item.info.intro),
                ])
                .style(text_style),
            );
        }

        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });
        paragraph.render(inner_area, buf);
    }
}
