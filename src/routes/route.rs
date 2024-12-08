use std::path::PathBuf;

use crate::components::{Loading, Page, ReadNovel, SelectNovel};

use super::Router;

#[derive(Debug, Clone)]
pub enum Route {
    SelectNovel(PathBuf),
    ReadNovel(PathBuf),
}

impl Route {
    pub fn to_page(self) -> Box<dyn Router> {
        match self {
            Route::SelectNovel(path) => Box::new(Page::<SelectNovel, PathBuf>::new(
                path,
                Loading::new("扫描文件中..."),
            )),
            Route::ReadNovel(path) => Box::new(Page::<ReadNovel, PathBuf>::new(
                path,
                Loading::new("加载小说中..."),
            )),
        }
    }
}
