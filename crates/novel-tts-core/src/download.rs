//! 文件下载管理模块
//!
//! 该模块提供了文件下载功能，支持断点续传和进度回调。

use super::Result;
use crate::ResourceError;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::{io::AsyncWriteExt, select};
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

    let _lock = crate::storage::lock(&dest.with_extension("download.lock"))?;
    if dest.exists() {
        return Ok(());
    }
    let path = format!("{}.download", dest.display());

    let (mut downloaded, mut file) = if let Ok(metadata) = std::fs::metadata(&path) {
        let file = fs::File::options().append(true).open(&path).await?;
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

    if downloaded > 0 {
        if res.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            let prefix = format!("bytes {downloaded}-");
            if !res
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with(&prefix))
            {
                return Err(anyhow::anyhow!("invalid resume Content-Range").into());
            }
        } else {
            // Servers may ignore Range. Restart, never append a complete file
            // to the existing partial download.
            file = fs::File::create(&path).await?;
            downloaded = 0;
        }
    }
    let content_length = res.content_length().map(|length| length + downloaded);

    on_progress(downloaded, content_length.unwrap_or(0));

    while let Some(data) = res.chunk().await? {
        file.write_all(&data).await?;
        downloaded += data.len() as u64;
        on_progress(downloaded, content_length.unwrap_or(0));
    }

    if content_length.is_some_and(|length| downloaded != length) {
        return Err(anyhow::anyhow!("Download failed").into());
    }

    file.sync_all().await?;
    drop(file);
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
        E: FnMut(ResourceError) + Send + 'static,
    {
        let path = self.path.clone();
        let cancel_token = CancellationToken::new();
        self.token = cancel_token.clone();
        let url = self.url.clone();

        tokio::spawn(async move {
            select! {
                _ = cancel_token.cancelled() => {
                    on_error(ResourceError::Cancel("download".into()));
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
        select! {
            _ = self.token.cancelled() => {
                Err(ResourceError::Cancel("download".into()))
            }
            res = download_from_url(&self.url, &self.path, on_progress) =>{
                if self.token.is_cancelled() {Err(ResourceError::Cancel("download".into()))} else {res}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn server(response: &'static str, range: bool) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/resource", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            assert_eq!(
                String::from_utf8_lossy(&request)
                    .to_lowercase()
                    .contains("range: bytes=3-"),
                range
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (url, task)
    }

    #[tokio::test]
    async fn resumes_partial_content_and_restarts_when_range_is_ignored() {
        for response in [
            "HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 3-5/6\r\nConnection: close\r\n\r\ndef",
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabcdef",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("model");
            std::fs::write(format!("{}.download", path.display()), b"abc").unwrap();
            let (url, task) = server(response, true);
            download_from_url(&url, &path, |_, _| {}).await.unwrap();
            task.join().unwrap();
            assert_eq!(std::fs::read(path).unwrap(), b"abcdef");
        }
    }

    #[tokio::test]
    async fn cancellation_keeps_partial_file_without_claiming_success() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("model");
        let (url, task) = server(
            "HTTP/1.1 200 OK\r\nContent-Length: 600\r\nConnection: close\r\n\r\nabc",
            false,
        );
        let mut download = Download::new(&path, &url);
        let token = download.token.clone();
        let result = download
            .async_download(move |downloaded, _| {
                if downloaded > 0 {
                    token.cancel();
                }
            })
            .await;
        task.join().unwrap();
        assert!(matches!(result, Err(ResourceError::Cancel(_))));
        assert!(!path.exists());
        assert!(PathBuf::from(format!("{}.download", path.display())).exists());
    }
}
