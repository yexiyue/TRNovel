use ratatui::style::{Color, Style, Stylize};
use serde::{Deserialize, Serialize};
use std::{fs::File, sync::LazyLock};

use crate::utils::novel_catch_dir;

pub static THEME_SETTING: LazyLock<ThemeSetting> = LazyLock::new(|| {
    let path = novel_catch_dir().unwrap().join("theme_setting.json");
    match File::open(path.clone()) {
        Ok(file) => serde_json::from_reader(file).unwrap_or_default(),
        Err(_) => ThemeSetting::default(),
    }
});

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSetting {
    pub color: Color,
    pub primary_color: Color,
    pub warning_color: Color,
    pub error_color: Color,
    pub success_color: Color,
    pub info_color: Color,

    pub basic: BasicSetting,
    pub logo: Style,
    pub highlight: Style,
    pub selected: Style,
    pub empty: Style,
    pub detail_info: Style,

    pub loading_modal: BasicSetting,
    pub warning_modal: BasicSetting,
    pub error_modal: BasicSetting,
    pub search: SearchSetting,
    pub novel: NovelSetting,
}

impl Default for ThemeSetting {
    fn default() -> Self {
        let color = Color::default();
        let primary_color = Color::LightBlue;
        let second_color = Color::LightCyan;
        let warning_color = Color::LightYellow;
        let error_color = Color::LightRed;
        let success_color = Color::LightGreen;
        let info_color = Color::DarkGray;
        let basic = BasicSetting {
            text: Style::new().fg(color),
            border: Style::new().dim(),
            border_info: Style::new().dim(),
            ..Default::default()
        };

        Self {
            color,
            primary_color,
            warning_color,
            error_color,
            success_color,
            info_color,
            detail_info: Style::new().fg(warning_color).bold(),
            logo: Style::new().light_green(),
            selected: Style::new().fg(success_color),
            highlight: Style::new().fg(second_color),
            empty: Style::new().fg(warning_color).bold(),
            search: SearchSetting {
                success_border: Style::new().fg(success_color),
                error_border: Style::new().fg(error_color),
                error_border_info: Style::new().fg(info_color),
                placeholder: Style::new().fg(second_color),
                text: Style::new().fg(color),
            },
            loading_modal: BasicSetting {
                border: basic.border.fg(second_color),
                text: Style::new().fg(second_color),
                ..Default::default()
            },
            warning_modal: BasicSetting {
                border: basic.border.fg(warning_color),
                text: Style::new().fg(warning_color),
                ..Default::default()
            },
            error_modal: BasicSetting {
                border: basic.border.fg(error_color),
                text: Style::new().fg(error_color),
                ..Default::default()
            },
            novel: NovelSetting {
                content: Style::new().fg(color),
                chapter: basic.border_title,
                page: basic.border_info,
                progress: basic.border_info,
                border: basic.border,
            },
            basic,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BasicSetting {
    pub text: Style,
    pub border: Style,
    pub border_title: Style,
    pub border_info: Style,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchSetting {
    pub success_border: Style,
    pub error_border: Style,
    pub error_border_info: Style,
    pub placeholder: Style,
    pub text: Style,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NovelSetting {
    pub chapter: Style,
    pub page: Style,
    pub content: Style,
    pub progress: Style,
    pub border: Style,
}
