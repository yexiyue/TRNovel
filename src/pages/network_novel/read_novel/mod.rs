use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::{BookInfo, Chapter, ChapterList};
use ratatui::layout::{Constraint, Layout};
use read_content::ReadContent;
use select_chapter::SelectChapter;
use tokio::sync::mpsc;

use crate::{
    app::State,
    components::{Component, Loading},
    errors::Errors,
    pages::{Page, PageWrapper},
    Events, Navigator, Result, Router,
};

pub mod read_content;
pub mod select_chapter;

pub enum ReadNovelMsg {
    Next,
    Prev,
    ScrollToPrev,
    SelectChapter(usize),
    QueryChapters(String),
    Chapters(ChapterList),
    Error(Errors),
    Content(String),
}

pub struct ReadNovel {
    pub loading: Loading,
    pub chapters: Option<ChapterList>,
    pub select_chapter: SelectChapter<'static>,
    pub state: State,
    pub sender: mpsc::Sender<ReadNovelMsg>,
    pub current_chapter: usize,
    pub read_content: ReadContent<'static>,
    pub show_select_chapter: bool,
}

impl ReadNovel {
    pub fn to_page_route(book_info: BookInfo) -> PageWrapper<ReadNovel, BookInfo, ReadNovelMsg> {
        PageWrapper::new(book_info, None)
    }

    fn get_current_chapter(&self) -> Chapter {
        self.chapters
            .as_ref()
            .unwrap()
            .chapter_list
            .get(self.current_chapter)
            .unwrap()
            .clone()
    }
    fn get_content(&self) {
        let book_source = self.state.book_source.clone();
        let sender_clone = self.sender.clone();
        let chapter = self.get_current_chapter();
        tokio::spawn(async move {
            match book_source
                .lock()
                .await
                .as_mut()
                .unwrap()
                .chapter_content(&chapter)
                .await
            {
                Err(e) => {
                    sender_clone
                        .send(ReadNovelMsg::Error(e.into()))
                        .await
                        .unwrap();
                }
                Ok(content) => {
                    sender_clone
                        .send(ReadNovelMsg::Content(content))
                        .await
                        .unwrap();
                }
            }
        });
    }
}

#[async_trait]
impl Page<BookInfo> for ReadNovel {
    type Msg = ReadNovelMsg;
    async fn init(
        arg: BookInfo,
        sender: mpsc::Sender<Self::Msg>,
        _navigator: Navigator,
        state: State,
    ) -> Result<Self> {
        let book_source = state.book_source.clone();
        let sender_clone = sender.clone();
        tokio::spawn(async move {
            match book_source
                .lock()
                .await
                .as_mut()
                .unwrap()
                .chapter_list(&arg)
                .await
            {
                Err(e) => {
                    sender_clone
                        .send(ReadNovelMsg::Error(e.into()))
                        .await
                        .unwrap();
                }
                Ok(chapter_list) => {
                    sender_clone
                        .send(ReadNovelMsg::Chapters(chapter_list))
                        .await
                        .unwrap();
                }
            }
        });

        Ok(Self {
            chapters: None,
            loading: Loading::new("获取章节目录中..."),
            select_chapter: SelectChapter::new(sender.clone(), None),
            read_content: ReadContent::new(
                (*state.size.clone().lock().unwrap()).unwrap(),
                sender.clone(),
                true,
            )?,
            state,
            sender,
            current_chapter: 0,
            show_select_chapter: true,
        })
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            ReadNovelMsg::Chapters(chapters) => {
                self.chapters.replace(chapters.clone());
                self.select_chapter.set_list(chapters);

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content();
            }
            ReadNovelMsg::QueryChapters(query) => {
                if let Some(res) = self.chapters.clone().map(|chapters| {
                    chapters
                        .chapter_list
                        .into_iter()
                        .filter(|item| item.chapter_name.contains(&query))
                        .collect::<Vec<_>>()
                }) {
                    self.select_chapter.set_list(res.into());
                }
            }
            ReadNovelMsg::SelectChapter(index) => {
                self.show_select_chapter = false;
                self.current_chapter = index;

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content();
            }
            ReadNovelMsg::Content(content) => {
                self.read_content
                    .set_current_chapter(self.get_current_chapter());
                self.read_content.set_content(content);
                self.read_content.set_chapter_percent(
                    self.current_chapter as f64
                        / self.chapters.as_ref().unwrap().chapter_list.len() as f64
                        * 100.0,
                );
                self.read_content.set_loading(false);
            }
            ReadNovelMsg::Next => {
                if self.current_chapter >= self.chapters.as_ref().unwrap().chapter_list.len() - 1 {
                    return Err(Errors::Warning("已经是最后一章了".into()));
                }

                self.current_chapter = self.current_chapter.saturating_add(1);

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content();
            }
            ReadNovelMsg::Prev => {
                self.read_content.set_loading(true);
                if self.current_chapter == 0 {
                    return Err(Errors::Warning("已经是第一章了".into()));
                }
                self.current_chapter = self.current_chapter.saturating_sub(1);

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content();
            }
            ReadNovelMsg::ScrollToPrev => {
                self.read_content.set_loading(true);

                if self.current_chapter == 0 {
                    return Err(Errors::Warning("已经是第一章了".into()));
                }

                self.current_chapter = self.current_chapter.saturating_sub(1);

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content();
            }

            _ => {}
        }
        Ok(())
    }
}

#[async_trait]
impl Router for ReadNovel {}

#[async_trait]
impl Component for ReadNovel {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self.chapters.is_none() {
            frame.render_widget(&self.loading, area);
        } else if self.show_select_chapter {
            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(area);
            self.select_chapter.render(frame, left_area)?;
            self.read_content.render(frame, right_area)?;
        } else {
            self.read_content.render(frame, area)?;
        }

        Ok(())
    }

    async fn handle_tick(&mut self, _state: State) -> Result<()> {
        if self.chapters.is_none() {
            self.loading.state.calc_next();
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent, _state: State) -> Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        match key.code {
            KeyCode::Tab => {
                self.show_select_chapter = !self.show_select_chapter;
                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        let events = if self.show_select_chapter {
            let Some(events) = self
                .select_chapter
                .handle_events(events, state.clone())
                .await?
            else {
                return Ok(None);
            };

            let Some(events) = self
                .read_content
                .handle_events(events, state.clone())
                .await?
            else {
                return Ok(None);
            };
            events
        } else {
            let Some(events) = self
                .read_content
                .handle_events(events, state.clone())
                .await?
            else {
                return Ok(None);
            };
            events
        };

        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .await
                .map(|item| item.map(Events::KeyEvent)),

            Events::Tick => {
                self.handle_tick(state).await?;

                Ok(Some(Events::Tick))
            }
            _ => Ok(Some(events)),
        }
    }
}
