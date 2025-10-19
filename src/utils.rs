use anyhow::{Result, anyhow};
use chrono::DateTime;
use std::path::{Path, PathBuf};

/// 获取小说缓存目录
pub fn novel_catch_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or(anyhow!("无法获取用户主目录"))?;

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
    let naive = DateTime::from_timestamp_millis(timestamp as i64).ok_or(anyhow!("时间戳无效"))?;

    // 格式化为指定的字符串格式
    Ok(naive.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// 将字节长度转换为可读的单位表示（B, KB, MB, GB, TB）
pub fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const KB: usize = 1024;

    let mut size = bytes;
    let mut unit_index = 0;

    while size >= KB && unit_index < UNITS.len() - 1 {
        size /= KB;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size, UNITS[unit_index])
    } else {
        format!(
            "{:.1} {}",
            (bytes as f64) / (KB as f64).powi(unit_index as i32),
            UNITS[unit_index]
        )
    }
}
