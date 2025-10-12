use super::{Novel, NovelChapters};
use crate::{Result, book_source::BookSourceCache, cache::NetworkNovelCache, history::HistoryItem};
use anyhow::anyhow;
use parse_book_source::{BookInfo, BookListItem, BookSourceParser, Chapter};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct NetworkNovel {
    pub book_list_item: BookListItem,
    pub book_source: Arc<Mutex<BookSourceParser>>,
    pub book_info: Option<BookInfo>,
    pub novel_chapters: NovelChapters<Chapter>,
}

impl NetworkNovel {
    pub fn from_url(url: &str, book_sources: &BookSourceCache) -> Result<Self> {
        let network_cache = NetworkNovelCache::try_from(url)?;
        let json_source = book_sources
            .find_book_source(
                &network_cache.book_source_url,
                &network_cache.book_source_name,
            )
            .cloned()
            .ok_or(anyhow!("book source not found"))?;

        let novel = NetworkNovel {
            book_list_item: network_cache.book_list_item,
            book_source: Arc::new(Mutex::new(BookSourceParser::try_from(json_source)?)),
            book_info: None,
            novel_chapters: NovelChapters {
                current_chapter: network_cache.current_chapter,
                line_percent: network_cache.line_percent,
                chapters: None,
            },
        };
        Ok(novel)
    }

    pub fn new(book_list_item: BookListItem, book_source: BookSourceParser) -> Self {
        Self {
            book_list_item,
            book_source: Arc::new(Mutex::new(book_source)),
            book_info: None,
            novel_chapters: NovelChapters::new(),
        }
    }

    pub fn set_book_info(&mut self, book_info: &BookInfo) {
        self.book_info = Some(book_info.clone());
    }
}

impl Deref for NetworkNovel {
    type Target = NovelChapters<Chapter>;
    fn deref(&self) -> &Self::Target {
        &self.novel_chapters
    }
}

impl DerefMut for NetworkNovel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.novel_chapters
    }
}

impl Novel for NetworkNovel {
    type Chapter = Chapter;
    type Args = Self;

    async fn init(args: Self::Args) -> Result<Self> {
        Ok(args)
    }

    fn get_current_chapter_name(&self) -> Result<String> {
        self.get_current_chapter()
            .map(|chapter| chapter.chapter_name)
    }

    async fn request_chapters(&self) -> Result<Vec<Self::Chapter>> {
        let book_source = self.book_source.clone();
        let book_info = self.book_info.clone().ok_or("book_info is none")?;

        Ok(book_source
            .lock()
            .await
            .get_chapters(&book_info.toc_url)
            .await?)
    }

    fn get_chapters_names(&self) -> Result<Vec<(String, usize)>> {
        Ok(self
            .get_chapters_result()?
            .iter()
            .enumerate()
            .map(|(index, item)| (item.chapter_name.clone(), index))
            .collect())
    }

    async fn get_content(&self) -> Result<String> {
        let book_source = self.book_source.clone();
        let chapter = self.get_current_chapter()?;

        Ok(book_source
            .lock()
            .await
            .get_content(&chapter.chapter_url)
            .await?)
    }

    fn to_history_item(&self) -> Result<HistoryItem> {
        let network_novel_cache = NetworkNovelCache::try_from(self)?;
        network_novel_cache.save()?;
        Ok(network_novel_cache.into())
    }

    fn get_id(&self) -> String {
        self.book_list_item.book_url.clone()
    }
}
