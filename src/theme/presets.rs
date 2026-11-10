//! 中文阅读向配色预设。
//!
//! 上游的 `ThemeName` 是外部 crate 里的 `#[non_exhaustive]` 枚举, 下游既不能加变体,
//! 也不该 fork crate; 但配置里的 `theme_slug` 一直只是个字符串。于是这里另立一层
//! [`ReadingTheme`], slug 与 `ThemeName::slug()` 共用同一命名空间, 由 [`ThemeChoice`]
//! 统一分派: 命中预设走本项目色板, 否则回落外部主题 —— 老配置里的 slug 因此照旧可用。
//!
//! 预设针对"长时间读中文正文"取舍: 正文前景/背景对比度 ≥ 4.5, 背景避开纯白
//! (降低亮度刺激); 选中态取与背景反差足够大的色块, 再用 [`readable_foreground`]
//! 选黑/白前景, 保证浅色与深色主题下都看得清选中行与边框。

// `Palette` 来自外部 crate 且 `#[non_exhaustive]`, 下游无法用结构体字面量构造,
// 只能 `Default::default()` + 逐字段赋值 —— 这正是 lint 想建议的替代写法不存在的情形,
// 所以在整个模块里豁免, 而不是给 6 个色板函数各贴一遍。
#![allow(clippy::field_reassign_with_default)]

use ratatui::style::Color;
use ratatui_kit::Palette;
use ratatui_kit_themes::{IntoKitPalette, ThemeName};

/// 一套本项目的阅读主题: 稳定 slug(写入 config.toml)+ 中文名 + 完整语义色板。
///
/// 色板存函数指针而非 `Palette` 值: `Palette` 是 `#[non_exhaustive]` 且无法 const
/// 构造, fn 指针让 [`ReadingTheme`] 保持 `Copy`, 又不必把 6 套色板都塞进常量。
#[derive(Debug, Clone, Copy)]
pub struct ReadingTheme {
    slug: &'static str,
    name: &'static str,
    palette: fn() -> Palette,
}

impl ReadingTheme {
    /// 列表展示顺序: 先两套浅色护眼, 再两套深色, 最后高对比与暖灰。
    pub const ALL: [Self; 6] = [
        Self {
            slug: "rice-paper",
            name: "米黄纸",
            palette: rice_paper,
        },
        Self {
            slug: "bean-green",
            name: "豆沙绿",
            palette: bean_green,
        },
        Self {
            slug: "ink-black",
            name: "墨黑",
            palette: ink_black,
        },
        Self {
            slug: "midnight-blue",
            name: "深夜蓝",
            palette: midnight_blue,
        },
        Self {
            slug: "high-contrast",
            name: "高对比",
            palette: high_contrast,
        },
        Self {
            slug: "warm-gray",
            name: "暖灰",
            palette: warm_gray,
        },
    ];

    /// 写入 config.toml 的稳定标识; 发布后不可改名, 否则用户配置失效。
    pub const fn slug(self) -> &'static str {
        self.slug
    }

    /// 设置页展示的中文名。
    pub const fn display_name(self) -> &'static str {
        self.name
    }

    pub fn palette(self) -> Palette {
        (self.palette)()
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|theme| theme.slug == slug)
    }
}

impl PartialEq for ReadingTheme {
    /// 按 slug 比较: 预设是编译期常量, 比函数指针地址更直观也更稳定。
    fn eq(&self, other: &Self) -> bool {
        self.slug == other.slug
    }
}

impl Eq for ReadingTheme {}

/// 主题列表项: 6 套阅读预设 + 上游全部内置主题, 共用同一套选中/持久化逻辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    Preset(ReadingTheme),
    Builtin(ThemeName),
}

impl ThemeChoice {
    /// 预设排最前(这次改造的主推), 内置主题保持上游顺序垫后。
    pub fn all() -> Vec<Self> {
        ReadingTheme::ALL
            .iter()
            .copied()
            .map(Self::Preset)
            .chain(ThemeName::all().iter().copied().map(Self::Builtin))
            .collect()
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Preset(theme) => theme.slug(),
            Self::Builtin(name) => name.slug(),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Preset(theme) => theme.display_name(),
            Self::Builtin(name) => name.display_name(),
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Preset(theme) => theme.palette(),
            Self::Builtin(name) => name.into_kit_palette(),
        }
    }

    /// slug 解析: 先查预设, 再按 slug 精确匹配内置主题; 未知 slug 返回 `None`,
    /// 由调用方兜底。不用 `ThemeName::from_str` —— 它接受 "tokyo" 之类别名,
    /// 会让主题页的「当前」标记与配置文件里的值漂移。
    pub fn from_slug(slug: &str) -> Option<Self> {
        ReadingTheme::from_slug(slug).map(Self::Preset).or_else(|| {
            ThemeName::all()
                .iter()
                .copied()
                .find(|name| name.slug() == slug)
                .map(Self::Builtin)
        })
    }
}

