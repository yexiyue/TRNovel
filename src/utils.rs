use anyhow::{anyhow, Result};
use chrono::DateTime;
use std::{
    env,
    path::{Path, PathBuf},
};

/// 获取小说缓存目录
pub fn novel_catch_dir() -> Result<PathBuf> {
    let home = match env::var("HOME") {
        Ok(home) => PathBuf::from(home),
        Err(_) => env::current_exe()?
            .parent()
            .ok_or(anyhow!("获取当前执行文件路径失败"))?
            .to_path_buf(),
    };

    let novel_catch_path = PathBuf::new().join(&home).join(".novel");

    if !novel_catch_path.exists() {
        std::fs::create_dir(&novel_catch_path)?;
    }

    Ok(novel_catch_path)
}

pub fn get_path_md5<T: AsRef<Path>>(path: T) -> Result<String> {
    let md5 = md5::compute(path.as_ref().canonicalize()?.to_string_lossy().as_bytes());
    Ok(format!("{:x}", md5))
}

pub fn get_md5_string<T: ToString>(value: T) -> String {
    let md5 = md5::compute(value.to_string());
    format!("{:x}", md5)
}

pub fn time_to_string(timestamp: u64) -> anyhow::Result<String> {
    // 将时间戳转换为NaiveDateTime
    let naive = DateTime::from_timestamp_millis(timestamp as i64).unwrap();

    // 格式化为指定的字符串格式
    Ok(naive.format("%Y-%m-%d %H:%M:%S").to_string())
}
