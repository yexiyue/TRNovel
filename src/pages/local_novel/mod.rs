use crate::{components::LoadingWrapper, Result, RoutePage};
use std::path::PathBuf;

pub mod select_file;
use select_file::SelectFile;

pub fn local_novel_first_page(path: PathBuf) -> Result<Box<dyn RoutePage>> {
    Ok(LoadingWrapper::<SelectFile>::route_page(
        "扫描文件中...",
        path,
        None,
    ))
}
