use anyhow::anyhow;

use crate::errors::Result;
use crate::history::History;
use crate::{cache::LocalNovelCache, errors::Errors};

use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

use super::Novel;

impl From<&mut NewLocalNovel> for LocalNovelCache {
    fn from(value: &mut NewLocalNovel) -> Self {
        Self {
            chapters: value.chapters.clone().unwrap(),
            encoding: value.encoding,
            current_chapter: value.current_chapter,
            path: value.path.clone(),
            line_percent: value.line_percent,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewLocalNovel {
    pub file: Arc<Mutex<File>>,
    pub chapters: Option<Vec<(String, usize)>>,
    pub encoding: &'static encoding_rs::Encoding,
    pub current_chapter: usize,
    pub line_percent: f64,
    pub path: PathBuf,
}

impl NewLocalNovel {
    pub fn from(value: LocalNovelCache) -> Result<Self> {
        let file = File::open(&value.path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            chapters: Some(value.chapters),
            encoding: value.encoding,
            current_chapter: value.current_chapter,
            path: value.path,
            line_percent: value.line_percent,
        })
    }

    pub fn from_path<T: AsRef<Path>>(path: T) -> Result<Self> {
        let path = path.as_ref().to_path_buf().canonicalize()?;
        match LocalNovelCache::from(&path) {
            Ok(cache) => Self::from(cache),
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
            chapters: None,
            encoding,
            current_chapter: 0,
            path,
            line_percent: 0.0,
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

impl Novel for NewLocalNovel {
    type Chapter = (String, usize);
    fn get_chapters_result(&self) -> Result<&Vec<(String, usize)>> {
        self.chapters.as_ref().ok_or(anyhow!("没有章节信息").into())
    }

    fn get_chapters(&self) -> Option<&Vec<Self::Chapter>> {
        self.chapters.as_ref()
    }
    fn chapter_percent(&self) -> Result<f64> {
        Ok(self.current_chapter as f64 / self.get_chapters_result()?.len() as f64 * 100.0)
    }

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
}

/// 当小说被释放时，将小说缓存到历史记录中
impl Drop for NewLocalNovel {
    fn drop(&mut self) {
        let txt_novel_cache: LocalNovelCache = self.into();
        txt_novel_cache.save().expect("小说缓存失败");
        let mut histories = History::load().expect("历史记录加载失败");
        histories.add(txt_novel_cache.path.clone(), txt_novel_cache.into());
        histories.save().expect("历史记录保存失败");
    }
}
