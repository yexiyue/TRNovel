use crate::keymap::{AppKeymap, ReaderAction, display_keys};
use crate::theme::presets::ThemeChoice;
use crate::{AppearanceConfig, BackgroundMode, ReaderDisplayConfig};

pub const SECTIONS: [&str; 5] = ["阅读", "外观", "键位", "数据", "关于"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Reader(usize),
    Theme,
    Background,
    Header,
    ConfirmExit,
    Info,
}

pub use crate::components::settings::Control;
pub type Row = crate::components::settings::Row<Key>;

#[derive(Clone, Default)]
pub struct Picker {
    pub key: Option<Key>,
    pub title: String,
    pub options: Vec<String>,
    pub selected: usize,
}

pub fn picker(
    key: Key,
    reader: &ReaderDisplayConfig,
    appearance: &AppearanceConfig,
) -> Option<Picker> {
    let (title, options, selected) = match key {
        Key::Reader(id) => {
            let (options, selected) = crate::pages::read_novel::settings::choices(reader, id)?;
            (if id == 6 { "边框" } else { "目录占宽" }, options, selected)
        }
        Key::Background => (
            "背景模式",
            vec!["主题背景".to_owned(), "终端背景".to_owned()],
            usize::from(appearance.background == BackgroundMode::Terminal),
        ),
        _ => return None,
    };
    Some(Picker {
        key: Some(key),
        title: title.to_owned(),
        options,
        selected,
    })
}

pub fn choose_reader(config: &mut ReaderDisplayConfig, key: Key, selected: usize) {
    if let Key::Reader(id) = key {
        crate::pages::read_novel::settings::choose_config(config, id, selected);
    }
}

pub fn rows(
    section: usize,
    reader: &ReaderDisplayConfig,
    appearance: &AppearanceConfig,
    keymap: &AppKeymap,
) -> Vec<Row> {
    match section {
        0 => crate::pages::read_novel::settings::rows::rows(reader)
            .into_iter()
            .map(|r| r.map_key(Key::Reader))
            .collect(),
        1 => {
            let theme = ThemeChoice::from_slug(&appearance.theme_slug)
                .map(|x| x.display_name().to_owned())
                .unwrap_or_else(|| appearance.theme_slug.clone());
            vec![
                row(
                    "主题",
                    Key::Theme,
                    "配色方案",
                    &theme,
                    "打开主题与自定义配色设置",
                    Control::Action,
                ),
                row(
                    "主题",
                    Key::Background,
                    "背景模式",
                    if appearance.background == BackgroundMode::Theme {
                        "主题背景"
                    } else {
                        "终端背景"
                    },
                    "选择使用配色方案或终端自身的背景",
                    Control::Choice,
                ),
                row(
                    "界面",
                    Key::Header,
                    "顶部栏",
                    "",
                    "隐藏后仍可按 F10 呼出菜单",
                    Control::Toggle(appearance.show_header),
                ),
                row(
                    "界面",
                    Key::ConfirmExit,
                    "退出确认",
                    "",
                    "退出前确认，可在退出弹窗内选择不再提醒",
                    Control::Toggle(appearance.confirm_exit),
                ),
            ]
        }
        2 => {
            const ACTIONS: &[(ReaderAction, &str)] = &[
                (ReaderAction::PageDown, "下一页"),
                (ReaderAction::PageUp, "上一页"),
                (ReaderAction::PrevChapter, "上一章"),
                (ReaderAction::NextChapter, "下一章"),
                (ReaderAction::SearchContent, "搜索正文"),
                (ReaderAction::ToggleReadMode, "目录 / 正文"),
                (ReaderAction::ToggleReaderSettings, "阅读设置"),
                (ReaderAction::ToggleInfo, "帮助"),
            ];
            let path = crate::paths::AppPaths::user_default()
                .map(|p| p.keybindings().display().to_string())
                .unwrap_or_default();
            ACTIONS
                .iter()
                .map(|(action, label)| {
                    row(
                        "当前绑定",
                        Key::Info,
                        label,
                        &display_keys(&keymap.reader, *action),
                        &format!("修改 {path} 后重启生效"),
                        Control::ReadOnly,
                    )
                })
                .collect()
        }
        3 => {
            let paths = crate::paths::AppPaths::user_default().ok();
            [
                ("配置", paths.as_ref().map(|p| p.config())),
                ("数据", paths.as_ref().map(|p| p.data())),
                ("缓存", paths.as_ref().map(|p| p.cache())),
            ]
            .into_iter()
            .map(|(label, path)| {
                let path = path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "无法获取用户目录".to_owned());
                row(
                    "本地目录",
                    Key::Info,
                    label,
                    &path,
                    &path,
                    Control::ReadOnly,
                )
            })
            .collect()
        }
        _ => {
            let build = match (cfg!(feature = "network"), cfg!(feature = "tts")) {
                (true, true) => "本地 + 网络 + 听书",
                (true, false) => "本地 + 网络",
                (false, true) => "本地 + 听书",
                (false, false) => "纯本地",
            };
            [
                ("版本", env!("CARGO_PKG_VERSION")),
                ("构建", build),
                ("上游仓库", "github.com/yexiyue/TRNovel"),
                ("许可证", "MIT"),
            ]
            .into_iter()
            .map(|(label, value)| row("TRNovel", Key::Info, label, value, value, Control::ReadOnly))
            .collect()
        }
    }
}

fn row(
    group: &'static str,
    key: Key,
    label: &str,
    value: &str,
    detail: &str,
    control: Control,
) -> Row {
    Row {
        group,
        key,
        label: label.to_owned(),
        value: value.to_owned(),
        detail: detail.to_owned(),
        control,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Frameline;
    #[test]
    fn reading_fields_cover_shared_settings_once() {
        let rows = crate::pages::read_novel::settings::rows::rows(&ReaderDisplayConfig::default());
        let mut ids = rows.iter().map(|r| r.key).collect::<Vec<_>>();
        ids.sort_unstable();
        assert_eq!(
            ids,
            (0..=crate::pages::read_novel::settings::BLANK_ROW).collect::<Vec<_>>()
        );
    }
    #[test]
    fn stepper_bounds_come_from_the_shared_adjustment_rules() {
        let mut config = ReaderDisplayConfig {
            h_margin: 6,
            ..Default::default()
        };
        let row = crate::pages::read_novel::settings::rows::rows(&config)
            .into_iter()
            .find(|r| r.key == 5)
            .unwrap();
        assert_eq!(
            row.control,
            Control::Number {
                decrease: true,
                increase: false
            }
        );
        let choice = picker(Key::Reader(6), &config, &AppearanceConfig::default()).unwrap();
        choose_reader(&mut config, Key::Reader(6), choice.options.len() - 1);
        assert_eq!(config.frameline, Frameline::None);
    }
}
