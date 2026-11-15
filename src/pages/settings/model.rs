use crate::keymap::{AppKeymap, ReaderAction, display_keys};
use crate::pages::read_novel::settings::{
    BLANK_ROW, GLOBAL_HINT_ROW, TOC_ROW, adjust_config, describe_setting,
};
use crate::theme::presets::ThemeChoice;
use crate::{AppearanceConfig, BackgroundMode, Frameline, ReaderDisplayConfig, TocRatio};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Toggle(bool),
    Number { decrease: bool, increase: bool },
    Choice,
    Action,
    ReadOnly,
}

#[derive(Clone)]
pub struct Row {
    pub group: &'static str,
    pub key: Key,
    pub label: String,
    pub value: String,
    pub detail: String,
    pub control: Control,
}

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
        Key::Reader(6) => (
            "边框",
            Frameline::ALL
                .iter()
                .map(|x| x.label().to_owned())
                .collect(),
            Frameline::ALL
                .iter()
                .position(|x| *x == reader.frameline)
                .unwrap_or(0),
        ),
        Key::Reader(TOC_ROW) => (
            "目录占宽",
            TocRatio::ALL.iter().map(|x| x.label().to_owned()).collect(),
            TocRatio::ALL
                .iter()
                .position(|x| *x == reader.toc_ratio)
                .unwrap_or(0),
        ),
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
    match key {
        Key::Reader(6) => {
            if let Some(value) = Frameline::ALL.get(selected) {
                config.frameline = *value;
            }
        }
        Key::Reader(TOC_ROW) => {
            if let Some(value) = TocRatio::ALL.get(selected) {
                config.toc_ratio = *value;
            }
        }
        _ => {}
    }
}

pub fn rows(
    section: usize,
    reader: &ReaderDisplayConfig,
    appearance: &AppearanceConfig,
    keymap: &AppKeymap,
) -> Vec<Row> {
    match section {
        0 => reading_rows(reader),
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

fn reading_rows(config: &ReaderDisplayConfig) -> Vec<Row> {
    let mut fields = vec![
        (4, "排版", "0 表示跟随窗口宽度"),
        (5, "排版", "正文左右各留白的终端列数"),
        (3, "排版", "只缩进段落第一行，不改原文"),
        (2, "排版", "段落之间额外补充的空行数"),
        (6, "排版", "选择阅读正文边框样式"),
        (TOC_ROW, "排版", "目录栏占阅读区域的比例"),
        (0, "翻页", "翻页时保留上一屏末尾的行数"),
        (9, "翻页", "方向键按整页翻动，j / k 仍逐行移动"),
        (BLANK_ROW, "翻页", "关闭时末页贴底，不产生多余空白"),
        (1, "界面", "控制界面标题，不删除 TXT 原文中的标题"),
        (7, "界面", "开启时将标题放在边框上，并显示边框听书状态"),
        (8, "界面", "显示阅读页提示与设置页操作说明"),
        (
            GLOBAL_HINT_ROW,
            "界面",
            "显示其它页面最底部的返回、菜单与退出提示",
        ),
    ];
    if cfg!(feature = "tts") {
        fields.push((10, "听书", "跟随实际播放位置自动滚动正文"));
    }
    fields
        .into_iter()
        .map(|(id, group, detail)| {
            let (label, value) = describe_setting(config, id, 0);
            let control = match id {
                0 | 2 | 3 | 4 | 5 => {
                    let mut lower = *config;
                    adjust_config(&mut lower, id, -1);
                    let mut upper = *config;
                    adjust_config(&mut upper, id, 1);
                    Control::Number {
                        decrease: lower != *config,
                        increase: upper != *config,
                    }
                }
                6 | TOC_ROW => Control::Choice,
                _ => Control::Toggle(match id {
                    1 => config.show_title,
                    7 => config.show_frameline_title,
                    8 => config.show_hint_bar,
                    9 => config.arrows_page,
                    GLOBAL_HINT_ROW => config.show_global_hint,
                    BLANK_ROW => config.allow_page_blank,
                    _ => config.follow_tts,
                }),
            };
            let value = value.split('(').next().unwrap_or(&value);
            row(
                group,
                Key::Reader(id),
                label.trim_end_matches(':'),
                value,
                detail,
                control,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reading_fields_cover_shared_settings_once() {
        let rows = reading_rows(&ReaderDisplayConfig::default());
        let mut ids = rows
            .iter()
            .filter_map(|r| {
                if let Key::Reader(id) = r.key {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        ids.sort_unstable();
        assert_eq!(
            ids,
            (0..crate::pages::read_novel::settings::SETTING_ROWS.len()).collect::<Vec<_>>()
        );
    }
    #[test]
    fn stepper_bounds_come_from_the_shared_adjustment_rules() {
        let mut config = ReaderDisplayConfig {
            h_margin: 6,
            ..Default::default()
        };
        let row = reading_rows(&config)
            .into_iter()
            .find(|r| r.key == Key::Reader(5))
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