/// 在若干"会被当作底色"的颜色里挑黑或白, 取最差情况更好的那个。
///
/// 沿用上游 `ratatui-kit-themes` 的契约: `on_accent` 同时叠在 `accent`(输入光标)
/// 与 `selection`(选中行)上, 所以必须两者都能读, 而不是只看其中一个。
fn readable_foreground(backgrounds: &[Color]) -> Color {
    let black_worst = backgrounds
        .iter()
        .map(|&bg| contrast_ratio(Color::Rgb(0, 0, 0), bg))
        .fold(f32::INFINITY, f32::min);
    let white_worst = backgrounds
        .iter()
        .map(|&bg| contrast_ratio(Color::Rgb(255, 255, 255), bg))
        .fold(f32::INFINITY, f32::min);
    // 用显式 RGB 而非 `Color::Black/White`: 预设色板要么全是 RGB, 要么全不是,
    // 混入命名色会让对比度校验和终端渲染都出现"半套颜色"的偏差。
    if black_worst >= white_worst {
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(255, 255, 255)
    }
}

/// WCAG 相对亮度; 单测用它验证对比度。非 RGB 色(如终端 `Reset`)给 `None`。
fn relative_luminance(color: Color) -> Option<f32> {
    let Color::Rgb(r, g, b) = color else {
        return None;
    };
    let [r, g, b] = [r, g, b].map(|channel| {
        let channel = f32::from(channel) / 255.0;
        if channel <= 0.03928 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b)
}

fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (Some(la), Some(lb)) = (relative_luminance(a), relative_luminance(b)) else {
        return 1.0;
    };
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

/// 米黄纸: 仿旧书的暖米色纸张, 正文近墨色。亮度刺激低于纯白, 适合白天长时间读。
fn rice_paper() -> Palette {
    let mut palette = Palette::default();
    palette.bg = Color::Rgb(245, 238, 224);
    palette.surface = Color::Rgb(238, 229, 211);
    palette.overlay = Color::Rgb(229, 218, 197);
    palette.fg = Color::Rgb(58, 50, 38);
    palette.fg_dim = Color::Rgb(122, 110, 92);
    palette.accent = Color::Rgb(122, 74, 34);
    palette.selection = Color::Rgb(138, 100, 64);
    palette.on_accent = readable_foreground(&[palette.accent, palette.selection]);
    palette.border = Color::Rgb(194, 175, 134);
    palette.border_active = palette.accent;
    palette.success = Color::Rgb(74, 110, 55);
    palette.warning = Color::Rgb(150, 101, 25);
    palette.error = Color::Rgb(158, 55, 43);
    palette.info = Color::Rgb(54, 101, 132);
    palette.placeholder = Color::Rgb(150, 138, 118);
    palette
}

/// 豆沙绿: 经典护眼绿, 冷色背景配深灰绿正文, 色相低饱和避免长时间读后发涩。
fn bean_green() -> Palette {
    let mut palette = Palette::default();
    palette.bg = Color::Rgb(201, 228, 197);
    palette.surface = Color::Rgb(188, 217, 184);
    palette.overlay = Color::Rgb(176, 206, 171);
    palette.fg = Color::Rgb(31, 44, 28);
    palette.fg_dim = Color::Rgb(74, 94, 72);
    palette.accent = Color::Rgb(46, 107, 60);
    palette.selection = Color::Rgb(58, 100, 66);
    palette.on_accent = readable_foreground(&[palette.accent, palette.selection]);
    palette.border = Color::Rgb(143, 176, 139);
    palette.border_active = palette.accent;
    palette.success = Color::Rgb(47, 122, 58);
    palette.warning = Color::Rgb(138, 104, 18);
    palette.error = Color::Rgb(163, 58, 46);
    palette.info = Color::Rgb(46, 96, 118);
    palette.placeholder = Color::Rgb(108, 129, 104);
    palette
}

/// 墨黑: 纯黑底 + 暖白正文, 夜间/OLED 屏最省眼; 强调色用暖琥珀, 不刺眼。
fn ink_black() -> Palette {
    let mut palette = Palette::default();
    palette.bg = Color::Rgb(0, 0, 0);
    palette.surface = Color::Rgb(13, 13, 13);
    palette.overlay = Color::Rgb(22, 22, 22);
    palette.fg = Color::Rgb(230, 225, 214);
    palette.fg_dim = Color::Rgb(139, 133, 122);
    palette.accent = Color::Rgb(210, 168, 92);
    palette.selection = Color::Rgb(140, 114, 66);
    palette.on_accent = readable_foreground(&[palette.accent, palette.selection]);
    palette.border = Color::Rgb(59, 59, 59);
    palette.border_active = palette.accent;
    palette.success = Color::Rgb(111, 191, 115);
    palette.warning = Color::Rgb(224, 179, 65);
    palette.error = Color::Rgb(224, 108, 96);
    palette.info = Color::Rgb(111, 168, 208);
    palette.placeholder = Color::Rgb(110, 106, 98);
    palette
}

/// 深夜蓝: 低亮度深蓝底, 正文偏冷白; 蓝底比纯黑柔和, 适合睡前读。
fn midnight_blue() -> Palette {
    let mut palette = Palette::default();
    palette.bg = Color::Rgb(16, 24, 38);
    palette.surface = Color::Rgb(22, 32, 47);
    palette.overlay = Color::Rgb(29, 41, 56);
    palette.fg = Color::Rgb(201, 214, 228);
    palette.fg_dim = Color::Rgb(124, 140, 161);
    palette.accent = Color::Rgb(111, 168, 220);
    palette.selection = Color::Rgb(110, 150, 196);
    palette.on_accent = readable_foreground(&[palette.accent, palette.selection]);
    palette.border = Color::Rgb(51, 71, 95);
    palette.border_active = palette.accent;
    palette.success = Color::Rgb(116, 184, 122);
    palette.warning = Color::Rgb(217, 178, 95);
    palette.error = Color::Rgb(224, 115, 106);
    palette.info = Color::Rgb(111, 168, 220);
    palette.placeholder = Color::Rgb(98, 116, 138);
    palette
}

/// 高对比: 纯黑白 + 高饱和强调色, 给弱视或在强光下用终端的人; 不追求柔和。
fn high_contrast() -> Palette {
    let mut palette = Palette::default();
    palette.bg = Color::Rgb(0, 0, 0);
    palette.surface = Color::Rgb(10, 10, 10);
    palette.overlay = Color::Rgb(22, 22, 22);
    palette.fg = Color::Rgb(255, 255, 255);
    palette.fg_dim = Color::Rgb(200, 200, 200);
    palette.accent = Color::Rgb(0, 229, 255);
    palette.selection = Color::Rgb(0, 190, 215);
    palette.on_accent = readable_foreground(&[palette.accent, palette.selection]);
    palette.border = Color::Rgb(122, 122, 122);
    palette.border_active = palette.accent;
    palette.success = Color::Rgb(0, 230, 118);
    palette.warning = Color::Rgb(255, 214, 0);
    palette.error = Color::Rgb(255, 82, 82);
    palette.info = Color::Rgb(64, 196, 255);
    palette.placeholder = Color::Rgb(158, 158, 158);
    palette
}

/// 暖灰: 中性偏暖的浅灰, 比米黄纸更素, 长时间看屏幕不累眼。
fn warm_gray() -> Palette {
    let mut palette = Palette::default();
    palette.bg = Color::Rgb(222, 216, 206);
    palette.surface = Color::Rgb(213, 206, 194);
    palette.overlay = Color::Rgb(203, 195, 182);
    palette.fg = Color::Rgb(46, 42, 37);
    palette.fg_dim = Color::Rgb(107, 99, 89);
    palette.accent = Color::Rgb(122, 78, 56);
    palette.selection = Color::Rgb(134, 96, 74);
    palette.on_accent = readable_foreground(&[palette.accent, palette.selection]);
    palette.border = Color::Rgb(168, 154, 133);
    palette.border_active = palette.accent;
    palette.success = Color::Rgb(78, 122, 69);
    palette.warning = Color::Rgb(152, 108, 30);
    palette.error = Color::Rgb(163, 65, 55);
    palette.info = Color::Rgb(69, 104, 127);
    palette.placeholder = Color::Rgb(138, 129, 117);
    palette
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppearanceConfig;

    /// `Palette` 默认值全是终端 `Reset`, 漏填任何一栏都会在界面上退化成终端本色
    /// —— 这正是"只换背景色"的假主题, 所以逐字段断言。
    #[test]
    fn every_preset_fills_all_palette_fields() {
        for theme in ReadingTheme::ALL {
            let palette = theme.palette();
            let fields = [
                ("fg", palette.fg),
                ("fg_dim", palette.fg_dim),
                ("bg", palette.bg),
                ("surface", palette.surface),
                ("overlay", palette.overlay),
                ("accent", palette.accent),
                ("on_accent", palette.on_accent),
                ("selection", palette.selection),
                ("border", palette.border),
                ("border_active", palette.border_active),
                ("success", palette.success),
                ("warning", palette.warning),
                ("error", palette.error),
                ("info", palette.info),
                ("placeholder", palette.placeholder),
            ];
            for (name, color) in fields {
                assert_ne!(
                    color,
                    Color::Reset,
                    "{} 的 {name} 未填写, 会退回终端本色",
                    theme.slug()
                );
            }
            assert_ne!(
                palette,
                Palette::default(),
                "{} 与默认色板等价",
                theme.slug()
            );
            assert_ne!(palette.fg, palette.bg, "{} 前景与背景同色", theme.slug());
        }
    }

    /// 预设的意义就是"读得清": 正文 4.5:1(WCAG AA), 选中态与边框也必须和底色分开,
    /// 否则浅色/深色主题下选中行会糊成一片。
    #[test]
    fn presets_keep_body_selection_and_border_readable() {
        for theme in ReadingTheme::ALL {
            let palette = theme.palette();
            let slug = theme.slug();
            assert!(
                contrast_ratio(palette.fg, palette.bg) >= 4.5,
                "{slug} 正文对比度不足: {:.2}",
                contrast_ratio(palette.fg, palette.bg)
            );
            assert!(
                contrast_ratio(palette.on_accent, palette.accent) >= 4.0,
                "{slug} 强调色上的前景不可读: {:.2}",
                contrast_ratio(palette.on_accent, palette.accent)
            );
            assert!(
                contrast_ratio(palette.on_accent, palette.selection) >= 4.0,
                "{slug} 选中行上的前景不可读: {:.2}",
                contrast_ratio(palette.on_accent, palette.selection)
            );
            assert!(
                contrast_ratio(palette.border, palette.bg) >= 1.5,
                "{slug} 边框与背景分不开: {:.2}",
                contrast_ratio(palette.border, palette.bg)
            );
        }
    }

    /// slug 是写进 config.toml 的契约: 必须唯一, 且不能与内置主题撞名,
    /// 否则解析时预设会悄悄覆盖用户原来的主题。
    #[test]
    fn preset_slugs_are_unique_and_do_not_shadow_builtin_themes() {
        let mut slugs: Vec<&str> = ReadingTheme::ALL.iter().map(|theme| theme.slug()).collect();
        let total = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), total, "预设 slug 有重复");

        for theme in ReadingTheme::ALL {
            assert!(
                !ThemeName::all()
                    .iter()
                    .any(|name| name.slug() == theme.slug()),
                "{} 与内置主题 slug 冲突",
                theme.slug()
            );
            assert_eq!(ReadingTheme::from_slug(theme.slug()), Some(theme));
        }
    }

    /// 主题 slug 经 TOML 落盘再读回必须原样保留(设置页写的就是它)。
    #[test]
    fn preset_slugs_round_trip_through_appearance_config() {
        for theme in ReadingTheme::ALL {
            let config = AppearanceConfig {
                theme_slug: theme.slug().to_string(),
                ..Default::default()
            };
            let encoded = toml::to_string(&config).unwrap();
            let decoded: AppearanceConfig = toml::from_str(&encoded).unwrap();

            assert_eq!(decoded.theme_slug, theme.slug());
            assert_eq!(
                ThemeChoice::from_slug(&decoded.theme_slug),
                Some(ThemeChoice::Preset(theme))
            );
        }
    }

    /// 向后兼容: 老配置里的内置主题 slug 仍在列表里, 解析出的色板与上游一致;
    /// 完全没见过的 slug 返回 `None`, 由调用方兜底而不是 panic。
    #[test]
    fn legacy_theme_slugs_still_resolve() {
        assert_eq!(
            ThemeChoice::from_slug("tokyo-night"),
            Some(ThemeChoice::Builtin(ThemeName::TokyoNight))
        );
        assert_eq!(
            ThemeChoice::from_slug("dracula").map(ThemeChoice::palette),
            Some(ThemeName::Dracula.into_kit_palette())
        );
        assert_eq!(ThemeChoice::from_slug("no-such-theme"), None);
    }

    /// 设置页显示中文名, 而不是只有 slug。
    #[test]
    fn theme_list_shows_chinese_names_for_presets() {
        let names: Vec<&str> = ReadingTheme::ALL
            .iter()
            .map(|theme| theme.display_name())
            .collect();
        assert_eq!(
            names,
            ["米黄纸", "豆沙绿", "墨黑", "深夜蓝", "高对比", "暖灰"]
        );

        let all = ThemeChoice::all();
        assert_eq!(
            all.len(),
            ReadingTheme::ALL.len() + ThemeName::all().len(),
            "预设与内置主题应全部列出"
        );
        let mut slugs: Vec<&str> = all.iter().map(|choice| choice.slug()).collect();
        let total = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), total, "整个主题列表里 slug 必须唯一");
    }
}
