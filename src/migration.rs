//! 旧目录 `~/.novel` 的迁移:同意门控 + 可观察、可取消的引擎。
//!
//! 旧数据保持原样、既有新文件优先;单文件原子落盘,完成标记不早于内容持久化。
//! 引擎经 [`Event`] 播报进度(计数快照,不逐文件输出),取消是协作式的——
//! 取消/中断不发布完成标记,已拷贝部分靠 noclobber 语义成为下次的「保留已有」。

use crate::{config::AppConfig, paths::AppPaths};
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const MARKER: &str = ".legacy-migration-v1";
const CLEARED: &str = ".legacy-reading-cleared-v1";
const DECLINED: &str = ".legacy-migration-declined-v1";

/// 迁移条目:源名(位于 `~/.novel` 下)与新位置。
fn destinations(paths: &AppPaths) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("book_sources.json", paths.book_sources()),
        ("history.json", paths.history()),
        ("source-state", paths.source_state()),
        ("browser-profile", paths.browser_profile()),
        ("local", paths.local()),
        ("network", paths.network()),
        ("keybindings.toml", paths.keybindings()),
        ("toc_rules.json", paths.toc_rules()),
        ("gen-fontmap", paths.fontmap()),
    ]
}

/// 清空阅读数据后不再重导入的阅读类条目。
const READING_SOURCES: &[&str] = &["history.json", "local", "network", "gen-fontmap"];

/// 启动状态;仅做 stat 级检查,不做任何拷贝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 无遗留数据(或遗留路径不是普通目录,另见 [`Self::NotNeeded`] 的容错)。
    NotNeeded,
    /// 完成标记已存在,无事可做。
    Done,
    /// 用户曾拒绝自动迁移询问;显式 `trn migrate` 不受影响。
    Declined,
    /// 遗留数据存在且未完成、未拒绝,等待用户决定。
    Pending,
}

/// stat 级状态检查;遗留目录异常(如指向外部的符号链接)视为无遗留,不阻塞启动。
pub fn status_at(paths: &AppPaths) -> Status {
    let Ok(marker) = fs::symlink_metadata(paths.root().join(MARKER)) else {
        return pending_or_not(paths);
    };
    if marker.is_file() {
        return Status::Done;
    }
    pending_or_not(paths)
}

fn pending_or_not(paths: &AppPaths) -> Status {
    let home = match paths.root().parent() {
        Some(home) => home,
        None => return Status::NotNeeded,
    };
    match fs::symlink_metadata(home.join(".novel")) {
        Ok(metadata) if metadata.is_dir() => {
            if fs::symlink_metadata(paths.root().join(DECLINED)).is_ok_and(|m| m.is_file()) {
                Status::Declined
            } else {
                Status::Pending
            }
        }
        _ => Status::NotNeeded,
    }
}

/// 预扫描:统计待迁移的条目数、文件数与字节数(尽力而为,失败计 0)。
fn survey(paths: &AppPaths, legacy: &Path, cleared: bool) -> (usize, u64, u64) {
    let mut items = 0;
    let mut files = 0;
    let mut bytes = 0;
    for (source, _) in destinations(paths) {
        if cleared && READING_SOURCES.contains(&source) {
            continue;
        }
        let path = legacy.join(source);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                items += 1;
                files += 1;
                bytes += metadata.len();
            }
            Ok(metadata) if metadata.is_dir() => {
                items += 1;
                measure(&path, &mut files, &mut bytes);
            }
            _ => {}
        }
    }
    let preference_files = preference_files(legacy);
    for (_, size) in &preference_files {
        files += 1;
        bytes += size;
    }
    if !preference_files.is_empty() {
        items += 1;
    }
    (items, files, bytes)
}

/// 用户待迁移规模(询问界面展示);仅 Pending 时有意义,失败计 0。
pub fn pending_survey(paths: &AppPaths) -> (usize, u64, u64) {
    let Some(home) = paths.root().parent() else {
        return (0, 0, 0);
    };
    let legacy = home.join(".novel");
    let cleared = file_type(&paths.root().join(CLEARED))
        .map(|kind| kind.is_some())
        .unwrap_or(false);
    survey(paths, &legacy, cleared)
}

fn measure(path: &Path, files: &mut u64, bytes: &mut u64) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            *files += 1;
            if let Ok(metadata) = entry.metadata() {
                *bytes += metadata.len();
            }
        } else if file_type.is_dir() {
            measure(&entry.path(), files, bytes);
        }
    }
}

