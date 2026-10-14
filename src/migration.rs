//! Explicit startup migration; old data stays intact and existing new files win.
use crate::{config::AppConfig, paths::AppPaths};
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::Path,
};

const MARKER: &str = ".legacy-migration-v1";
const CLEARED: &str = ".legacy-reading-cleared-v1";

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

/// Run after argument parsing, outside the async runtime's worker threads.
pub async fn startup() {
    let result = tokio::task::spawn_blocking(|| {
        let paths = AppPaths::user_default()?;
        migrate(&paths)
    })
    .await;
    match result {
        Ok(Ok(messages)) => {
            for message in messages {
                eprintln!("旧目录迁移：{message}（源文件保留，下次启动重试）");
            }
        }
        Ok(Err(error)) => eprintln!("旧目录迁移失败：{error:#}；源文件保留，继续启动"),
        Err(error) => eprintln!("旧目录迁移未完成：{error}；继续启动"),
    }
}

fn migrate(paths: &AppPaths) -> Result<Vec<String>> {
    let home = paths.root().parent().context("应用目录没有父目录")?;
    let legacy = home.join(".novel");
    let Some(kind) = file_type(&legacy)? else {
        return Ok(Vec::new());
    };
    if !kind.is_dir() {
        anyhow::bail!("跳过非普通旧目录 {}", legacy.display());
    }
    directory(paths.root(), paths.root())?;
    let lock = lock(&paths.root().join(".migration.lock"))?;
    lock.lock()?;
    if file_type(&paths.root().join(MARKER))?.is_some() {
        return Ok(Vec::new());
    }
    let mut messages = Vec::new();
    let cleared = file_type(&paths.root().join(CLEARED))?.is_some();
    for (source, destination) in [
        ("book_sources.json", paths.book_sources()),
        ("history.json", paths.history()),
        ("source-state", paths.source_state()),
        ("browser-profile", paths.browser_profile()),
        ("local", paths.local()),
        ("network", paths.network()),
        ("keybindings.toml", paths.keybindings()),
        ("toc_rules.json", paths.toc_rules()),
        ("gen-fontmap", paths.fontmap()),
    ] {
        if cleared && matches!(source, "history.json" | "local" | "network" | "gen-fontmap") {
            continue;
        }
        if let Err(error) = copy(&legacy.join(source), &destination, paths.root()) {
            messages.push(format!("{source}: {error:#}"));
        }
    }
    if let Err(error) = preferences(&legacy, paths) {
        messages.push(format!("外观与阅读偏好: {error:#}"));
    }
    if messages.is_empty() {
        publish(&paths.root().join(MARKER), b"legacy .novel migration v1\n")?;
    }
    Ok(messages)
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

fn copy(source: &Path, destination: &Path, root: &Path) -> Result<()> {
    let Some(kind) = file_type(source)? else {
        return Ok(());
    };
    if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
        eprintln!("旧目录迁移：跳过非普通文件 {}", source.display());
        return Ok(());
    }
    if kind.is_dir() {
        directory(destination.parent().context("目标没有父目录")?, root)?;
        if file_type(destination)?.is_some_and(|kind| !kind.is_dir()) {
            eprintln!(
                "旧目录迁移：保留已有路径 {}，跳过旧目录 {}",
                destination.display(),
                source.display()
            );
            return Ok(());
        }
        directory(destination, root)?;
        let mut errors = Vec::new();
        for entry in fs::read_dir(source)? {
            match entry {
                Ok(entry) => {
                    if let Err(error) =
                        copy(&entry.path(), &destination.join(entry.file_name()), root)
                    {
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
        eprintln!(
            "旧目录迁移：保留新文件 {}，未覆盖 {}",
            destination.display(),
            source.display()
        );
        return Ok(());
    }
    let mut source_file = File::open(source)?;
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
    finish(temporary, destination)
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

fn finish(temporary: tempfile::NamedTempFile, destination: &Path) -> Result<()> {
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(destination) {
        Ok(_) => {
            #[cfg(unix)]
            File::open(destination.parent().context("目标没有父目录")?)?.sync_all()?;
            Ok(())
        }
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error.into()),
    }
}

fn publish(destination: &Path, bytes: &[u8]) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(destination.parent().context("目标没有父目录")?)?;
    temporary.write_all(bytes)?;
    finish(temporary, destination)
}

fn section<T: DeserializeOwned>(path: &Path, names: &[(&str, &str)]) -> Result<Option<T>> {
    let Some(kind) = file_type(path)? else {
        return Ok(None);
    };
    if !kind.is_file() {
        eprintln!("旧目录迁移：跳过非普通偏好文件 {}", path.display());
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

fn preferences(legacy: &Path, paths: &AppPaths) -> Result<()> {
    let config_lock = lock(&paths.config().with_extension("lock"))?;
    config_lock.lock()?;
    if file_type(&paths.config())?.is_some() {
        eprintln!(
            "旧目录迁移：保留已有配置 {}，不合并旧偏好",
            paths.config().display()
        );
        return Ok(());
    }
    let appearance = section(
        &legacy.join("appearance.json"),
        &[("themeSlug", "theme_slug"), ("showHeader", "show_header")],
    )?;
    let reader = section(
        &legacy.join("reader-display.json"),
        &[
            ("followTts", "follow_tts"),
            ("showTitle", "show_title"),
            ("pageOverlap", "page_overlap"),
            ("paragraphSpacing", "paragraph_spacing"),
        ],
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
    publish(&paths.config(), toml::to_string_pretty(&config)?.as_bytes())
}

#[cfg(test)]
mod tests;
