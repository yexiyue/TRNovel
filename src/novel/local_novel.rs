use crate::errors::Result;
use crate::history::HistoryItem;
use crate::{cache::LocalNovelCache, errors::Errors};

use std::ops::{Deref, DerefMut};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

use super::{Novel, NovelChapters};

#[derive(Debug, Clone)]
pub struct LocalNovel {
    pub file: Arc<Mutex<File>>,
    pub novel_chapters: NovelChapters<(String, usize)>,
    pub encoding: &'static encoding_rs::Encoding,
    pub path: PathBuf,
}

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
    pub fn from_path<T: AsRef<Path>>(path: T) -> Result<Self> {
        let path = path.as_ref().to_path_buf().canonicalize()?;
        match LocalNovelCache::try_from(path.as_path()) {
            Ok(cache) => Self::try_from(cache),
            Err(_) => Self::new(path),
        }
    }

    pub fn new<T>(path: T) -> Result<Self>
    where
        T: AsRef<Path>,
    {
        let path = path.as_ref().to_path_buf().canonicalize()?;

        let file = File::open(&path)?;
        let encoding = Self::get_file_encoding(file)?;

        let file = File::open(&path)?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            novel_chapters: NovelChapters::new(),
            encoding,
            path,
        })
    }

    fn get_file_encoding(mut file: File) -> std::io::Result<&'static encoding_rs::Encoding> {
        let mut buffer = vec![];

        file.read_to_end(&mut buffer)?;
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

    fn request_chapters<T: FnMut(Result<Vec<Self::Chapter>>) + Send + 'static>(
        &self,
        mut callback: T,
    ) -> Result<()> {
        let path = self.path.clone();
        let encoding = self.encoding;

        tokio::spawn(async move {
            let res = async {
                let file = File::open(path)?;

                let mut buf_reader = BufReader::new(file);
                let regexp = regex::Regex::new(r"第.+章").unwrap();
                let mut chapter_offset = Vec::new();
                let mut offset = 0;

                let mut line = vec![];

                while let Ok(chunk_size) = buf_reader.read_until(b'\n', &mut line) {
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
            .await;

            callback(res);
        });
        Ok(())
    }

    fn get_content<T: FnMut(Result<String>) + Send + 'static>(
        &mut self,
        mut callback: T,
    ) -> Result<()> {
        let start = if self.current_chapter == 0 {
            0
        } else {
            let (_, start) = self.get_chapters_result()?[self.current_chapter];
            start.to_owned()
        };

        let end = if self.get_chapters_result()?.is_empty()
            || self.current_chapter + 1 >= self.get_chapters_result()?.len()
        {
            self.file.try_lock()?.metadata()?.len() as usize
        } else {
            let (_, end) = self.get_chapters_result()?[self.current_chapter + 1];
            end.to_owned()
        };

        let file = self.file.clone();
        let encoding = self.encoding;
        tokio::spawn(async move {
            let res = async {
                let mut buffer = vec![0; end - start];
                file.lock().await.seek(SeekFrom::Start(start as u64))?;
                file.lock().await.read_exact(&mut buffer)?;

                let (str, _, has_error) = encoding.decode(&buffer);
                if has_error {
                    return Err(anyhow::anyhow!("解码错误").into());
                }
                Ok::<String, Errors>(str.to_string())
            }
            .await;

            callback(res);
        });
        Ok(())
    }

    fn get_current_chapter_name(&self) -> Result<String> {
        self.get_current_chapter().map(|chapter| chapter.0)
    }

    fn get_chapters_names(&self) -> Result<Vec<String>> {
        Ok(self
            .get_chapters_result()?
            .iter()
            .map(|item| item.0.clone())
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

impl TryFrom<LocalNovelCache> for LocalNovel {
    type Error = Errors;
    fn try_from(value: LocalNovelCache) -> Result<Self> {
        let file = File::open(&value.path)?;
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
}
