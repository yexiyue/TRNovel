use std::{
    path::Path,
    sync::{Arc, LazyLock},
};

use futures::{stream::FuturesOrdered, StreamExt};
use regex::Regex;
use tokio::{io::AsyncWriteExt, sync::Mutex};
use tokio_util::sync::CancellationToken;

use crate::{error, BookInfo, BookSourceParser, Chapter, Result};

static CHAPTER_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"第.+章").unwrap());

#[derive(Clone, Debug)]
pub struct Downloader {
    pub book_source_parser: Arc<Mutex<BookSourceParser>>,
    pub book_info: BookInfo,
    pub downloaded_chapter: usize,
    pub cancel_token: CancellationToken,
}

impl Downloader {
    pub fn new(
        book_source_parser: &BookSourceParser,
        book_info: BookInfo,
        downloaded_chapter: usize,
    ) -> Self {
        Self {
            book_source_parser: Arc::new(Mutex::new(book_source_parser.clone())),
            book_info,
            downloaded_chapter,
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    // https://docs.rs/futures/0.3.31/futures/prelude/stream/struct.FuturesOrdered.html
    pub async fn download<T: AsRef<Path>>(
        &mut self,
        file_name: T,
        on_progress: impl FnMut(&Chapter, usize, usize),
    ) -> Result<()> {
        let chapters = self
            .book_source_parser
            .lock()
            .await
            .get_chapters(&self.book_info.toc_url)
            .await?;

        self.download_with_chapters(file_name, chapters, on_progress)
            .await
    }

    pub async fn download_with_chapters<T: AsRef<Path>>(
        &mut self,
        file_name: T,
        chapters: Vec<Chapter>,
        mut on_progress: impl FnMut(&Chapter, usize, usize),
    ) -> Result<()> {
        let mut futures_ordered = FuturesOrdered::new();

        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(file_name)
            .await?;

        let len = chapters.len();

        for chapter in &chapters[self.downloaded_chapter..len] {
            let book_source_parser = self.book_source_parser.clone();
            let cancel_token = self.cancel_token.clone();

            futures_ordered.push_back(async move {
                let mut book_source_parser = book_source_parser.lock().await;
                tokio::select! {
                    _ = cancel_token.cancelled()=>{
                        return Err::<_, error::ParseError>(error::ParseError::Canceled);
                    }
                    content = book_source_parser.get_content(&chapter.chapter_url)=>{
                        return Ok::<_, error::ParseError>((chapter, content?));
                    }
                }
            });
        }

        while let Some(result) = futures_ordered.next().await {
            let (chapter, content) = result?;
            self.downloaded_chapter += 1;
            on_progress(chapter, self.downloaded_chapter, len);
            if !CHAPTER_REGEX.is_match(&content) {
                file.write_all(format!("\n\n{}\n\n", chapter.chapter_name).as_bytes())
                    .await?;
            }
            file.write_all(content.as_bytes()).await?;
        }

        Ok(())
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        self.cancel();
    }
}
