use crate::{app::State, components::LoadingWrapper, Navigator, Result, RoutePage};
use std::path::PathBuf;

pub mod read_novel;
pub use read_novel::ReadNovel;
pub mod select_novel;
pub use select_novel::SelectNovel;

pub fn local_novel_first_page(
    path: PathBuf,
    navigator: Navigator,
    state: State,
) -> Result<Box<dyn RoutePage>> {
    LoadingWrapper::<SelectNovel, PathBuf>::route_page(
        "扫描文件中...",
        navigator.clone(),
        state,
        path,
    )
}
