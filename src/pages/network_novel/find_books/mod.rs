use crate::{
    Events, Result, Router,
    app::State,
    components::{Component, KeyShortcutInfo, Loading, Search},
    errors::Errors,
    pages::{Page, PageWrapper},
};
use anyhow::anyhow;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::{BookList, BookSource, BookSourceParser, ExploreItem, ExploreList};
use ratatui::layout::{Constraint, Layout};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;

pub mod books;
pub use books::*;
pub mod select_explore;
pub use select_explore::*;

pub enum FindBooksMsg {
    Init(ExploreList),
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
    pub book_source_parser: Arc<Mutex<BookSourceParser>>,
    pub explore: Option<SelectExplore<'a>>,
    pub search: Search<'a>,
    pub book_list: Books,
    pub navigator: crate::Navigator,
    pub sender: Sender<FindBooksMsg>,
    pub current_explore: Option<ExploreItem>,
    pub current: Option<Current>,
}

#[async_trait]
impl Page<BookSourceParser> for FindBooks<'_> {
    type Msg = FindBooksMsg;
    async fn init(
        parser: BookSourceParser,
        sender: Sender<Self::Msg>,
        navigator: crate::Navigator,
        _state: State,
    ) -> Result<Self> {
        tokio::spawn({
            let sender_clone = sender.clone();
            let mut parser = parser.clone();
            async move {
                match parser.get_explores().await {
                    Ok(explores) => sender_clone.send(FindBooksMsg::Init(explores)).await,
                    Err(e) => sender_clone.send(FindBooksMsg::Error(e.into())).await,
                }
            }
        });

        let book_source_parser = Arc::new(Mutex::new(parser));

        let sender_clone = sender.clone();
        let search = Search::new(
            "请输入关键字",
            move |query| {
                sender_clone.try_send(FindBooksMsg::Search(query)).unwrap();
            },
            |_| (true, ""),
        );

        Ok(Self {
            explore: None,
            search,
            book_list: Books::new(
                navigator.clone(),
                "搜索结果",
                "请输入搜索内容",
                Loading::new("加载分类中..."),
                true,
                book_source_parser.clone(),
            ),
            book_source_parser,
            navigator,
            sender,
            current: None,
            current_explore: None,
        })
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            FindBooksMsg::Init(explores) => {
                if !explores.is_empty() {
                    if let Some(first) = explores.first() {
                        self.sender
                            .send(FindBooksMsg::SelectExplore(first.clone()))
                            .await
                            .unwrap();

                        self.book_list.set_title("频道列表");
                        self.book_list.set_empty_tip("暂无书籍");
                        self.book_list.set_loading(Loading::new("加载中..."), true);
                        self.explore = Some(SelectExplore::new(explores, self.sender.clone()));
                    } else {
                        self.book_list.set_title("搜索结果");
                        self.book_list.set_empty_tip("请输入搜索内容");
                        self.book_list.set_loading(Loading::new("搜索中..."), false);
                    }
                } else {
                    self.book_list.set_loading(Loading::new("搜索中..."), false);
                };
            }
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
    ) -> Result<PageWrapper<FindBooks<'static>, BookSourceParser, FindBooksMsg>> {
        let json_source = BookSourceParser::try_from(book_source)?;
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
            let book_source = self.book_source_parser.clone();
            tokio::spawn(async move {
                if let Err(e) = (async {
                    let book_list = match current {
                        Current::Search(key) => {
                            book_source
                                .lock()
                                .await
                                .search_books(&key, page as u32, page_size)
                                .await?
                        }
                        Current::Explore => {
                            book_source
                                .lock()
                                .await
                                .explore_books(&explore.unwrap().url, page as u32, page_size)
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
            KeyCode::Left | KeyCode::Char('h') => {
                self.book_list.page = self.book_list.page.saturating_sub(1).max(1);
                self.get_book_list();
                Ok(None)
            }
            KeyCode::Right | KeyCode::Char('l') => {
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
            Some(events)
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

    fn key_shortcut_info(&self) -> crate::components::KeyShortcutInfo {
        let mut info = KeyShortcutInfo::new(vec![
            ("选择下一个书籍", "J / ▼"),
            ("选择上一个书籍", "K / ▲"),
            ("下一页", "L / ►"),
            ("上一页", "H / ◄"),
        ]);

        if let Some(explore) = &self.explore {
            let explore_info = explore.key_shortcut_info();

            if explore.state.show {
                info = explore_info;
            } else {
                info.append(&mut KeyShortcutInfo::new(vec![
                    ("进入频道列表", "Tab"),
                    ("进入搜索模式", "S"),
                    ("退出搜索模式", "ESC"),
                    ("搜索/进入阅读模式", "Enter"),
                ]));
            }
        } else {
            info.append(&mut KeyShortcutInfo::new(vec![
                ("进入搜索模式", "S"),
                ("退出搜索模式", "ESC"),
                ("搜索/进入阅读模式", "Enter"),
            ]));
        }

        info
    }
}