fn preference_files(legacy: &Path) -> Vec<(&'static str, u64)> {
    [
        "appearance.json",
        "reader-display.json",
        "browser_assist.on",
    ]
    .into_iter()
    .filter_map(|name| {
        let metadata = fs::symlink_metadata(legacy.join(name)).ok()?;
        metadata.is_file().then_some((name, metadata.len()))
    })
    .collect()
}

/// 引擎向外播报的事件;UI 适配器折叠为进度快照,无头适配器打印摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// 预扫描完成:条目数、源侧文件数与字节数(进度条分母)。
    Scan {
        items: usize,
        files: u64,
        bytes: u64,
    },
    /// 开始迁移一个条目。
    Item { name: &'static str },
    /// 一个文件拷贝完成(带字节数)。
    Copied { bytes: u64 },
    /// 目标已有新文件,保留不覆盖(计数,不逐文件输出)。
    Kept,
    /// 跳过非普通文件或已有配置(计数)。
    Skipped,
}

/// 迁移终态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 全部条目成功,完成标记已发布(或本就无需迁移)。
    Done,
    /// 用户取消;未发布完成标记,已拷贝部分保留。
    Cancelled,
    /// 条目级错误(下次可重试)或整体失败。
    Failed {
        fatal: Option<String>,
        errors: Vec<String>,
    },
}

/// 执行迁移。事件经 `emit` 播报;`cancel` 在文件边界协作式检查。
/// 任何错误都折叠进 [`Outcome::Failed`](fatal 携带错误链),此函数不返回 Err、不 panic。
pub fn run(paths: &AppPaths, cancel: &AtomicBool, emit: &mut dyn FnMut(Event)) -> Outcome {
    match run_inner(paths, cancel, emit) {
        Ok(outcome) => outcome,
        Err(error) => Outcome::Failed {
            fatal: Some(format!("{error:#}")),
            errors: Vec::new(),
        },
    }
}

fn run_inner(
    paths: &AppPaths,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    (|| {
        let home = paths.root().parent().context("应用目录没有父目录")?;
        let legacy = home.join(".novel");
        let Some(kind) = file_type(&legacy)? else {
            return Ok(Outcome::Done);
        };
        if !kind.is_dir() {
            anyhow::bail!("跳过非普通旧目录 {}", legacy.display());
        }
        if file_type(&paths.root().join(MARKER))?.is_some_and(|kind| kind.is_file()) {
            return Ok(Outcome::Done);
        }
        let cleared = file_type(&paths.root().join(CLEARED))?.is_some();
        let (items, files, bytes) = survey(paths, &legacy, cleared);
        emit(Event::Scan {
            items,
            files,
            bytes,
        });

        directory(paths.root(), paths.root())?;
        let lock = lock(&paths.root().join(".migration.lock"))?;
        lock.lock()?;
        // 拿到锁后复查:并发的首个实例可能刚刚完成迁移。
        if file_type(&paths.root().join(MARKER))?.is_some_and(|kind| kind.is_file()) {
            return Ok(Outcome::Done);
        }
        let mut runner = Runner {
            cancel,
            emit,
            errors: Vec::new(),
            dirs: Vec::new(),
        };
        for (source, destination) in destinations(paths) {
            if cleared && READING_SOURCES.contains(&source) {
                continue;
            }
            if runner.cancelled() {
                return Ok(Outcome::Cancelled);
            }
            runner.item(source);
            if let Err(error) = copy(
                &legacy.join(source),
                &destination,
                paths.root(),
                &mut runner,
            ) {
                runner.errors.push(format!("{source}: {error:#}"));
            }
        }
        if let Err(error) = preferences(&legacy, paths, &mut runner) {
            runner.errors.push(format!("外观与阅读偏好: {error:#}"));
        }
        if runner.cancelled() {
            return Ok(Outcome::Cancelled);
        }
        if runner.errors.is_empty() {
            // 完成标记不早于内容持久化:先批量同步本次触及的目录,再发布标记。
            runner.sync_dirs()?;
            publish(&paths.root().join(MARKER), b"legacy .novel migration v1\n")?;
            Ok(Outcome::Done)
        } else {
            Ok(Outcome::Failed {
                fatal: None,
                errors: runner.errors,
            })
        }
    })()
}

/// 单线程执行上下文:贯穿递归拷贝的取消、事件与批量目录同步。
struct Runner<'a> {
    cancel: &'a AtomicBool,
    emit: &'a mut dyn FnMut(Event),
    errors: Vec<String>,
    dirs: Vec<PathBuf>,
}

