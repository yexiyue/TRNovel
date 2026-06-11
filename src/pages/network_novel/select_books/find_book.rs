use crate::{
    ThemeConfig,
    components::{WarningModal, list_select::ListSelect, search_input::SearchInput},
    errors::Errors,
    hooks::{UseInitState, UseThemeConfig},
    pages::network_novel::book_detail::BookDetailState,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::{BookListItem, Engine};
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
    let theme = hooks.use_theme_config();
    let is_inputting = *hooks.use_context::<State<bool>>();
    let mut page = hooks.use_state(|| 1);
    let page_size = hooks.use_state(|| 20);
    let list_state = hooks.use_state(ListState::default);
    let mut navigate = hooks.use_navigate();
    let is_editing = props.is_editing;

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_editing
            && !is_inputting.get()
        {
            match key.code {
                KeyCode::Char('h') | KeyCode::Left if page.get() > 1 => {
                    page.set(page.get() - 1);
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    page.set(page.get() + 1);
                }
                _ => {}
            }
        }
    });

    hooks.use_effect(
        || {
            page.set(1);
        },
        props.current_explore.clone(),
    );

    let (books, loading, error) = hooks.use_effect_state(
        {
            // 该 block 每次渲染都求值,**只能捕获值、绝不能在此 spawn 取页**:
            // 在此 `tokio::spawn(explore)` 会绕过 use_async_effect 的 deps 控制,每帧开一次
            // 浏览器(render explore 时系统栏狂闪)。explore/search 一律在下面 async 体内 await,
            // 由 use_async_effect 按 deps 仅在依赖变化时触发一次。
            let engine = props.engine.read().clone();
            let url = props.current_explore.as_ref().map(|e| e.0.url.clone());
            let page = page.get();
            let page_size = page_size.get();
            let filter_text = filter_text.read().clone();
            async move {
                let Some(engine) = engine else {
                    return Ok::<Vec<BookListItem>, Errors>(vec![]);
                };
                let res = if filter_text.is_empty() {
                    match url {
                        Some(url) => engine.explore(&url, page, page_size).await?,
                        None => return Ok(vec![]),
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
            items: books.read().clone().unwrap_or_default(),
            top_title: Line::from(
                if let Some(explore)= &props.current_explore{
                    format!("选择书籍 ({})",explore.0.title)
                }else{
                    "选择书籍".to_string()
                }
            ).style(theme.basic.border_title).centered(),
            bottom_title: if books.read().as_ref().map(|b|b.len()).unwrap_or(0)>0{
                Line::from(
                    format!("第 {} 页, {}/{}", page.get(), list_state.read().selected.unwrap_or(0)+1, books.read().as_ref().map(|b|b.len()).unwrap_or(0))
                ).centered().style(theme.basic.border_info)
            }else{
                Line::from("暂无书籍").centered().style(theme.basic.border_info)
            },
            is_editing: !is_inputting.get() && props.is_editing,
            empty_message: "暂无书籍，请切换频道，或者搜索",
            loading: loading.get(),
            loading_tip: if filter_text.read().is_empty() {
                "加载中..."
            } else {
                "搜索中..."
            },
            render_item: move |context:&ListBuildContext| {
                let list=books.read().clone().unwrap_or_default();
                (FindBookItem {
                    book_list_item: list[context.index].clone(),
                    selected: context.is_selected,
                    theme: theme.clone(),
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
    pub theme: ThemeConfig,
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
            self.theme.basic.text.patch(self.theme.selected)
        } else {
            self.theme.basic.text
        };

        let mut text = vec![];

        if !item.info.name.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("名称：").style(self.theme.basic.border_info),
                    Span::from(&item.info.name),
                ])
                .style(text_style),
            );
        }
        if !item.info.author.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("作者：").style(self.theme.basic.border_info),
                    Span::from(&item.info.author),
                ])
                .style(text_style),
            );
        }

        if !item.info.kind.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("类型：").style(self.theme.basic.border_info),
                    Span::from(&item.info.kind),
                ])
                .style(text_style),
            );
        }

        if !item.info.last_chapter.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("最新章节：").style(self.theme.basic.border_info),
                    Span::from(&item.info.last_chapter),
                ])
                .style(text_style),
            );
        }

        if !item.info.word_count.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("字数：").style(self.theme.basic.border_info),
                    Span::from(&item.info.word_count),
                ])
                .style(text_style),
            );
        }

        if !item.info.intro.is_empty() {
            text.push(
                Line::from(vec![
                    Span::from("简介：").style(self.theme.basic.border_info),
                    Span::from(&item.info.intro),
                ])
                .style(text_style),
            );
        }

        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });
        paragraph.render(inner_area, buf);
    }
}
