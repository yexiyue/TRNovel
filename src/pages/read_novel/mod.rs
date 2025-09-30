use crate::{
    Events, Navigator, Result, Router,
    app::State,
    components::{Component, KeyShortcutInfo, Loading},
    errors::Errors,
    novel::Novel,
    pages::{Page, PageWrapper},
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Size};
use read_content::ReadContent;
use select_chapter::SelectChapter;
use tokio::sync::mpsc;

pub mod read_content;
pub mod select_chapter;

pub enum ReadNovelMsg<T: Novel + 'static> {
    Next,
    Prev,
    SelectChapter(usize),
    QueryChapters(String),
    Chapters(Vec<T::Chapter>),
    Error(Errors),
    Content(String),
    Initialized(T),
}

pub struct ReadNovel<T: Novel + Send + Sync + 'static> {
    pub loading: Loading,
    pub chapters: Vec<(String, usize)>,
    pub select_chapter: SelectChapter<'static, T>,
    pub sender: mpsc::Sender<ReadNovelMsg<T>>,
    pub read_content: ReadContent<T>,
    pub show_select_chapter: bool,
    pub novel: Option<T>,
    pub init_line_percent: Option<f64>,
}

impl<T> ReadNovel<T>
where
    T: Novel + Send + Sync,
{
    pub fn to_page_route(
        init_args: T::Args,
    ) -> PageWrapper<ReadNovel<T>, T::Args, ReadNovelMsg<T>> {
        PageWrapper::new(init_args, None)
    }

    pub fn get_content(&mut self) -> Result<()> {
        let sender_clone = self.sender.clone();
        self.novel.as_mut().unwrap().get_content(move |res| {
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
impl<T> Page<T::Args> for ReadNovel<T>
where
    T: Novel + Send + Sync + 'static,
{
    type Msg = ReadNovelMsg<T>;
    async fn init(
        init_args: T::Args,
        sender: mpsc::Sender<Self::Msg>,
        _navigator: Navigator,
        state: State,
    ) -> Result<Self> {
        let sender_clone = sender.clone();

        tokio::spawn(async move {
            match T::init(init_args).await {
                Ok(novel) => {
                    let msg = ReadNovelMsg::Initialized(novel);
                    sender_clone.send(msg).await.unwrap();
                }
                Err(e) => {
                    let msg = ReadNovelMsg::Error(e);
                    sender_clone.send(msg).await.unwrap();
                }
            }
        });

        let size = state.size.lock().await.unwrap();

        Ok(Self {
            init_line_percent: None,
            loading: Loading::new("加载小说中..."),
            select_chapter: SelectChapter::new(sender.clone(), None, 0),
            read_content: ReadContent::new(
                Size::new(size.width - 4, size.height - 5),
                sender.clone(),
                true,
            )?,
            novel: None,
            sender,
            show_select_chapter: true,
            chapters: vec![],
        })
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        let sender_clone = self.sender.clone();
        match msg {
            ReadNovelMsg::Initialized(mut novel) => {
                self.init_line_percent = Some(novel.line_percent);

                if let Ok(chapters) = novel.get_chapters_names() {
                    self.chapters = chapters.clone();
                    self.select_chapter.set_total_chapters(chapters.len());
                    self.select_chapter
                        .set_list(chapters, Some(novel.current_chapter));
                }

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
                self.novel = Some(novel);
            }
            ReadNovelMsg::Chapters(chapters) => {
                self.novel.as_mut().unwrap().set_chapters(&chapters);

                let chapters = self.novel.as_ref().unwrap().get_chapters_names()?;

                self.chapters = chapters.clone();
                self.select_chapter.set_total_chapters(chapters.len());
                self.select_chapter
                    .set_list(chapters, Some(self.novel.as_mut().unwrap().current_chapter));

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content()?;
            }
            ReadNovelMsg::QueryChapters(query) => {
                if let Some(index) = query.strip_prefix("$") {
                    if let Ok(index) = index.parse::<usize>() {
                        if let Some(chapter) = self.chapters.get(index.saturating_sub(1)) {
                            self.select_chapter.set_list(vec![chapter.clone()], None);
                        } else {
                            self.select_chapter.set_list(vec![], None);
                        }

                        return Ok(());
                    }
                }

                let filter_list = self
                    .chapters
                    .iter()
                    .filter(|(item, _)| item.contains(&query))
                    .cloned()
                    .collect::<Vec<_>>();

                self.select_chapter.set_list(filter_list, None);
            }
            ReadNovelMsg::SelectChapter(index) => {
                self.show_select_chapter = false;

                // 如果是当前章节，直接返回
                if index == self.novel.as_ref().unwrap().current_chapter {
                    return Ok(());
                }

                self.novel.as_mut().unwrap().set_chapter(index)?;

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content()?;
            }
            ReadNovelMsg::Content(content) => {
                self.read_content
                    .set_content(content, self.init_line_percent.take());

                if let Ok(chapter_name) = self.novel.as_ref().unwrap().get_current_chapter_name() {
                    self.read_content.set_current_chapter(chapter_name);
                }

                self.read_content
                    .set_chapter_percent(self.novel.as_ref().unwrap().chapter_percent()?);

                self.read_content.set_loading(false);
            }
            ReadNovelMsg::Next => {
                self.novel.as_mut().unwrap().next_chapter()?;

                self.read_content.set_loading(true);

                self.read_content.set_current_line(0);

                self.get_content()?;
            }
            ReadNovelMsg::Prev => {
                self.novel.as_mut().unwrap().prev_chapter()?;

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
impl<T> Component for ReadNovel<T>
where
    T: Novel + Send + Sync,
{
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self
            .novel
            .as_ref()
            .is_none_or(|novel| novel.get_chapters().is_none())
        {
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
        if self
            .novel
            .as_ref()
            .is_none_or(|novel| novel.get_chapters().is_none())
        {
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

                // 选中当前正在阅读的章节
                if self.show_select_chapter {
                    self.select_chapter.search.set_value("");
                    self.select_chapter.set_list(
                        self.chapters.clone(),
                        Some(self.novel.as_ref().unwrap().current_chapter),
                    );
                }
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

            if matches!(events, Events::Tick) {
                self.read_content.handle_tick(state.clone()).await?;
            }

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
                ("搜索章节", "S"),
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

#[async_trait]
impl<T> Router for ReadNovel<T>
where
    T: Novel + Send + Sync,
{
    // 在回退时添加进历史记录
    async fn on_hide(&mut self, state: State) -> Result<()> {
        // 这里需要更新行数进度
        let percent =
            self.read_content.current_line as f64 / self.read_content.content_lines as f64;
        self.novel.as_mut().unwrap().line_percent = percent;

        state.history.lock().await.add(
            &self.novel.as_mut().unwrap().get_id(),
            self.novel.as_mut().unwrap().to_history_item()?,
        );
        Ok(())
    }
}
