use super::{Novel, NovelChapters};
use crate::{
    book_source::BookSourceCache, cache::NetworkNovelCache, errors::Errors, history::HistoryItem,
    Result,
};
use anyhow::anyhow;
use async_trait::async_trait;
use parse_book_source::{BookInfo, BookListItem, Chapter, JsonSource};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct NetworkNovel {
    pub book_list_item: BookListItem,
    pub book_source: Arc<Mutex<JsonSource>>,
    pub book_info: Option<BookInfo>,
    pub novel_chapters: NovelChapters<Chapter>,
}

impl NetworkNovel {
    pub async fn from_url(url: &str, book_sources: Arc<Mutex<BookSourceCache>>) -> Result<Self> {
        let network_cache = NetworkNovelCache::try_from(url)?;
        let json_source = book_sources
            .lock()
            .await
            .find_book_source(
                &network_cache.book_source_url,
                &network_cache.book_source_name,
            )
            .cloned()
            .ok_or(anyhow!("book source not found"))?;

        let novel = NetworkNovel {
            book_list_item: network_cache.book_list_item,
            book_source: Arc::new(Mutex::new(JsonSource::try_from(json_source)?)),
            book_info: None,
            novel_chapters: NovelChapters {
                current_chapter: network_cache.current_chapter,
                line_percent: network_cache.line_percent,
                chapters: None,
            },
        };
        Ok(novel)
    }

    pub fn new(book_list_item: BookListItem, book_source: Arc<Mutex<JsonSource>>) -> Self {
        Self {
            book_list_item,
            book_source,
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

#[async_trait]
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

    fn request_chapters<T: FnMut(Result<Vec<Self::Chapter>>) + Send + 'static>(
        &self,
        mut callback: T,
    ) -> Result<()> {
        let book_source = self.book_source.clone();
        let book_info = self.book_info.clone().ok_or("book_info is none")?;

        tokio::spawn(async move {
            let res = book_source
                .lock()
                .await
                .chapter_list(&book_info)
                .await
                .map(|chapters| chapters.chapter_list)
                .map_err(Errors::from);

            callback(res);
        });
        Ok(())
    }

    fn get_chapters_names(&self) -> Result<Vec<String>> {
        Ok(self
            .get_chapters_result()?
            .iter()
            .map(|item| item.chapter_name.clone())
            .collect())
    }

    fn get_content<T: FnMut(Result<String>) + Send + 'static>(
        &mut self,
        mut callback: T,
    ) -> Result<()> {
        let book_source = self.book_source.clone();
        let chapter = self.get_current_chapter()?;

        tokio::spawn(async move {
            let res = book_source
                .lock()
                .await
                .chapter_content(&chapter)
                .await
                .map_err(Errors::from);

            callback(res);
        });
        Ok(())
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
