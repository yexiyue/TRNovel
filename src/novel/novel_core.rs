use crate::{Result, history::HistoryItem};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

/// 卷标记（分卷元数据）。
///
/// 卷以「平行元数据」形式存在：扁平章节列表与导航语义完全不变，
/// 卷只记录标题及其首章在扁平章节列表中的索引，供目录分组展示使用。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VolumeMarker {
    /// 卷标题，例如 "第一卷 魔性不改"。
    pub title: String,
    /// 该卷的第一章在扁平章节列表中的索引。
    pub first_chapter_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NovelChapters<T> {
    pub current_chapter: usize,
    pub line_percent: f64,
    pub chapters: Option<Vec<T>>,
    /// 分卷元数据，空表示该小说无分卷（向后兼容旧缓存）。
    #[serde(default)]
    pub volumes: Vec<VolumeMarker>,
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
            volumes: Vec::new(),
        }
    }
}

pub trait Novel: Deref<Target = NovelChapters<Self::Chapter>> + DerefMut + Sized + Clone {
    type Chapter: Sync + Send + Clone;
    type Args: Sync + Send + Clone;

    fn init(args: Self::Args) -> impl Future<Output = Result<Self>> + Send;

    fn set_chapters(&mut self, chapters: &[Self::Chapter]) {
        self.chapters = Some(chapters.to_vec());
    }

    /// 设置分卷元数据。
    fn set_volumes(&mut self, volumes: Vec<VolumeMarker>) {
        self.volumes = volumes;
    }

    /// 获取分卷元数据（无分卷时为空切片）。
    fn get_volumes(&self) -> &[VolumeMarker] {
        &self.volumes
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

    fn get_chapters_names(&self) -> Result<Vec<(String, usize)>>;

    fn get_content(&self) -> impl Future<Output = Result<String>> + Send;

    /// 请求目录：一次扫描同时产出扁平章节列表与分卷元数据。
    ///
    /// 返回 `(chapters, volumes)`。无分卷的来源返回空的 volumes。
    fn request_toc(
        &self,
    ) -> impl Future<Output = Result<(Vec<Self::Chapter>, Vec<VolumeMarker>)>> + Send;

    fn get_current_chapter_name(&self) -> Result<String>;

    fn to_history_item(&self) -> Result<HistoryItem>;

    fn get_id(&self) -> String;
}
