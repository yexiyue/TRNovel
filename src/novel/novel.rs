use crate::{history::HistoryItem, Result};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NovelChapters<T> {
    pub current_chapter: usize,
    pub line_percent: f64,
    pub chapters: Option<Vec<T>>,
}

impl<T> NovelChapters<T>
where
    T: Clone,
{
    pub fn new() -> Self {
        Self {
            current_chapter: 0,
            line_percent: 0.0,
            chapters: None,
        }
    }
}

pub trait Novel: Deref<Target = NovelChapters<Self::Chapter>> + DerefMut {
    type Chapter: Sync + Send + Clone;

    fn set_chapters(&mut self, chapters: &[Self::Chapter]) {
        self.chapters = Some(chapters.to_vec());
    }

    fn get_current_chapter(&self) -> Result<Self::Chapter> {
        Ok(self
            .chapters
            .as_ref()
            .ok_or(anyhow!("章节列表为空"))?
            .get(self.current_chapter)
            .ok_or(anyhow!("当前章节不存在"))?
            .clone())
    }

    fn chapter_percent(&self) -> Result<f64> {
        Ok(self.current_chapter as f64 / self.get_chapters_result()?.len() as f64 * 100.0)
    }

    fn get_chapters_result(&self) -> Result<&Vec<Self::Chapter>> {
        self.chapters.as_ref().ok_or(anyhow!("没有章节信息").into())
    }

    fn get_chapters(&self) -> Option<&Vec<Self::Chapter>> {
        self.chapters.as_ref()
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
    // 下面的逻辑需要根据Self::Chapter实现,上面的是通用的，可以直接使用NovelChapters的方法

    fn get_chapters_names(&self) -> Result<Vec<String>>;

    fn get_content<T: FnMut(Result<String>) + Send + 'static>(&mut self, callback: T)
        -> Result<()>;

    fn request_chapters<T: FnMut(Result<Vec<Self::Chapter>>) + Send + 'static>(
        &self,
        callback: T,
    ) -> Result<()>;

    fn get_current_chapter_name(&self) -> Result<String>;

    fn to_history_item(&self) -> Result<HistoryItem>;

    fn get_id(&self) -> String;
}
