//! 文件下载管理模块
//!
//! 该模块提供了文件下载功能，支持断点续传和进度回调。

use super::Result;
use crate::NovelTTSError;
use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
};
use tokio::fs;
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt},
    select,
};
use tokio_util::sync::CancellationToken;

/// 缓存目录名称
pub static CACHE_DIR: &str = ".novel-tts";

/// 获取缓存目录路径
///
/// # 返回值
/// 返回Result包装的PathBuf，包含缓存目录的路径
pub fn get_cache_dir() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .map(|home| home.join(CACHE_DIR))
        .ok_or_else(|| anyhow::anyhow!("No home directory found"))?)
}

/// 从URL下载文件
///
/// # 参数
/// * `url` - 下载地址
/// * `dest` - 目标文件路径
/// * `on_progress` - 进度回调函数
///
/// # 返回值
/// 返回Result，下载成功返回Ok，失败返回Err
pub async fn download_from_url<F>(url: &str, dest: &PathBuf, mut on_progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).await?;
    }

    let path = format!("{}.download", dest.display());

    let (mut downloaded, mut file) = if let Ok(metadata) = std::fs::metadata(&path) {
        let mut file = fs::File::options().append(true).open(&path).await?;
        file.seek(SeekFrom::Start(metadata.len())).await?;
        (metadata.len(), file)
    } else {
        (0, fs::File::create(&path).await?)
    };

    let client = reqwest::Client::new();

    let mut client = client.get(url);
    if downloaded > 0 {
        client = client.header(reqwest::header::RANGE, format!("bytes={}-", downloaded));
    }

    let mut res = client.send().await?.error_for_status()?;

    let content_length = res.content_length().unwrap_or(0);

    on_progress(downloaded, content_length);

    while let Some(data) = res.chunk().await? {
        file.write_all(&data).await?;
        downloaded += data.len() as u64;
        on_progress(downloaded, content_length);
    }

    if downloaded != content_length {
        return Err(anyhow::anyhow!("Download failed").into());
    }

    fs::rename(path, dest).await?;
    Ok(())
}

/// 下载信息结构体
///
/// 包含下载任务的相关信息，如文件路径、URL和取消令牌
#[derive(Debug, Clone)]
pub struct Download {
    /// 文件路径
    pub path: PathBuf,
    /// 下载URL
    pub url: String,
    /// 取消令牌
    pub token: CancellationToken,
}

impl Download {
    /// 创建新的下载任务
    ///
    /// # 参数
    /// * `path` - 文件保存路径
    /// * `url` - 下载地址
    ///
    /// # 返回值
    /// 返回新的Download实例
    pub fn new<P: AsRef<Path>>(path: P, url: &str) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            token: CancellationToken::new(),
            url: url.to_string(),
        }
    }

    /// 检查文件是否已下载
    ///
    /// # 返回值
    /// 如果文件已存在返回true，否则返回false
    pub fn is_downloaded(&self) -> bool {
        self.path.exists()
    }

    /// 取消下载任务
    pub fn cancel_download(&self) {
        self.token.cancel();
    }

    /// 启动下载任务（同步方式）
    ///
    /// # 参数
    /// * `on_progress` - 进度回调函数
    /// * `on_error` - 错误回调函数
    ///
    /// # 返回值
    /// 返回Result，启动成功返回Ok，失败返回Err
    pub fn download<F, E>(&mut self, on_progress: F, mut on_error: E)
    where
        F: FnMut(u64, u64) + Send + 'static,
        E: FnMut(NovelTTSError) + Send + 'static,
    {
        let path = self.path.clone();
        let cancel_token = CancellationToken::new();
        self.token = cancel_token.clone();
        let url = self.url.clone();

        tokio::spawn(async move {
            select! {
                _ = cancel_token.cancelled() => {
                    on_error(NovelTTSError::Cancel("download".into()));
                }
                res = download_from_url(&url, &path, on_progress) =>{
                    if let Err(e) = res {
                        on_error(e);
                    }
                }
            }
        });
    }

    /// 启动下载任务（异步方式）
    ///
    /// # 参数
    /// * `on_progress` - 进度回调函数
    ///
    /// # 返回值
    /// 返回Result，下载成功返回Ok，失败返回Err
    pub async fn async_download<F>(&mut self, on_progress: F) -> Result<()>
    where
        F: FnMut(u64, u64) + Send + 'static,
    {
        let cancel_token = CancellationToken::new();
        self.token = cancel_token.clone();
        select! {
            _ = self.token.cancelled() => {
                Ok(())
            }
            res = download_from_url(&self.url, &self.path, on_progress) =>{
                res
            }
        }
    }
}
