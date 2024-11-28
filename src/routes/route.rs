use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::components::{
    loading::Loading, read_novel::ReadNovel, select_novel::SelectNovel, LoadingPage,
};

use super::Router;

#[derive(Debug, Clone)]
pub enum Route {
    SelectNovel(PathBuf),
    ReadNovel(PathBuf),
}

impl Route {
    pub fn to_page(self) -> Arc<Mutex<dyn Router>> {
        match self {
            Route::SelectNovel(path) => Arc::new(Mutex::new(
                LoadingPage::<SelectNovel, PathBuf>::new(path, Some(Loading::new("扫描文件中..."))),
            )),
            Route::ReadNovel(path) => Arc::new(Mutex::new(LoadingPage::<ReadNovel, PathBuf>::new(
                path,
                Some(Loading::new("加载小说中...")),
            ))),
        }
    }
}