impl Runner<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn item(&mut self, name: &'static str) {
        (self.emit)(Event::Item { name });
    }
    fn emit(&mut self, event: Event) {
        (self.emit)(event);
    }
    fn record_directory(&mut self, path: &Path, root: &Path) {
        let mut current = Some(path);
        while let Some(directory) = current {
            self.dirs.push(directory.to_path_buf());
            if directory == root {
                break;
            }
            current = directory.parent();
        }
    }
    /// 目录 fsync 收尾批量执行:逐文件目录同步是大目录拷贝的主要耗时来源。
    fn sync_dirs(&mut self) -> Result<()> {
        self.dirs.sort();
        self.dirs.dedup();
        self.dirs
            .sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for dir in &self.dirs {
            File::open(dir)?.sync_all()?;
        }
        Ok(())
    }
}

/// 测试兼容入口:静默执行,返回条目级错误(供既有断言)。
#[cfg(test)]
fn migrate(paths: &AppPaths) -> Result<Vec<String>> {
    let cancel = AtomicBool::new(false);
    match run(paths, &cancel, &mut |_| {}) {
        Outcome::Done | Outcome::Cancelled => Ok(Vec::new()),
        Outcome::Failed {
            fatal: Some(error), ..
        } => Err(anyhow::Error::msg(error)),
        Outcome::Failed {
            fatal: None,
            errors,
        } => Ok(errors),
    }
}

/// Keep an explicit clear authoritative even after only part of migration succeeded.
pub fn clear(paths: &AppPaths) -> Result<()> {
    directory(paths.root(), paths.root())?;
    if file_type(&paths.data())?.is_some() {
        directory(&paths.data(), paths.root())?;
    }
    let lock = lock(&paths.root().join(".migration.lock"))?;
    lock.lock()?;
    publish(
        &paths.root().join(CLEARED),
        b"legacy reading data cleared v1\n",
    )?;
    paths.clear()?;
    Ok(())
}

/// 写入拒绝标记:此后正常启动不再自动询问(`trn migrate` 不受影响)。
pub fn publish_declined(paths: &AppPaths) -> Result<()> {
    directory(paths.root(), paths.root())?;
    publish(
        &paths.root().join(DECLINED),
        b"legacy .novel migration declined v1\n",
    )
}

fn file_type(path: &Path) -> std::io::Result<Option<fs::FileType>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata.file_type())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

// Check each parent before creating children; never follow destination symlinks.
fn directory(path: &Path, root: &Path) -> Result<()> {
    if path != root {
        directory(path.parent().context("目录没有父目录")?, root)?;
    }
    match file_type(path)? {
        Some(kind) if kind.is_dir() => Ok(()),
        Some(_) => anyhow::bail!("目标不是普通目录 {}", path.display()),
        None => match fs::create_dir(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => directory(path, root),
            Err(error) => Err(error.into()),
        },
    }
}

fn lock(path: &Path) -> Result<File> {
    if file_type(path)?.is_some_and(|kind| !kind.is_file()) {
        anyhow::bail!("锁不是普通文件 {}", path.display());
    }
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}

fn copy(source: &Path, destination: &Path, root: &Path, runner: &mut Runner) -> Result<()> {
    if runner.cancelled() {
        return Ok(());
    }
    let Some(kind) = file_type(source)? else {
        return Ok(());
    };
    if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
        runner.emit(Event::Skipped);
        return Ok(());
    }
    if kind.is_dir() {
        directory(destination.parent().context("目标没有父目录")?, root)?;
        if file_type(destination)?.is_some_and(|kind| !kind.is_dir()) {
            runner.emit(Event::Kept);
            return Ok(());
        }
        directory(destination, root)?;
        runner.record_directory(destination, root);
        let mut errors = Vec::new();
        for entry in fs::read_dir(source)? {
            if runner.cancelled() {
                break;
            }
            match entry {
                Ok(entry) => {
                    if let Err(error) = copy(
                        &entry.path(),
                        &destination.join(entry.file_name()),
                        root,
                        runner,
                    ) {
                        errors.push(format!("{}: {error:#}", entry.path().display()));
                    }
                }
                Err(error) => errors.push(error.to_string()),
            }
        }
        if !errors.is_empty() {
            anyhow::bail!("{}", errors.join("；"));
        }
        return Ok(());
    }
    let parent = destination.parent().context("目标没有父目录")?;
    directory(parent, root)?;
    // An existing new file, including a link, is never replaced or read.
    if file_type(destination)?.is_some() {
        runner.emit(Event::Kept);
        return Ok(());
    }
    let mut source_file = File::open(source)?;
    let bytes = source_file.metadata()?.len();
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut source_file, temporary.as_file_mut())?;
    if source
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        validate_json(temporary.reopen()?, destination, root)
            .with_context(|| format!("旧 JSON 损坏 {}", source.display()))?;
    }
    temporary
        .as_file()
        .set_permissions(source_file.metadata()?.permissions())?;
    // 单文件内容 fsync 保留;父目录 fsync 由 Runner::sync_dirs 收尾批量执行。
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(destination) {
        Ok(_) => {
            runner.record_directory(parent, root);
            runner.emit(Event::Copied { bytes });
            Ok(())
        }
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error.into()),
    }
}

