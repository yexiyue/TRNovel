use crate::{
    actions::Actions,
    components::{
        loading::Loading,
        read_novel::ReadNovel,
        select_novel::{SelectFile, SelectHistory, SelectNovel},
        Component,
    },
    events::Events,
    file_list::NovelFiles,
    history::History,
    novel::TxtNovel,
};

#[derive(Clone, Debug)]
pub enum Mode<'a> {
    Loading(Loading),
    Select(SelectNovel<'a>),
    Read(ReadNovel),
}

impl<'a> Default for Mode<'a> {
    fn default() -> Self {
        Self::Loading(Loading::new("扫描文件中..."))
    }
}

impl<'a> Component for Mode<'a> {
    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        match self {
            Self::Loading(loading) => {
                frame.render_widget(loading, frame.area());
            }
            Self::Select(select) => select.draw(frame, area)?,
            Self::Read(read) => read.draw(frame, area)?,
        }
        Ok(())
    }

    fn handle_term_events(
        &mut self,
        event: crossterm::event::Event,
    ) -> anyhow::Result<Option<crate::actions::Actions>> {
        match self {
            Self::Loading(_) => Ok(None),
            Self::Select(select) => select.handle_term_events(event),
            Self::Read(read) => read.handle_term_events(event),
        }
    }

    fn handle_events(
        &mut self,
        events: Events,
        tx: tokio::sync::mpsc::UnboundedSender<Events>,
    ) -> anyhow::Result<()> {
        match events {
            Events::CrosstermEvent(event) => match self.handle_term_events(event)? {
                Some(Actions::SelectedFile(path)) => {
                    tokio::spawn(async move {
                        match (|| {
                            tx.send(Events::Loading(Loading::new("正在加载小说...")))?;
                            let novel = TxtNovel::from_path(path)?;
                            tx.send(Events::ReadNovel(novel))?;
                            Ok::<(), anyhow::Error>(())
                        })() {
                            Ok(_) => {}
                            Err(e) => {
                                tx.send(Events::Error(e.to_string())).unwrap();
                            }
                        }
                    });
                }
                _ => {}
            },
            Events::Init(path) => {
                tokio::spawn(async move {
                    match (|| {
                        let novel_files = NovelFiles::from_path(path)?;
                        let history = History::default()?;
                        match novel_files {
                            NovelFiles::File(path) => {
                                tx.send(Events::Loading(Loading::new("正在加载小说...")))?;
                                let novel = TxtNovel::from_path(path)?;
                                tx.send(Events::ReadNovel(novel))?;
                            }
                            NovelFiles::FileTree(tree) => {
                                tx.send(Events::SelectNovel((tree, history)))?;
                            }
                        }
                        Ok::<(), anyhow::Error>(())
                    })() {
                        Ok(_) => {}
                        Err(e) => {
                            tx.send(Events::Error(e.to_string())).unwrap();
                        }
                    }
                });
            }
            Events::Loading(loading) => {
                *self = Self::Loading(loading);
            }
            Events::ReadNovel(novel) => {
                *self = Self::Read(ReadNovel::new(novel)?);
            }
            Events::SelectNovel((tree, history)) => {
                *self = Self::Select(SelectNovel::new(
                    SelectFile::new(tree)?,
                    SelectHistory::new(history.histories),
                )?);
            }
            Events::Tick => match self {
                Self::Loading(loading) => {
                    loading.state.calc_next();
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }
}
