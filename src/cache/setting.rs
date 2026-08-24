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
    /// 翻页时与上一屏重叠保留的行数,翻页步长由 [`Self::page_step`] 派生。
    /// 取值恒在 `0..=PAGE_OVERLAP_MAX`:磁盘脏值在 [`Self::load`] 归一,
    /// 运行期只经 [`Self::increase_page_overlap`] / [`Self::decrease_page_overlap`] 修改。
    #[serde(default = "default_page_overlap")]
    pub page_overlap: u16,
    /// 是否在逻辑段落之间补一个空行。小说正文多以单换行分段,不补则段落首尾相接、
    /// 读起来发闷;但补了会让总行数近乎翻倍,小屏用户一屏能看的正文明显变少 ——
    /// 众口难调,交给用户在阅读设置面板里切。
    #[serde(default = "default_paragraph_spacing")]
    pub paragraph_spacing: bool,
}

impl ReaderDisplayConfig {
    /// 翻页重叠行数上限。再大就会把「翻一页」压成「滚几行」,属配置错误而非偏好。
    const PAGE_OVERLAP_MAX: u16 = 10;

    pub fn path() -> Result<PathBuf> {
        Ok(novel_catch_dir()?.join("reader-display.json"))
    }

    /// 给定可见行数下的翻页步长:整屏减去重叠行数。
    ///
    /// 步长不落盘,每次按当前视口现算 → 终端尺寸变化后自动跟随;重叠 ≥ 视口时
    /// (终端极矮)兜底为 1,保证翻页仍能推进。正文与设置面板的「每页滚动 M 行」
    /// 都调它,不各自抄公式。
    pub fn page_step(&self, visible_lines: usize) -> usize {
        visible_lines
            .saturating_sub(self.page_overlap as usize)
            .max(1)
    }

    pub fn increase_page_overlap(&mut self) {
        self.page_overlap = (self.page_overlap + 1).min(Self::PAGE_OVERLAP_MAX);
    }

    pub fn decrease_page_overlap(&mut self) {
        self.page_overlap = self.page_overlap.saturating_sub(1);
    }

    pub fn load() -> Result<Self> {
        match File::open(Self::path()?) {
            Ok(file) => {
                let mut config: Self = serde_json::from_reader(file).unwrap_or_default();
                // 手改 JSON 是唯一能产生越界值的入口,在此归一 → 之后全程直接读字段,
                // 且下次 save() 写回的也是规范值(读侧兜底会让脏值永远留在磁盘上)。
                config.page_overlap = config.page_overlap.min(Self::PAGE_OVERLAP_MAX);
                Ok(config)
            }
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
            page_overlap: default_page_overlap(),
            paragraph_spacing: default_paragraph_spacing(),
        }
    }
}

fn default_show_title() -> bool {
    true
}

/// 默认关闭:补空行会让正文总行数近乎翻倍,一屏能看的内容少近一半 —— 这对
/// 「一屏多看几行」的终端读者是明显的退步,不该替所有人做主。想要宽松排版的
/// 用户在阅读设置面板里一眼可开。
fn default_paragraph_spacing() -> bool {
    false
}

/// 默认保留 2 行重叠:Vim `Ctrl-F`、`less` 等的通行默认,翻页后仍有视觉锚点。
/// 取 0(整屏平移)是旧行为,想要的用户在阅读设置面板里一眼可调。
fn default_page_overlap() -> u16 {
    2
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
    fn page_overlap_defaults_to_two() {
        assert_eq!(ReaderDisplayConfig::default().page_overlap, 2);
    }

    #[test]
    fn legacy_config_without_page_overlap_takes_default() {
        // 旧版本写出的文件只有 showTitle;缺字段走默认,既有字段不受影响。
        let config: ReaderDisplayConfig = serde_json::from_str(r#"{"showTitle": false}"#).unwrap();

        assert!(!config.show_title);
        assert_eq!(config.page_overlap, 2);
        assert!(!config.paragraph_spacing);
    }

    #[test]
    fn paragraph_spacing_defaults_to_off_and_round_trips() {
        assert!(!ReaderDisplayConfig::default().paragraph_spacing);

        // 显式开启必须能被读回来,不能被 default 压回关。
        let config: ReaderDisplayConfig =
            serde_json::from_str(r#"{"paragraphSpacing": true}"#).unwrap();
        assert!(config.paragraph_spacing);
    }

    #[test]
    fn page_overlap_stays_within_range() {
        let mut config = ReaderDisplayConfig::default();

        for _ in 0..ReaderDisplayConfig::PAGE_OVERLAP_MAX + 5 {
            config.increase_page_overlap();
        }
        assert_eq!(config.page_overlap, ReaderDisplayConfig::PAGE_OVERLAP_MAX);

        for _ in 0..ReaderDisplayConfig::PAGE_OVERLAP_MAX + 5 {
            config.decrease_page_overlap();
        }
        assert_eq!(config.page_overlap, 0);
    }

    #[test]
    fn page_step_subtracts_overlap_and_never_stalls() {
        let mut config = ReaderDisplayConfig::default();

        assert_eq!(config.page_step(30), 28);
        config.page_overlap = 0;
        assert_eq!(config.page_step(30), 30);
        // 终端极矮:重叠吃掉整屏也必须还能推进 1 行。
        config.page_overlap = ReaderDisplayConfig::PAGE_OVERLAP_MAX;
        assert_eq!(config.page_step(3), 1);
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