fn validate_json(file: File, destination: &Path, root: &Path) -> Result<()> {
    let value: serde_json::Value = serde_json::from_reader(file)?;
    let data = root.join("data");
    if destination == data.join("history.json") {
        // Deserializing History itself would run its Drop::save on validation.
        serde_json::from_value::<Vec<(String, crate::cache::HistoryItem)>>(
            value
                .get("histories")
                .context("历史缺少 histories")?
                .clone(),
        )?;
        if let Some(path) = value.get("local_path") {
            serde_json::from_value::<Option<std::path::PathBuf>>(path.clone())?;
        }
    } else if destination == data.join("book_sources.json") {
        let sources = value.get("book_sources").cloned().unwrap_or(value);
        parse_book_source::BookSource::from_value_many(sources)
            .map_err(parse_book_source::BookSourceError::from)?;
    } else if destination.starts_with(data.join("local")) {
        serde_json::from_value::<crate::cache::LocalNovelCache>(value)?;
    } else if destination.starts_with(data.join("network")) {
        serde_json::from_value::<crate::cache::NetworkNovelCache>(value)?;
    }
    Ok(())
}

/// 单文件原子落盘:内容 fsync + 不覆盖重命名。父目录持久化由调用方负责。
fn finish(temporary: tempfile::NamedTempFile, destination: &Path) -> Result<()> {
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(destination) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error.into()),
    }
}

/// 标记发布自带目录持久化:标记绝不早于其存在性可见。
fn publish(destination: &Path, bytes: &[u8]) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(destination.parent().context("目标没有父目录")?)?;
    temporary.write_all(bytes)?;
    finish(temporary, destination)?;
    File::open(destination.parent().context("目标没有父目录")?)?.sync_all()?;
    Ok(())
}

fn section<T: DeserializeOwned>(
    path: &Path,
    names: &[(&str, &str)],
    runner: &mut Runner,
) -> Result<Option<T>> {
    let Some(kind) = file_type(path)? else {
        return Ok(None);
    };
    if !kind.is_file() {
        runner.emit(Event::Skipped);
        return Ok(None);
    }
    let mut value: serde_json::Value = serde_json::from_reader(File::open(path)?)
        .with_context(|| format!("旧 JSON 损坏 {}", path.display()))?;
    let fields = value.as_object_mut().context("旧偏好必须是 JSON 对象")?;
    for (old, new) in names {
        if let Some(value) = fields.remove(*old) {
            fields.insert((*new).into(), value);
        }
    }
    Ok(Some(serde_json::from_value(value)?))
}

fn preferences(legacy: &Path, paths: &AppPaths, runner: &mut Runner) -> Result<()> {
    runner.item("外观与阅读偏好");
    let sources = preference_files(legacy);
    let config_lock = lock(&paths.config().with_extension("lock"))?;
    config_lock.lock()?;
    if file_type(&paths.config())?.is_some() {
        for _ in sources {
            runner.emit(Event::Kept);
        }
        return Ok(());
    }
    let appearance = section(
        &legacy.join("appearance.json"),
        &[("themeSlug", "theme_slug"), ("showHeader", "show_header")],
        runner,
    )?;
    let reader = section(
        &legacy.join("reader-display.json"),
        &[
            ("followTts", "follow_tts"),
            ("showTitle", "show_title"),
            ("pageOverlap", "page_overlap"),
            ("paragraphSpacing", "paragraph_spacing"),
        ],
        runner,
    )?;
    let browser = file_type(&legacy.join("browser_assist.on"))?.is_some_and(|kind| kind.is_file());
    if appearance.is_none() && reader.is_none() && !browser {
        return Ok(());
    }
    let mut config = AppConfig::default();
    if let Some(appearance) = appearance {
        config.appearance = appearance;
    }
    if let Some(reader) = reader {
        config.reader = reader;
    }
    config.reader.normalize();
    config.browser.always_allow = browser;
    publish(&paths.config(), toml::to_string_pretty(&config)?.as_bytes())?;
    for (_, bytes) in sources {
        runner.emit(Event::Copied { bytes });
    }
    Ok(())
}

pub mod ui;

#[cfg(test)]
mod tests;
