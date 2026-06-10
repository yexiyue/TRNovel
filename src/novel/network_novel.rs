use super::{Novel, NovelChapters, VolumeMarker};
use crate::{
    Result, book_source::BookSourceCache, cache::NetworkNovelCache, errors::Errors,
    history::HistoryItem,
};
use anyhow::anyhow;
use parse_book_source::{BookInfo, BookListItem, Chapter, Engine};
use std::ops::{Deref, DerefMut};

/// 网络小说:持有 v2 `Engine`(廉价 Clone、内部 Arc,无需外层 Mutex)。
#[derive(Debug, Clone)]
pub struct NetworkNovel {
    pub book_list_item: BookListItem,
    pub engine: Engine,
    pub book_info: Option<BookInfo>,
    pub novel_chapters: NovelChapters<Chapter>,
}

impl NetworkNovel {
    pub fn from_url(url: &str, book_sources: &BookSourceCache) -> Result<Self> {
        let network_cache = NetworkNovelCache::try_from(url)?;
        let source = book_sources
            .find_book_source(
                &network_cache.book_source_url,
                &network_cache.book_source_name,
            )
            .cloned()
            .ok_or(anyhow!("book source not found"))?;

        Ok(NetworkNovel {
            book_list_item: network_cache.book_list_item,
            // 注回书籍级捕获变量(scope=book 多步 vars),使续读时目录/正文沿用上次捕获的 token。
            engine: crate::browser_assist::build_engine(source)?
                .with_book_vars(network_cache.book_vars),
            book_info: None,
            novel_chapters: NovelChapters {
                current_chapter: network_cache.current_chapter,
                line_percent: network_cache.line_percent,
                chapters: None,
                volumes: Vec::new(),
            },
        })
    }

    pub fn new(book_list_item: BookListItem, engine: Engine) -> Self {
        Self {
            book_list_item,
            engine,
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
        self.get_current_chapter().map(|chapter| chapter.title)
    }

    async fn request_toc(&self) -> Result<(Vec<Self::Chapter>, Vec<VolumeMarker>)> {
        let book_info = self.book_info.as_ref().ok_or("book_info is none")?;
        // 引擎已把卷条目拆出(目录 isVolume),直接得到扁平章节 + 卷元数据。
        let toc = self.engine.toc(&book_info.toc_url).await?;
        let volumes = toc
            .volumes
            .into_iter()
            .map(|v| VolumeMarker {
                title: v.title,
                first_chapter_index: v.first_chapter_index,
            })
            .collect();
        Ok((toc.chapters, volumes))
    }

    fn get_chapters_names(&self) -> Result<Vec<(String, usize)>> {
        Ok(self
            .get_chapters_result()?
            .iter()
            .enumerate()
            .map(|(index, item)| (item.title.clone(), index))
            .collect())
    }

    async fn get_content(&self) -> Result<String> {
        let chapter = self.get_current_chapter()?;
        self.engine
            .content(&chapter.url)
            .await
            .map_err(Errors::from)
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
