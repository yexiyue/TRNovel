use super::{Novel, NovelChapters};
use crate::cache::LocalNovelCache;
use crate::errors::Result;
use crate::history::HistoryItem;
use anyhow::anyhow;
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, BufReader};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct LocalNovel {
    pub file: Arc<Mutex<File>>,
    pub novel_chapters: NovelChapters<(String, usize)>,
    pub encoding: &'static encoding_rs::Encoding,
    pub path: PathBuf,
}

unsafe impl Send for LocalNovel {}
unsafe impl Sync for LocalNovel {}

impl Deref for LocalNovel {
    type Target = NovelChapters<(String, usize)>;
    fn deref(&self) -> &Self::Target {
        &self.novel_chapters
    }
}

impl DerefMut for LocalNovel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.novel_chapters
    }
}

impl LocalNovel {
    async fn from_cache(value: LocalNovelCache) -> Result<Self> {
        let file = File::open(&value.path).await?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            novel_chapters: NovelChapters {
                chapters: Some(value.chapters),
                current_chapter: value.current_chapter,
                line_percent: value.line_percent,
            },
            encoding: value.encoding,
            path: value.path,
        })
    }

    pub async fn from_path<T: AsRef<Path>>(path: T) -> Result<Self> {
        let path = path.as_ref().to_path_buf().canonicalize()?;
        match LocalNovelCache::try_from(path.as_path()) {
            Ok(cache) => Self::from_cache(cache).await,
            Err(_) => Self::new(path).await,
        }
    }

    pub async fn new<T>(path: T) -> Result<Self>
    where
        T: AsRef<Path>,
    {
        let path = path.as_ref().to_path_buf().canonicalize()?;

        let file = File::open(&path).await?;
        let encoding = Self::get_file_encoding(file).await?;

        let file = File::open(&path).await?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            novel_chapters: NovelChapters::new(),
            encoding,
            path,
        })
    }

    async fn get_file_encoding(mut file: File) -> std::io::Result<&'static encoding_rs::Encoding> {
        let mut buffer = vec![];

        file.read_to_end(&mut buffer).await?;
        if let (_, encoding, false) = encoding_rs::UTF_8.decode(&buffer) {
            return Ok(encoding);
        }

        if let (_, encoding, false) = encoding_rs::GBK.decode(&buffer) {
            return Ok(encoding);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unsupported encoding",
        ))
    }
}

impl Novel for LocalNovel {
    type Chapter = (String, usize);
    type Args = PathBuf;

    async fn init(args: Self::Args) -> Result<Self> {
        Self::from_path(args).await
    }

    async fn request_chapters(&self) -> Result<Vec<Self::Chapter>> {
        let path = self.path.clone();
        let encoding = self.encoding;

        let file = File::open(path).await?;

        let mut buf_reader = BufReader::new(file);
        let regexp = regex::Regex::new(r"第.+章").unwrap();
        let mut chapter_offset = Vec::new();
        let mut offset = 0;

        let mut line = vec![];

        while let Ok(chunk_size) = buf_reader.read_until(b'\n', &mut line).await {
            if chunk_size == 0 {
                break;
            }
            let (new_line, _, _) = encoding.decode(&line);

            if regexp.is_match(&new_line) {
                chapter_offset.push((new_line.trim().to_string(), offset));
            }
            line.clear();
            offset += chunk_size;
        }
        Ok(chapter_offset)
    }

    async fn get_content(&self) -> Result<String> {
        let start = if self.current_chapter == 0 {
            0
        } else {
            let (_, start) = self.get_chapters_result()?[self.current_chapter];
            start.to_owned()
        };
        let is_last = self.get_chapters_result()?.is_empty()
            || self.current_chapter + 1 >= self.get_chapters_result()?.len();

        let end = self.chapters.as_ref().and_then(|chapters| {
            chapters
                .get(self.current_chapter + 1)
                .map(|chapter| chapter.1)
        });

        let file = self.file.clone();
        let encoding = self.encoding;
        let mut file = file.lock().await;

        let end = if is_last {
            file.metadata().await?.len() as usize
        } else {
            end.ok_or(anyhow!("找不到下一章"))?
        };

        let mut buffer = vec![0; end - start];
        file.seek(SeekFrom::Start(start as u64)).await?;
        file.read_exact(&mut buffer).await?;

        let (str, _, has_error) = encoding.decode(&buffer);
        if has_error {
            return Err(anyhow::anyhow!("解码错误").into());
        }
        Ok(str.to_string())
    }

    fn get_current_chapter_name(&self) -> Result<String> {
        self.get_current_chapter().map(|chapter| chapter.0)
    }

    fn get_chapters_names(&self) -> Result<Vec<(String, usize)>> {
        Ok(self
            .get_chapters_result()?
            .iter()
            .enumerate()
            .map(|(index, item)| (item.0.clone(), index))
            .collect())
    }

    fn to_history_item(&self) -> Result<HistoryItem> {
        let local_novel_cache = LocalNovelCache::try_from(self)?;
        local_novel_cache.save()?;
        Ok(local_novel_cache.into())
    }

    fn get_id(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}
