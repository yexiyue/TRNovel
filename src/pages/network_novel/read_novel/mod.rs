use crate::components::Loading;


pub mod read_content;
pub mod select_chapter;

pub enum ReadNovelMsg {
    Next,
    Prev,
    SelectChapter(usize),
    QueryChapters(String),
}

pub struct ReadNovel {
    pub loading: Loading,
}
