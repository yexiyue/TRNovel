use anyhow::{anyhow, Result};
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
