//! Application-owned paths; resolving a path never reads legacy homes.
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    pub fn from_home(home: &Path) -> Self {
        Self {
            root: home.join(".trnovel"),
        }
    }
    pub fn user_default() -> io::Result<Self> {
        dirs::home_dir()
            .map(|home| Self::from_home(&home))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法获取用户主目录"))
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn config(&self) -> PathBuf {
        self.root.join("config.toml")
    }
    pub fn keybindings(&self) -> PathBuf {
        self.root.join("keybindings.toml")
    }
    pub fn toc_rules(&self) -> PathBuf {
        self.root.join("toc_rules.json")
    }
    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn book_sources(&self) -> PathBuf {
        self.data().join("book_sources.json")
    }
    pub fn history(&self) -> PathBuf {
        self.data().join("history.json")
    }
    pub fn source_state(&self) -> PathBuf {
        self.data().join("source-state")
    }
    pub fn local(&self) -> PathBuf {
        self.data().join("local")
    }
    pub fn network(&self) -> PathBuf {
        self.data().join("network")
    }
    pub fn browser_profile(&self) -> PathBuf {
        self.data().join("browser-profile")
    }
    pub fn fontmap(&self) -> PathBuf {
        self.cache().join("gen-fontmap")
    }

    /// Clear only reading records and resources that can be rebuilt.
    pub fn clear(&self) -> io::Result<()> {
        match std::fs::remove_file(self.history()) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        for path in [self.local(), self.network(), self.cache()] {
            match std::fs::remove_dir_all(path) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clear_preserves_settings_sources_login_and_legacy_home() {
        let home = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_home(home.path());
        let preserved = [
            paths.config(),
            paths.keybindings(),
            paths.toc_rules(),
            paths.book_sources(),
            paths.source_state().join("login.json"),
            paths.browser_profile().join("Cookies"),
            home.path().join(".novel/history.json"),
        ];
        let removed = [
            paths.history(),
            paths.local().join("book.json"),
            paths.network().join("book.json"),
            paths.fontmap().join("font.ttf"),
        ];
        for path in preserved.iter().chain(&removed) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "unchanged").unwrap();
        }
        paths.clear().unwrap();
        paths.clear().unwrap();
        for path in preserved {
            assert_eq!(std::fs::read_to_string(path).unwrap(), "unchanged");
        }
        for path in removed {
            assert!(!path.exists());
        }
    }
}
