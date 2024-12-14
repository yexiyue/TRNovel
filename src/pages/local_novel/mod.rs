use crate::{components::LoadingWrapper, Result, RoutePage};
use std::path::PathBuf;

pub mod read_novel;
pub use read_novel::ReadNovel;
pub mod select_novel;
pub use select_novel::SelectNovel;

pub fn local_novel_first_page(path: PathBuf) -> Result<Box<dyn RoutePage>> {
    Ok(LoadingWrapper::<SelectNovel>::route_page(
        "扫描文件中...",
        path,
        None,
    ))
}
