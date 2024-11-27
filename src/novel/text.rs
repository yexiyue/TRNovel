use anyhow::Result;
use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::{
    history::History,
    utils::{get_path_md5, novel_catch_dir},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TxtNovelCache {
    pub chapter_offset: Vec<(String, usize)>,
    pub encoding: &'static encoding_rs::Encoding,
    pub current_chapter: usize,
    pub current_line: usize,
    pub content_lines: usize,
    pub path: PathBuf,
}

impl TxtNovelCache {
    pub fn from<T: AsRef<Path>>(value: T) -> Result<Self> {
        let cache_path = Self::path_to_catch(value)?;
        let file = File::open(cache_path)?;
        Ok(serde_json::from_reader(file)?)
    }

    pub fn save(&self) -> Result<()> {
        let cache_path = Self::path_to_catch(&self.path)?;
        let file = File::create(cache_path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn path_to_catch<T: AsRef<Path>>(path: T) -> Result<PathBuf> {
        Ok(PathBuf::new()
            .join(novel_catch_dir()?)
            .join(get_path_md5(path)?)
            .with_extension("json"))
    }
}

impl From<&mut TxtNovel> for TxtNovelCache {
    fn from(value: &mut TxtNovel) -> Self {
        Self {
            chapter_offset: value.chapter_offset.clone(),
            encoding: value.encoding,
            current_chapter: value.current_chapter,
            path: value.path.clone(),
            current_line: value.current_line,
            content_lines: value.content_lines,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TxtNovel {
    pub file: Arc<RwLock<File>>,
    pub chapter_offset: Vec<(String, usize)>,
    pub encoding: &'static encoding_rs::Encoding,
    pub current_chapter: usize,
    pub current_line: usize,
    pub content_lines: usize,
    pub path: PathBuf,
}

impl TxtNovel {
    pub fn from(value: TxtNovelCache) -> Result<Self> {
        let file = File::open(&value.path)?;
        Ok(Self {
            file: Arc::new(RwLock::new(file)),
            chapter_offset: value.chapter_offset,
            encoding: value.encoding,
            current_chapter: value.current_chapter,
            path: value.path,
            current_line: value.current_line,
            content_lines: value.content_lines,
        })
    }

    pub fn from_path<T: AsRef<Path>>(path: T) -> Result<Self> {
        let path = path.as_ref().to_path_buf().canonicalize()?;
        match TxtNovelCache::from(&path) {
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
            file: Arc::new(RwLock::new(file)),
            chapter_offset,
            encoding,
            current_chapter: 0,
            path,
            current_line: 0,
            content_lines: 0,
        })
    }

    fn get_file_encoding(mut file: File) -> std::io::Result<&'static encoding_rs::Encoding> {
        let mut buffer = vec![];

        file.read_to_end(&mut buffer)?;
        if let (_, encoding, false) = encoding_rs::UTF_8.decode(&buffer) {
            return Ok(&encoding);
        }

        if let (_, encoding, false) = encoding_rs::GBK.decode(&buffer) {
            return Ok(&encoding);
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

        if chapter_offset.is_empty() {
            chapter_offset = vec![(first_line, 0)];
        }

        Ok(chapter_offset)
    }

    pub fn get_content(&mut self) -> Result<String> {
        let start = if self.current_chapter == 0 {
            0
        } else {
            let (_, start) = &self.chapter_offset[self.current_chapter];
            start.to_owned()
        };

        let end = if self.current_chapter + 1 >= self.chapter_offset.len() {
            self.file.read().unwrap().metadata()?.len() as usize
        } else {
            let (_, end) = &self.chapter_offset[self.current_chapter + 1];
            end.to_owned()
        };

        let mut buffer = vec![0; end - start];
        self.file
            .write()
            .unwrap()
            .seek(SeekFrom::Start(start as u64))?;
        self.file.write().unwrap().read(&mut buffer)?;

        let (str, _, has_error) = self.encoding.decode(&buffer);
        if has_error {
            return Err(anyhow::anyhow!("解码错误"));
        }

        self.content_lines = str.lines().count();

        Ok(str.to_string())
    }

    pub fn next_chapter(&mut self) -> Result<String> {
        if self.current_chapter + 1 >= self.chapter_offset.len() {
            Err(anyhow::anyhow!("已经是最后一章"))
        } else {
            self.current_chapter += 1;
            self.current_line = 0;
            Ok(self.get_content()?)
        }
    }

    pub fn set_chapter(&mut self, chapter: usize) -> Result<()> {
        if chapter >= self.chapter_offset.len() {
            Err(anyhow::anyhow!("章节不存在"))
        } else {
            if self.current_chapter != chapter {
                self.current_chapter = chapter;
                self.current_line = 0;
            }
            Ok(())
        }
    }

    pub fn prev_chapter(&mut self) -> Result<String> {
        if self.current_chapter <= 0 {
            Err(anyhow::anyhow!("已经是第一章"))
        } else {
            self.current_chapter -= 1;
            self.current_line = 0;
            Ok(self.get_content()?)
        }
    }
}

impl Drop for TxtNovel {
    fn drop(&mut self) {
        let txt_novel_cache: TxtNovelCache = self.into();
        let mut histories = History::default().expect("历史记录加载失败");
        histories.add(txt_novel_cache.path.clone(), txt_novel_cache.clone().into());
        histories.save().expect("历史记录保存失败");
        txt_novel_cache.save().expect("小说缓存失败");
    }
}
