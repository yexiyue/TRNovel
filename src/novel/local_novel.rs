use crate::cache::LocalNovelCache;
use crate::errors::Result;
use crate::history::History;
use encoding_rs::Encoding;

use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

impl From<&mut LocalNovel> for LocalNovelCache {
    fn from(value: &mut LocalNovel) -> Self {
        Self {
            chapters: value.chapters.clone(),
            encoding: value.encoding,
            current_chapter: value.current_chapter,
            path: value.path.clone(),
            line_percent: value.line_percent,
        }
    }
}

#[derive(Debug)]
pub struct LocalNovel {
    pub file: File,
    pub chapters: Vec<(String, usize)>,
    pub encoding: &'static encoding_rs::Encoding,
    pub current_chapter: usize,
    pub line_percent: f64,
    pub path: PathBuf,
}

impl LocalNovel {
    pub fn from(value: LocalNovelCache) -> Result<Self> {
        let file = File::open(&value.path)?;
        Ok(Self {
            file,
            chapters: value.chapters,
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
        let mut file = File::open(&path)?;
        let chapter_offset = Self::get_chapter_offset(&mut file, encoding)?;

        Ok(Self {
            file,
            chapters: chapter_offset,
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

    fn get_chapter_offset(
        file: &mut File,
        encoding: &'static Encoding,
    ) -> Result<Vec<(String, usize)>> {
        let mut buf_reader = BufReader::new(file);
        let regexp = regex::Regex::new(r"第.+章").unwrap();
        let mut chapter_offset = Vec::new();
        let mut offset = 0;

        let mut line = vec![];

        let mut first_line = String::new();
        while let Ok(chunk_size) = buf_reader.read_until(b'\n', &mut line) {
            if chunk_size == 0 {
                break;
            }
            let (new_line, _, _) = encoding.decode(&line);
            if first_line.is_empty() {
                first_line = new_line.to_string();
            }
            if regexp.is_match(&new_line) {
                chapter_offset.push((new_line.trim().to_string(), offset));
            }
            line.clear();
            offset += chunk_size;
        }

        Ok(chapter_offset)
    }

    pub fn get_content(&mut self) -> Result<String> {
        let start = if self.current_chapter == 0 {
            0
        } else {
            let (_, start) = &self.chapters[self.current_chapter];
            start.to_owned()
        };

        let end = if self.chapters.is_empty() || self.current_chapter + 1 >= self.chapters.len() {
            self.file.metadata()?.len() as usize
        } else {
            let (_, end) = &self.chapters[self.current_chapter + 1];
            end.to_owned()
        };

        let mut buffer = vec![0; end - start];
        self.file.seek(SeekFrom::Start(start as u64))?;
        self.file.read_exact(&mut buffer)?;

        let (str, _, has_error) = self.encoding.decode(&buffer);
        if has_error {
            return Err(anyhow::anyhow!("解码错误").into());
        }

        Ok(str.to_string())
    }

    pub fn next_chapter(&mut self) -> Result<()> {
        if self.current_chapter + 1 >= self.chapters.len() {
            Err("已经是最后一章".into())
        } else {
            self.current_chapter += 1;
            Ok(())
        }
    }

    pub fn set_chapter(&mut self, chapter: usize) -> Result<()> {
        if chapter >= self.chapters.len() {
            Err("章节不存在".into())
        } else {
            if self.current_chapter != chapter {
                self.current_chapter = chapter;
            }
            Ok(())
        }
    }

    pub fn prev_chapter(&mut self) -> Result<()> {
        if self.current_chapter == 0 {
            Err("已经是第一章".into())
        } else {
            self.current_chapter -= 1;
            Ok(())
        }
    }
}

/// 当小说被释放时，将小说缓存到历史记录中
impl Drop for LocalNovel {
    fn drop(&mut self) {
        let txt_novel_cache: LocalNovelCache = self.into();
        txt_novel_cache.save().expect("小说缓存失败");
        let mut histories = History::load().expect("历史记录加载失败");
        histories.add(txt_novel_cache.path.clone(), txt_novel_cache.into());
        histories.save().expect("历史记录保存失败");
    }
}
