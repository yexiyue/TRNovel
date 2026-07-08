use crate::Result;
use crate::utils::novel_catch_dir;
use ratatui_kit::Palette;
use ratatui_kit_themes::{IntoKitPalette, ThemeName, terminal_background};
use serde::{Deserialize, Serialize};
use std::{fs::File, io::ErrorKind, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceConfig {
    pub theme_slug: String,
    pub background: BackgroundMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundMode {
    Theme,
    Terminal,
}

impl AppearanceConfig {
    const DEFAULT_THEME: ThemeName = ThemeName::TokyoNight;

    pub fn path() -> Result<PathBuf> {
        Ok(novel_catch_dir()?.join("appearance.json"))
    }

    pub fn load() -> Result<Self> {
        match File::open(Self::path()?) {
            Ok(file) => Ok(serde_json::from_reader(file).unwrap_or_default()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self) -> Result<()> {
        let file = File::create(Self::path()?)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn theme_name(&self) -> ThemeName {
        ThemeName::all()
            .iter()
            .copied()
            .find(|name| name.slug() == self.theme_slug)
            .unwrap_or(Self::DEFAULT_THEME)
    }

    pub fn palette(&self) -> Palette {
        let palette = self.theme_name().into_kit_palette();
        match self.background {
            BackgroundMode::Theme => palette,
            BackgroundMode::Terminal => terminal_background(palette),
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme_slug: Self::DEFAULT_THEME.slug().to_string(),
            background: BackgroundMode::Terminal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderDisplayConfig {
    #[serde(default = "default_show_title")]
    pub show_title: bool,
}

impl ReaderDisplayConfig {
    pub fn path() -> Result<PathBuf> {
        Ok(novel_catch_dir()?.join("reader-display.json"))
    }

    pub fn load() -> Result<Self> {
        match File::open(Self::path()?) {
            Ok(file) => Ok(serde_json::from_reader(file).unwrap_or_default()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self) -> Result<()> {
        let file = File::create(Self::path()?)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }
}

impl Default for ReaderDisplayConfig {
    fn default() -> Self {
        Self {
            show_title: default_show_title(),
        }
    }
}

fn default_show_title() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn unknown_theme_slug_falls_back_to_default_theme() {
        let config = AppearanceConfig {
            theme_slug: "missing-theme".to_string(),
            background: BackgroundMode::Terminal,
        };

        assert_eq!(config.theme_name(), AppearanceConfig::DEFAULT_THEME);
    }

    #[test]
    fn terminal_background_resets_only_background_layers() {
        let config = AppearanceConfig {
            theme_slug: ThemeName::Dracula.slug().to_string(),
            background: BackgroundMode::Terminal,
        };

        let palette = config.palette();
        let themed = ThemeName::Dracula.into_kit_palette();

        assert_eq!(palette.bg, Color::Reset);
        assert_eq!(palette.surface, Color::Reset);
        assert_eq!(palette.overlay, Color::Reset);
        assert_eq!(palette.accent, themed.accent);
        assert_eq!(palette.fg, themed.fg);
    }
}
