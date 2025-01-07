use crate::{
    app::State,
    components::{Component, Empty, KeyShortcutInfo, Loading, Search},
    errors::Errors,
    file_list::NovelFiles,
    novel::local_novel::LocalNovel,
    pages::{Page, PageWrapper, ReadNovel},
    Events, History, Navigator, Result, RoutePage, Router,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Scrollbar},
};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tui_tree_widget::{Tree, TreeItem, TreeState};

pub enum SelectFileMsg<'a> {
    InputDirOrFile(String),
    Files(NovelFiles<'a>),
    Error(Errors),
}

pub struct SelectFile<'a: 'static> {
    pub state: TreeState<PathBuf>,
    pub items: Vec<TreeItem<'a, PathBuf>>,
    pub navigator: Navigator,
    pub loading: Loading,
    pub is_loading: bool,
    pub search: Search<'a>,
    pub sender: tokio::sync::mpsc::Sender<SelectFileMsg<'a>>,
    pub history: Arc<Mutex<History>>,
}

impl<'a> SelectFile<'a> {
    pub fn to_page_route(arg: Option<PathBuf>) -> Box<dyn RoutePage> {
        Box::new(PageWrapper::<
            SelectFile<'a>,
            Option<PathBuf>,
            SelectFileMsg<'a>,
        >::new(arg, None))
    }
    pub fn new(
        sender: tokio::sync::mpsc::Sender<SelectFileMsg<'a>>,
        navigator: Navigator,
        path: Option<PathBuf>,
        history: Arc<Mutex<History>>,
    ) -> Result<Self> {
        let sender_clone = sender.clone();
        let mut search = Search::new(
            "输入文件夹路径或文件名",
            move |query| {
                sender_clone
                    .try_send(SelectFileMsg::InputDirOrFile(query))
                    .unwrap();
            },
            |query| {
                let path = PathBuf::from(query);
                if path.exists() {
                    if path.is_file() && path.extension().unwrap_or_default() != "txt" {
                        (false, "文件格式不正确")
                    } else {
                        (true, "")
                    }
                } else {
                    (false, "路径不存在")
                }
            },
        );
        if let Some(path) = &path {
            let input = path.to_string_lossy();
            search.set_value(&input);
            sender
                .try_send(SelectFileMsg::InputDirOrFile(input.to_string()))
                .unwrap();
        }

        Ok(Self {
            state: TreeState::default(),
            items: vec![],
            navigator,
            loading: Loading::new("扫描文件中..."),
            is_loading: path.is_some(),
            search,
            sender,
            history,
        })
    }
}

#[async_trait]
impl Component for SelectFile<'_> {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        let [top, content] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        self.search.render(frame, top)?;

        let block = Block::bordered()
            .title(Line::from("本地小说").centered())
            .border_style(Style::new().dim());

        let inner_area = block.inner(content);
        frame.render_widget(block, content);

        if self.is_loading {
            frame.render_widget(&self.loading, inner_area);
        } else if self.items.is_empty() {
            if self.search.get_value().is_empty() {
                frame.render_widget(Empty::new("请输入文件夹路径或文件名"), inner_area);
            } else {
                frame.render_widget(Empty::new("该目录下未搜索到小说文件"), inner_area);
            }
        } else {
            let tree_widget = Tree::new(&self.items)?
                .highlight_style(Style::new().bold().on_light_cyan())
                .experimental_scrollbar(Some(Scrollbar::default()));

            frame.render_stateful_widget(tree_widget, inner_area, &mut self.state);
        }

        Ok(())
    }

    async fn handle_tick(&mut self, _state: State) -> Result<()> {
        if self.is_loading {
            self.loading.state.calc_next();
        }
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _state: State,
    ) -> Result<Option<KeyEvent>> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => {
                    self.state.key_left();
                    return Ok(None);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.state.key_down();
                    return Ok(None);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.state.key_up();
                    return Ok(None);
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    let res = self.state.selected().last();
                    if let Some(path) = res {
                        if path.is_file() {
                            self.navigator.push(Box::new(
                                ReadNovel::<LocalNovel>::to_page_route(path.clone()),
                            ))?;
                        } else {
                            self.state.toggle_selected();
                        }
                    }
                    return Ok(None);
                }
                _ => {
                    return Ok(Some(key));
                }
            }
        }
        Ok(Some(key))
    }

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        let Some(events) = self.search.handle_events(events, state.clone()).await? else {
            return Ok(None);
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
        KeyShortcutInfo::new(vec![
            ("选择下一个", "J / ▼"),
            ("选择上一个", "K / ▲"),
            ("取消选择", "H / ◄"),
            ("确认选择", "L / ► / Enter"),
        ])
    }
}

impl Router for SelectFile<'_> {}

#[async_trait]
impl Page<Option<PathBuf>> for SelectFile<'_> {
    type Msg = SelectFileMsg<'static>;
    async fn init(
        arg: Option<PathBuf>,
        sender: tokio::sync::mpsc::Sender<Self::Msg>,
        navigator: Navigator,
        State { history, .. }: State,
    ) -> Result<Self> {
        Ok(Self::new(sender, navigator, arg, history)?)
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            SelectFileMsg::InputDirOrFile(query) => {
                self.is_loading = true;
                let sender = self.sender.clone();
                let history = self.history.clone();
                tokio::spawn(async move {
                    match NovelFiles::from_path(query.clone().into()) {
                        Ok(files) => {
                            history.lock().await.local_path = Some(query.into());
                            sender.send(SelectFileMsg::Files(files)).await.unwrap();
                        }
                        Err(e) => {
                            sender.send(SelectFileMsg::Error(e.into())).await.unwrap();
                        }
                    }
                });
            }
            SelectFileMsg::Files(files) => {
                self.is_loading = false;
                self.items = files.into_tree_item();
            }
            SelectFileMsg::Error(e) => {
                self.is_loading = false;
                return Err(e);
            }
        }

        Ok(())
    }
}
