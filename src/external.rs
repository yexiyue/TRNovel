//! Typed destinations opened by system launchers without interpolating shell commands.
use std::{io, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Config,
    Data,
    Cache,
    Repository,
    Documentation,
}
#[derive(Debug)]
pub enum Target {
    File(PathBuf),
    Directory(PathBuf),
    Url(&'static str),
}
impl Link {
    pub fn target(self) -> io::Result<Target> {
        use crate::paths::AppPaths;
        Ok(match self {
            Self::Repository => Target::Url("https://github.com/yexiyue/TRNovel"),
            Self::Documentation => Target::Url("https://yexiyue.github.io/TRNovel"),
            Self::Config => Target::File(AppPaths::user_default()?.config()),
            Self::Data => Target::Directory(AppPaths::user_default()?.data()),
            Self::Cache => Target::Directory(AppPaths::user_default()?.cache()),
        })
    }
}
impl Target {
    pub fn display(&self) -> String {
        match self {
            Self::Url(url) => format!("{} ↗", url.trim_start_matches("https://")),
            Self::File(path) | Self::Directory(path) => {
                if let Some(home) = dirs::home_dir().filter(|p| !p.as_os_str().is_empty())
                    && let Ok(relative) = path.strip_prefix(home)
                {
                    return format!("~/{}", relative.display());
                }
                path.display().to_string()
            }
        }
    }
    pub fn description(&self) -> String {
        match self {
            Self::Url(url) => format!("在浏览器打开 · {url}"),
            Self::File(path) => format!("定位配置文件 · {}", path.display()),
            Self::Directory(path) => format!("打开目录 · {}", path.display()),
        }
    }
    pub fn open(&self) -> io::Result<()> {
        match self {
            Self::Url(url) => open_default(std::ffi::OsStr::new(url)),
            Self::Directory(path) => {
                if !path.is_dir() {
                    return Err(io::Error::new(io::ErrorKind::NotFound, "目录不存在"));
                }
                open_default(path.as_os_str())
            }
            Self::File(path) => {
                if !path.is_file() {
                    return Err(io::Error::new(io::ErrorKind::NotFound, "配置文件不存在"));
                }
                reveal(path)
            }
        }
    }
}
fn open_default(target: &std::ffi::OsStr) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos_open(target, false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        open::that(target)
    }
}
#[cfg(target_os = "macos")]
fn macos_open(target: &std::ffi::OsStr, reveal: bool) -> io::Result<()> {
    let mut command = std::process::Command::new("open");
    if reveal {
        command.arg("-R");
    }
    let status = command
        .arg("--")
        .arg(target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("系统打开操作失败"))
    }
}
fn reveal(path: &std::path::Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos_open(path.as_os_str(), true)
    }
    #[cfg(target_os = "windows")]
    {
        let mut argument = std::ffi::OsString::from("/select,");
        argument.push(path);
        let status = std::process::Command::new("explorer.exe")
            .arg(argument)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("文件管理器打开失败"))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        open::that(
            path.parent()
                .ok_or_else(|| io::Error::other("配置文件没有父目录"))?,
        )
    }
}
