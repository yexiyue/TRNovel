use serde::{Deserialize, Serialize};

pub type BookList = Vec<BookListItem>;
pub type ChapterList = Vec<Chapter>;
pub type ExploreList = Vec<ExploreItem>;

#[derive(Default, Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ExploreItem {
    pub title: String,
    pub url: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct BookListItem {
    pub book_url: String,

    #[serde(flatten)]
    pub book_info: BookInfo,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct BookInfo {
    pub author: String,
    pub cover_url: String,
    pub intro: String,
    pub kind: String,
    pub last_chapter: String,
    pub name: String,
    pub toc_url: String,
    pub word_count: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct Chapter {
    pub chapter_name: String,
    pub chapter_url: String,
}
