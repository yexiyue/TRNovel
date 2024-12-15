use crate::{errors::Errors, Result};
use anyhow::anyhow;
use parse_book_source::{BookInfo, BookListItem, Chapter, JsonSource};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::Novel;

#[derive(Debug, Clone)]
pub struct NetworkNovel {
    pub book_list_item: BookListItem,
    pub book_source: Arc<Mutex<JsonSource>>,
    pub current_chapter: usize,
    pub line_percent: f64,
    pub book_info: Option<BookInfo>,
    pub chapters: Option<Vec<Chapter>>,
}

impl NetworkNovel {
    pub fn new(book_list_item: BookListItem, book_source: Arc<Mutex<JsonSource>>) -> Self {
        Self {
            book_list_item,
            book_source,
            current_chapter: 0,
            line_percent: 0.0,
            book_info: None,
            chapters: None,
        }
    }

    pub fn set_book_info(&mut self, book_info: &BookInfo) {
        self.book_info = Some(book_info.clone());
    }
}

impl Novel for NetworkNovel {
    type Chapter = Chapter;

    fn set_chapters(&mut self, chapters: &[Self::Chapter]) {
        self.chapters = Some(chapters.to_vec());
    }

    fn get_current_chapter(&self) -> Result<Chapter> {
        Ok(self
            .chapters
            .as_ref()
            .ok_or(anyhow!("章节列表为空"))?
            .get(self.current_chapter)
            .ok_or(anyhow!("当前章节不存在"))?
            .clone())
    }

    fn get_current_chapter_name(&self) -> Result<String> {
        self.get_current_chapter()
            .map(|chapter| chapter.chapter_name)
    }

    fn chapter_percent(&self) -> Result<f64> {
        Ok(self.current_chapter as f64 / self.get_chapters_result()?.len() as f64 * 100.0)
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

    fn get_chapters_result(&self) -> Result<&Vec<Chapter>> {
        self.chapters.as_ref().ok_or(anyhow!("没有章节信息").into())
    }

    fn get_chapters(&self) -> Option<&Vec<Self::Chapter>> {
        self.chapters.as_ref()
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

    fn next_chapter(&mut self) -> Result<()> {
        if self.current_chapter + 1 >= self.get_chapters_result()?.len() {
            Err("已经是最后一章了".into())
        } else {
            self.current_chapter += 1;
            Ok(())
        }
    }

    fn set_chapter(&mut self, chapter: usize) -> Result<()> {
        if chapter >= self.get_chapters_result()?.len() {
            Err("章节不存在".into())
        } else {
            if self.current_chapter != chapter {
                self.current_chapter = chapter;
            }
            Ok(())
        }
    }

    fn prev_chapter(&mut self) -> Result<()> {
        if self.current_chapter == 0 {
            Err("已经是第一章了".into())
        } else {
            self.current_chapter -= 1;
            Ok(())
        }
    }
}
