use crate::RoutePage;
use std::path::PathBuf;

pub mod select_file;
use select_file::SelectFile;

pub fn local_novel_first_page(path: Option<PathBuf>) -> Box<dyn RoutePage> {
    SelectFile::to_page_route(path)
}
