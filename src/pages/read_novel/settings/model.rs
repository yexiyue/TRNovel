//! Reader preference metadata and edits, independent of modal state and persistence.
use crate::components::settings::{Control, Row};
use crate::{Frameline, ReaderDisplayConfig, TocWidth};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    Layout,
    Paging,
    Interface,
    #[cfg(feature = "tts")]
    Listening,
}
impl Group {
    pub const ALL: &'static [Self] = &[
        Self::Layout,
        Self::Paging,
        Self::Interface,
        #[cfg(feature = "tts")]
        Self::Listening,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Layout => "排版",
            Self::Paging => "翻页",
            Self::Interface => "界面",
            #[cfg(feature = "tts")]
            Self::Listening => "听书",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Setting {
    LineWidth,
    HorizontalMargin,
    FirstLineIndent,
    ParagraphGap,
    Border,
    TocWidth,
    ResetTocWidth,
    PageOverlap,
    ArrowPaging,
    PageBlank,
    Title,
    BorderTitle,
    ReaderHint,
    GlobalHint,
    #[cfg(feature = "tts")]
    FollowPlayback,
}
impl Setting {
    pub const ALL: &'static [Self] = &[
        Self::LineWidth,
        Self::HorizontalMargin,
        Self::FirstLineIndent,
        Self::ParagraphGap,
        Self::Border,
        Self::TocWidth,
        Self::ResetTocWidth,
        Self::PageOverlap,
        Self::ArrowPaging,
        Self::PageBlank,
        Self::Title,
        Self::BorderTitle,
        Self::ReaderHint,
        Self::GlobalHint,
        #[cfg(feature = "tts")]
        Self::FollowPlayback,
    ];
    fn metadata(self) -> (Group, &'static str, &'static str) {
        match self {
            Self::LineWidth => (Group::Layout, "每行字数", "0 表示跟随窗口宽度"),
            Self::HorizontalMargin => (Group::Layout, "左右留白", "正文左右各留白的终端列数"),
            Self::FirstLineIndent => (Group::Layout, "首行缩进", "只缩进段落第一行，不改原文"),
            Self::ParagraphGap => (Group::Layout, "段间空行", "段落之间额外补充的空行数"),
            Self::Border => (Group::Layout, "边框", "选择阅读正文边框样式"),
            Self::TocWidth => (
                Group::Layout,
                "目录占宽",
                "按百分点调整，也可拖动目录与正文之间的分隔条",
            ),
            Self::ResetTocWidth => (
                Group::Layout,
                "重置目录宽度",
                "恢复默认 50%，不改其它阅读偏好",
            ),
            Self::PageOverlap => (Group::Paging, "翻页重叠", "翻页时保留上一屏末尾的行数"),
            Self::ArrowPaging => (
                Group::Paging,
                "↑/↓ 翻页",
                "方向键按整页翻动，j / k 仍逐行移动",
            ),
            Self::PageBlank => (Group::Paging, "翻页留白", "关闭时末页贴底，不产生多余空白"),
            Self::Title => (
                Group::Interface,
                "显示标题",
                "控制界面标题，不删除 TXT 原文中的标题",
            ),
            Self::BorderTitle => (
                Group::Interface,
                "边框标题",
                "开启时将标题放在边框上，并显示边框听书状态",
            ),
            Self::ReaderHint => (
                Group::Interface,
                "框底提示",
                "显示阅读页提示与设置页操作说明",
            ),
            Self::GlobalHint => (
                Group::Interface,
                "页底提示",
                "显示其它页面最底部的返回、菜单与退出提示",
            ),
            #[cfg(feature = "tts")]
            Self::FollowPlayback => (Group::Listening, "跟随朗读", "跟随实际播放位置自动滚动正文"),
        }
    }
    pub fn group(self) -> Group {
        self.metadata().0
    }
    pub fn label(self) -> &'static str {
        self.metadata().1
    }
    pub fn row(self, config: &ReaderDisplayConfig) -> Row<Self> {
        let (group, label, detail) = self.metadata();
        let control = self.control(config);
        let on_off = |value: bool| if value { "开" } else { "关" }.to_owned();
        let value = match self {
            Self::LineWidth if config.lines_per_row == 0 => "跟随窗口".into(),
            Self::LineWidth => format!("{} 字", config.lines_per_row),
            Self::HorizontalMargin => format!("{} 列", config.h_margin),
            Self::FirstLineIndent => format!("{} 字", config.first_line_indent),
            Self::ParagraphGap => format!("{} 行", config.paragraph_gap),
            Self::PageOverlap => format!("{} 行", config.page_overlap),
            Self::Border => config.frameline.label().into(),
            Self::TocWidth => config.toc_width.label(),
            Self::ResetTocWidth => "恢复 50%".into(),
            Self::Title => on_off(config.show_title),
            Self::BorderTitle => on_off(config.show_frameline_title),
            Self::ReaderHint => on_off(config.show_hint_bar),
            Self::GlobalHint => on_off(config.show_global_hint),
            Self::ArrowPaging => on_off(config.arrows_page),
            Self::PageBlank => on_off(config.allow_page_blank),
            #[cfg(feature = "tts")]
            Self::FollowPlayback => on_off(config.follow_tts),
        };
        Row {
            swatch: None,
            key: self,
            group: group.label(),
            label: label.into(),
            detail: detail.into(),
            value,
            control,
        }
    }
    fn control(self, config: &ReaderDisplayConfig) -> Control {
        match self {
            Self::LineWidth
            | Self::HorizontalMargin
            | Self::FirstLineIndent
            | Self::ParagraphGap
            | Self::PageOverlap
            | Self::TocWidth => {
                let mut lower = *config;
                self.adjust(&mut lower, -1);
                let mut upper = *config;
                self.adjust(&mut upper, 1);
                Control::Number {
                    decrease: lower != *config,
                    increase: upper != *config,
                }
            }
            Self::Border => Control::Choice,
            Self::ResetTocWidth => Control::Action,
            Self::Title => Control::Toggle(config.show_title),
            Self::BorderTitle => Control::Toggle(config.show_frameline_title),
            Self::ReaderHint => Control::Toggle(config.show_hint_bar),
            Self::GlobalHint => Control::Toggle(config.show_global_hint),
            Self::ArrowPaging => Control::Toggle(config.arrows_page),
            Self::PageBlank => Control::Toggle(config.allow_page_blank),
            #[cfg(feature = "tts")]
            Self::FollowPlayback => Control::Toggle(config.follow_tts),
        }
    }
    pub fn choices(self, config: &ReaderDisplayConfig) -> Option<(Vec<String>, usize)> {
        match self {
            Self::Border => Some((
                Frameline::ALL.iter().map(|v| v.label().into()).collect(),
                Frameline::ALL
                    .iter()
                    .position(|v| *v == config.frameline)
                    .unwrap_or(0),
            )),
            _ => None,
        }
    }
    pub fn choose(self, config: &mut ReaderDisplayConfig, index: usize) {
        if self == Self::Border
            && let Some(value) = Frameline::ALL.get(index)
        {
            config.frameline = *value;
        }
    }
    pub fn activate(self, config: &mut ReaderDisplayConfig) {
        if self == Self::ResetTocWidth {
            config.toc_width = TocWidth::default();
        }
    }
    pub fn adjust(self, config: &mut ReaderDisplayConfig, delta: i32) {
        match self {
            Self::PageOverlap => {
                if delta > 0 {
                    config.increase_page_overlap();
                } else {
                    config.decrease_page_overlap();
                }
            }
            Self::ParagraphGap => {
                if delta > 0 {
                    config.increase_paragraph_gap();
                } else {
                    config.decrease_paragraph_gap();
                }
            }
            Self::FirstLineIndent => {
                if delta > 0 {
                    config.increase_first_line_indent();
                } else {
                    config.decrease_first_line_indent();
                }
            }
            Self::LineWidth => {
                if delta > 0 {
                    config.increase_lines_per_row();
                } else {
                    config.decrease_lines_per_row();
                }
            }
            Self::HorizontalMargin => {
                if delta > 0 {
                    config.increase_h_margin();
                } else {
                    config.decrease_h_margin();
                }
            }
            Self::Border => {
                config.frameline = if delta > 0 {
                    config.frameline.next()
                } else {
                    config.frameline.previous()
                }
            }
            Self::TocWidth => config.toc_width = config.toc_width.shift(delta.signum()),
            Self::ResetTocWidth => {}
            Self::Title => config.show_title = delta > 0,
            Self::BorderTitle => config.show_frameline_title = delta > 0,
            Self::ReaderHint => config.show_hint_bar = delta > 0,
            Self::GlobalHint => config.show_global_hint = delta > 0,
            Self::ArrowPaging => config.arrows_page = delta > 0,
            Self::PageBlank => config.allow_page_blank = delta > 0,
            #[cfg(feature = "tts")]
            Self::FollowPlayback => config.follow_tts = delta > 0,
        }
    }
}
pub(crate) fn rows(config: &ReaderDisplayConfig) -> Vec<Row<Setting>> {
    Setting::ALL
        .iter()
        .map(|setting| setting.row(config))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_bounds_follow_config_and_choices_apply_to_their_setting() {
        let mut config = ReaderDisplayConfig {
            h_margin: 6,
            ..Default::default()
        };
        assert_eq!(
            Setting::HorizontalMargin.row(&config).control,
            Control::Number {
                decrease: true,
                increase: false
            }
        );
        let (options, _) = Setting::Border.choices(&config).unwrap();
        Setting::Border.choose(&mut config, options.len() - 1);
        assert_eq!(config.frameline, Frameline::None);
    }
    #[test]
    fn toc_percentage_is_numeric_and_reset_only_changes_its_width() {
        let mut config = ReaderDisplayConfig {
            toc_width: TocWidth::from_percent(37.25),
            h_margin: 3,
            ..Default::default()
        };
        assert!(matches!(
            Setting::TocWidth.row(&config).control,
            Control::Number { .. }
        ));
        Setting::TocWidth.adjust(&mut config, 1);
        assert_eq!(config.toc_width.percent(), 38.25);
        Setting::ResetTocWidth.activate(&mut config);
        assert_eq!(config.toc_width, TocWidth::default());
        assert_eq!(config.h_margin, 3);
        let encoded = toml::to_string(&config).unwrap();
        assert!(encoded.contains("toc_width_percent = 50.0"));
        assert_eq!(
            toml::from_str::<ReaderDisplayConfig>(&encoded).unwrap(),
            config
        );
    }
}
