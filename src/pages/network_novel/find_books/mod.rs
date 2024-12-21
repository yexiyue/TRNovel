use crate::{
    app::State,
    components::{Component, Loading, Search},
    errors::Errors,
    pages::{Page, PageWrapper},
    Events, Result, Router,
};
use anyhow::anyhow;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::{utils::Params, BookList, BookSource, ExploreItem, JsonSource};
use ratatui::layout::{Constraint, Layout};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

pub mod books;
pub use books::*;
pub mod select_explore;
pub use select_explore::*;

pub enum FindBooksMsg {
    Search(String),
    SelectExplore(ExploreItem),
    BookList(BookList),
    Error(Errors),
}

#[derive(Debug, Clone)]
pub enum Current {
    Search(String),
    Explore,
}

pub struct FindBooks<'a> {
    pub book_source: Arc<Mutex<JsonSource>>,
    pub explore: Option<SelectExplore<'a>>,
    pub search: Search<'a>,
    pub book_list: Books,
    pub navigator: crate::Navigator,
    pub sender: Sender<FindBooksMsg>,
    pub current_explore: Option<ExploreItem>,
    pub current: Option<Current>,
}

#[async_trait]
impl Page<JsonSource> for FindBooks<'_> {
    type Msg = FindBooksMsg;
    async fn init(
        json_source: JsonSource,
        sender: Sender<Self::Msg>,
        navigator: crate::Navigator,
        _state: State,
    ) -> Result<Self> {
        let explores = json_source.explores.clone();
        let book_source = Arc::new(Mutex::new(json_source));

        let (explore, book_list) = if let Some(explores) = explores {
            if let Some(first) = explores.first() {
                sender
                    .send(FindBooksMsg::SelectExplore(first.clone()))
                    .await
                    .map_err(|_| anyhow!("发送消息失败"))?;
                (
                    Some(SelectExplore::new(explores, sender.clone())),
                    Books::new(
                        navigator.clone(),
                        "频道列表",
                        "暂无书籍",
                        Loading::new("加载中..."),
                        true,
                        book_source.clone(),
                    ),
                )
            } else {
                (
                    None,
                    Books::new(
                        navigator.clone(),
                        "搜索结果",
                        "请输入搜索内容",
                        Loading::new("搜索中..."),
                        false,
                        book_source.clone(),
                    ),
                )
            }
        } else {
            (
                None,
                Books::new(
                    navigator.clone(),
                    "搜索结果",
                    "请输入搜索内容",
                    Loading::new("搜索中..."),
                    false,
                    book_source.clone(),
                ),
            )
        };

        let sender_clone = sender.clone();
        let search = Search::new(
            "请输入关键字",
            move |query| {
                sender_clone.try_send(FindBooksMsg::Search(query)).unwrap();
            },
            |_| (true, ""),
        );

        Ok(Self {
            book_source,
            explore,
            search,
            book_list,
            navigator,
            sender,
            current: None,
            current_explore: None,
        })
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            FindBooksMsg::Search(text) => {
                if text.is_empty() {
                    if self.current_explore.is_none() {
                        self.current = None;
                    } else {
                        self.current = Some(Current::Explore);
                    }
                } else {
                    self.current = Some(Current::Search(text.clone()));
                }

                self.book_list.page = 1;
                self.get_book_list();
            }
            FindBooksMsg::SelectExplore(explore) => {
                self.current_explore = Some(explore.clone());
                self.current = Some(Current::Explore);

                self.book_list.page = 1;
                self.get_book_list();
            }
            FindBooksMsg::BookList(book_list) => {
                self.book_list.is_loading = false;
                if book_list.is_empty() {
                    self.book_list.books = None;
                } else {
                    self.book_list.set_books(book_list);
                }
            }
            FindBooksMsg::Error(error) => {
                return Err(error);
            }
        }
        Ok(())
    }
}

impl Router for FindBooks<'_> {}

impl FindBooks<'_> {
    pub fn to_page_route(
        book_source: BookSource,
    ) -> Result<PageWrapper<FindBooks<'static>, JsonSource, FindBooksMsg>> {
        let json_source = JsonSource::try_from(book_source)?;
        Ok(PageWrapper::new(json_source, None))
    }

    fn get_book_list(&mut self) {
        let sender = self.sender.clone();

        let page = self.book_list.page;
        let page_size = 10;

        if matches!(self.current, Some(Current::Explore)) {
            self.book_list.loading = Loading::new("加载中...");
            self.book_list.set_title("频道列表");
            if page > 1 {
                self.book_list.set_empty_tip("暂无更多书籍");
            } else {
                self.book_list.set_empty_tip("暂无书籍");
            }
        } else {
            self.book_list.loading = Loading::new("搜索中...");
            self.book_list.set_title("搜索结果");
            if page > 1 {
                self.book_list.set_empty_tip("暂无更多书籍");
            } else {
                self.book_list.set_empty_tip("没有找到相关书籍");
            }
        }

        if let Some(current) = self.current.clone() {
            self.book_list.state.select(None);
            self.book_list.is_loading = true;

            let explore = self.current_explore.clone();
            let book_source = self.book_source.clone();
            tokio::spawn(async move {
                if let Err(e) = (async {
                    let book_list = match current {
                        Current::Search(key) => {
                            book_source
                                .lock()
                                .await
                                .search_books(
                                    Params::new().key(&key).page(page).page_size(page_size),
                                )
                                .await?
                        }
                        Current::Explore => {
                            book_source
                                .lock()
                                .await
                                .explore_books(
                                    &explore.unwrap(),
                                    Params::new().page(page).page_size(page_size),
                                )
                                .await?
                        }
                    };
                    sender
                        .send(FindBooksMsg::BookList(book_list))
                        .await
                        .map_err(|_| anyhow!("发送消息失败"))?;
                    Ok(())
                })
                .await
                {
                    sender.send(FindBooksMsg::Error(e)).await.unwrap();
                }
            });
        }
    }
}
#[async_trait]
impl Component for FindBooks<'_> {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        let [top, content] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        self.search.render(frame, top)?;
        self.book_list.render(frame, content)?;

        if let Some(explore) = &mut self.explore {
            explore.render(frame, area)?;
        }
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        _state: State,
    ) -> crate::Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        match key.code {
            KeyCode::Left => {
                self.book_list.page = self.book_list.page.saturating_sub(1).max(1);
                self.get_book_list();
                Ok(None)
            }
            KeyCode::Right => {
                self.book_list.page = self.book_list.page.saturating_add(1);
                self.get_book_list();
                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }

    async fn handle_events(
        &mut self,
        events: crate::Events,
        state: State,
    ) -> crate::Result<Option<crate::Events>> {
        let Some(events) = self.search.handle_events(events, state.clone()).await? else {
            return Ok(None);
        };

        let Some(events) = (if let Some(explore) = &mut self.explore {
            let Some(events) = explore.handle_events(events, state.clone()).await? else {
                return Ok(None);
            };
            Some(events)
        } else {
            None
        }) else {
            return Ok(None);
        };

        let Some(events) = self.book_list.handle_events(events, state.clone()).await? else {
            return Ok(None);
        };

        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .await
                .map(|item| item.map(Events::KeyEvent)),
            other => Ok(Some(other)),
        }
    }
}
