use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Size};
use read_content::ReadContent;
use select_chapter::SelectChapter;
use tokio::sync::mpsc;

use crate::{
    app::State,
    components::{Component, KeyShortcutInfo, Loading},
    errors::Errors,
    novel::Novel,
    pages::{Page, PageWrapper},
    Events, Navigator, Result, Router,
};

pub mod read_content;
pub mod select_chapter;

pub enum ReadNovelMsg<T: Novel> {
    Next,
    Prev,
    SelectChapter(usize),
    QueryChapters(String),
    Chapters(Vec<T::Chapter>),
    Error(Errors),
    Content(String),
}

pub struct ReadNovel<T: Novel + 'static> {
    pub loading: Loading,
    pub select_chapter: SelectChapter<'static, T>,
    pub sender: mpsc::Sender<ReadNovelMsg<T>>,
    pub read_content: ReadContent<'static, T>,
    pub show_select_chapter: bool,
    pub novel: T,
}

impl<T> ReadNovel<T>
where
    T: Novel + Send + Sync + 'static,
{
    pub fn to_page_route(novel: T) -> PageWrapper<ReadNovel<T>, T, ReadNovelMsg<T>> {
        PageWrapper::new(novel, None)
    }

    pub fn get_content(&mut self) -> Result<()> {
        let sender_clone = self.sender.clone();
        self.novel.get_content(move |res| {
            let msg = match res {
                Ok(content) => ReadNovelMsg::Content(content),
                Err(e) => ReadNovelMsg::Error(e),
            };
            sender_clone.try_send(msg).unwrap();
        })?;
        Ok(())
    }
}

#[async_trait]
impl<T> Page<T> for ReadNovel<T>
where
    T: Novel + Send + Sync + 'static,
{
    type Msg = ReadNovelMsg<T>;
    async fn init(
        mut novel: T,
        sender: mpsc::Sender<Self::Msg>,
        _navigator: Navigator,
        state: State,
    ) -> Result<Self> {
        let sender_clone = sender.clone();

        if novel.get_chapters().is_none() {
            novel.request_chapters(move |res| {
                let msg = match res {
                    Ok(chapters) => ReadNovelMsg::Chapters(chapters),
                    Err(e) => ReadNovelMsg::Error(e),
                };
                sender_clone.try_send(msg).unwrap();
            })?;
        } else {
            novel.get_content(move |res| {
                let msg = match res {
                    Ok(content) => ReadNovelMsg::Content(content),
                    Err(e) => ReadNovelMsg::Error(e),
                };
                sender_clone.try_send(msg).unwrap();
            })?;
        }

        let size = state.size.lock().unwrap().unwrap();

        Ok(Self {
            loading: Loading::new("加载小说中..."),
            select_chapter: SelectChapter::new(sender.clone(), novel.get_chapters_names().ok()),
            read_content: ReadContent::new(
                Size::new(size.width - 4, size.height - 5),
                sender.clone(),
                true,
            )?,
            novel,
            sender,
            show_select_chapter: true,
        })
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            ReadNovelMsg::Chapters(chapters) => {
                self.novel.set_chapters(&chapters);

                self.select_chapter
                    .set_list(self.novel.get_chapters_names()?);

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content()?;
            }
            ReadNovelMsg::QueryChapters(query) => {
                let filter_list = self
                    .novel
                    .get_chapters_names()?
                    .into_iter()
                    .filter(|item| item.contains(&query))
                    .collect::<Vec<_>>();
                self.select_chapter.set_list(filter_list);
            }
            ReadNovelMsg::SelectChapter(index) => {
                self.show_select_chapter = false;
                self.novel.set_chapter(index)?;

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content()?;
            }
            ReadNovelMsg::Content(content) => {
                self.read_content.set_content(content);

                self.read_content
                    .set_current_chapter(self.novel.get_current_chapter_name()?);

                self.read_content
                    .set_chapter_percent(self.novel.chapter_percent()?);

                self.read_content.set_loading(false);
            }
            ReadNovelMsg::Next => {
                self.novel.next_chapter()?;

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content()?;
            }
            ReadNovelMsg::Prev => {
                self.novel.prev_chapter()?;

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content()?;
            }
            ReadNovelMsg::Error(e) => {
                return Err(e);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl<T> Router for ReadNovel<T> where T: Novel + Send + Sync {}

#[async_trait]
impl<T> Component for ReadNovel<T>
where
    T: Novel + Send,
{
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self.novel.get_chapters().is_none() {
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
        if self.novel.get_chapters().is_none() {
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

    fn key_shortcut_info(&self) -> KeyShortcutInfo {
        let data = if self.show_select_chapter {
            vec![
                ("选择下一个", "J / ▼"),
                ("选择上一个", "K / ▲"),
                ("切换阅读模式", "Tab / Esc"),
                ("阅读选中章节", "Enter"),
            ]
        } else {
            vec![
                ("切换选择章节模式", "Tab"),
                ("下一行", "J / ▼ / Space"),
                ("上一行", "K / ▲"),
                ("下一章", "L / ►"),
                ("上一章", "H / ◄"),
                ("下一页", "PageDown"),
                ("上一页", "PageUp"),
            ]
        };
        KeyShortcutInfo::new(data)
    }
}

// todo 添加历史记录
// #[async_trait]
// impl Router for ReadNovel {
//     // 在回退时添加进历史记录
//     async fn on_hide(&mut self, state: State) -> Result<()> {
//         // 这里需要更新行数进度
//         let percent = self.novel.current_line as f64 / self.novel.content_lines as f64;
//         self.novel.inner.line_percent = percent;

//         state.history.lock().unwrap().add(
//             self.novel.path.clone(),
//             HistoryItem::from(&self.novel.inner),
//         );
//         Ok(())
//     }
// }