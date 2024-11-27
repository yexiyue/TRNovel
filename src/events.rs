use std::path::PathBuf;

use tui_tree_widget::TreeItem;

use crate::{components::loading::Loading, history::History, novel::TxtNovel};

#[derive(Debug, Clone)]
pub enum Events {
    Init(PathBuf),
    Loading(Loading),
    CrosstermEvent(crossterm::event::Event),
    ReadNovel(TxtNovel),
    SelectNovel((Vec<TreeItem<'static, PathBuf>>, History)),
    Tick,
}
