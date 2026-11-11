//! 中文阅读向配色预设。
//!
//! 上游的 `ThemeName` 是外部 crate 里的 `#[non_exhaustive]` 枚举, 下游既不能加变体,
//! 也不该 fork crate; 但配置里的 `theme_slug` 一直只是个字符串。于是这里另立一层
//! [`ReadingTheme`], slug 与 `ThemeName::slug()` 共用同一命名空间, 由 [`ThemeChoice`]
//! 统一分派: 命中预设走本项目色板, 否则回落外部主题 —— 老配置里的 slug 因此照旧可用。
//!
//! 预设针对"长时间读中文正文"取舍: 正文前景/背景对比度 ≥ 4.5, 背景避开纯白
//! (降低亮度刺激); 选中态取与背景反差足够大的色块, 再用 `readable_foreground`
//! 选黑/白前景, 保证浅色与深色主题下都看得清选中行与边框。

// `Palette` 来自外部 crate 且 `#[non_exhaustive]`, 下游无法用结构体字面量构造,
// 只能 `Default::default()` + 逐字段赋值 —— 这正是 lint 想建议的替代写法不存在的情形,
// 所以在整个模块里豁免, 而不是给 6 个色板函数各贴一遍。
#![allow(clippy::field_reassign_with_default)]

use ratatui::style::Color;
use ratatui_kit::Palette;
use ratatui_kit_themes::{IntoKitPalette, ThemeName};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

/// 自定义方案可循环选择的固定 16 色。
///
/// 终端里手敲 hex 既慢又容易打错, 所以 UI 只让用户在有限的色格里左右切换。
/// 色格同时覆盖浅底、深底与常见强调色, 保证不选 hex 也能配出可读的一套。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomColor {
    pub name: &'static str,
    pub color: Color,
}

pub const CUSTOM_COLORS: [CustomColor; 16] = [
    CustomColor {
        name: "墨黑",
        color: Color::Rgb(0, 0, 0),
    },
    CustomColor {
        name: "纯白",
        color: Color::Rgb(255, 255, 255),
    },
    CustomColor {
        name: "米黄",
        color: Color::Rgb(245, 238, 224),
    },
    CustomColor {
        name: "暖灰",
        color: Color::Rgb(222, 216, 206),
    },
    CustomColor {
        name: "豆沙绿",
        color: Color::Rgb(201, 228, 197),
    },
    CustomColor {
        name: "深夜蓝",
        color: Color::Rgb(16, 24, 38),
    },
    CustomColor {
        name: "石板青",
        color: Color::Rgb(29, 41, 56),
    },
    CustomColor {
        name: "暖白",
        color: Color::Rgb(230, 225, 214),
    },
    CustomColor {
        name: "深灰",
        color: Color::Rgb(46, 42, 37),
    },
    CustomColor {
        name: "中灰",
        color: Color::Rgb(122, 114, 104),
    },
    CustomColor {
        name: "琥珀",
        color: Color::Rgb(210, 168, 92),
    },
    CustomColor {
        name: "砖红",
        color: Color::Rgb(158, 55, 43),
    },
    CustomColor {
        name: "松绿",
        color: Color::Rgb(46, 107, 60),
    },
    CustomColor {
        name: "天蓝",
        color: Color::Rgb(111, 168, 220),
    },
    CustomColor {
        name: "紫灰",
        color: Color::Rgb(107, 91, 123),
    },
    CustomColor {
        name: "粉橙",
        color: Color::Rgb(224, 138, 95),
    },
];

/// 一套配色里所有可单独覆盖的字段, 顺序与 Palette 的声明一致。
///
/// UI、校验与文档都从这张表取字段名和说明, 避免三处各写一份而漂移。
/// key 必须与 Palette 的字段名逐字相同 —— 配置里的 colors 用它做键。
#[derive(Debug, Clone, Copy)]
pub struct PaletteField {
    /// Palette 字段名, 也是配置里 colors 的键。
    pub key: &'static str,
    /// 设置页展示的中文标签。
    pub label: &'static str,
    /// 这个颜色在界面上的实际用途。
    pub usage: &'static str,
}

pub const PALETTE_FIELDS: [PaletteField; 15] = [
    PaletteField {
        key: "fg",
        label: "正文",
        usage: "阅读页正文与目录条目文字",
    },
    PaletteField {
        key: "fg_dim",
        label: "次要文字",
        usage: "页脚状态栏、快捷键提示与标签",
    },
    PaletteField {
        key: "bg",
        label: "背景",
        usage: "阅读页与列表的整屏背景",
    },
    PaletteField {
        key: "surface",
        label: "面板",
        usage: "输入框、面板等容器的抬升底色",
    },
    PaletteField {
        key: "overlay",
        label: "遮罩",
        usage: "模态弹窗背后的遮罩",
    },
    PaletteField {
        key: "accent",
        label: "强调",
        usage: "标题、章节名与高亮符号",
    },
    PaletteField {
        key: "on_accent",
        label: "强调前景",
        usage: "选中行等强调底色上的文字",
    },
    PaletteField {
        key: "selection",
        label: "选中行",
        usage: "列表当前选中行的背景",
    },
    PaletteField {
        key: "border",
        label: "边框",
        usage: "阅读页外框、列表与浮层边框",
    },
    PaletteField {
        key: "border_active",
        label: "聚焦边框",
        usage: "当前聚焦控件与弹窗边框",
    },
    PaletteField {
        key: "success",
        label: "成功",
        usage: "成功提示与听书朗读高亮",
    },
    PaletteField {
        key: "warning",
        label: "警告",
        usage: "警告提示",
    },
    PaletteField {
        key: "error",
        label: "错误",
        usage: "错误提示",
    },
    PaletteField {
        key: "info",
        label: "信息",
        usage: "一般信息提示",
    },
    PaletteField {
        key: "placeholder",
        label: "占位",
        usage: "空列表提示与输入占位",
    },
];

/// 按字段名读取色板颜色; 未知字段名返回 None。
pub fn palette_field(palette: &Palette, key: &str) -> Option<Color> {
    Some(match key {
        "fg" => palette.fg,
        "fg_dim" => palette.fg_dim,
        "bg" => palette.bg,
        "surface" => palette.surface,
        "overlay" => palette.overlay,
        "accent" => palette.accent,
        "on_accent" => palette.on_accent,
        "selection" => palette.selection,
        "border" => palette.border,
        "border_active" => palette.border_active,
        "success" => palette.success,
        "warning" => palette.warning,
        "error" => palette.error,
        "info" => palette.info,
        "placeholder" => palette.placeholder,
        _ => return None,
    })
}

/// 按字段名取得色板颜色的可变引用; 未知字段名返回 None。
fn palette_field_mut<'a>(palette: &'a mut Palette, key: &str) -> Option<&'a mut Color> {
    Some(match key {
        "fg" => &mut palette.fg,
        "fg_dim" => &mut palette.fg_dim,
        "bg" => &mut palette.bg,
        "surface" => &mut palette.surface,
        "overlay" => &mut palette.overlay,
        "accent" => &mut palette.accent,
        "on_accent" => &mut palette.on_accent,
        "selection" => &mut palette.selection,
        "border" => &mut palette.border,
        "border_active" => &mut palette.border_active,
        "success" => &mut palette.success,
        "warning" => &mut palette.warning,
        "error" => &mut palette.error,
        "info" => &mut palette.info,
        "placeholder" => &mut palette.placeholder,
        _ => return None,
    })
}

/// 把配置里的 `#rrggbb` 解析成终端 RGB; 只接受这一种格式。
///
/// 严格解析而不是接受 `#rgb`/颜色名: 写盘与回读必须是同一种表示, 否则
/// 「循环色格」与手工改配置会得到肉眼看不到的差别。
pub fn parse_hex_color(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    Some(Color::Rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// `parse_hex_color` 的反向操作; 非 RGB 色没有稳定的文本表示, 返回 `None`。
pub fn color_to_hex(color: Color) -> Option<String> {
    let Color::Rgb(r, g, b) = color else {
        return None;
    };
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// 从任意色板色找最接近的固定色格, 用于新建方案时预填当前主题的观感。
pub fn nearest_custom_color(color: Color) -> usize {
    let Color::Rgb(r, g, b) = color else {
        return 0;
    };
    CUSTOM_COLORS
        .iter()
        .enumerate()
        .min_by_key(|(_, entry)| {
            let Color::Rgb(er, eg, eb) = entry.color else {
                return u32::MAX;
            };
            let distance = |a: u8, b: u8| {
                let delta = i32::from(a) - i32::from(b);
                (delta * delta) as u32
            };
            distance(r, er) + distance(g, eg) + distance(b, eb)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

/// 用户创建的一套配色, 持久化在 [appearance].custom_themes。
///
/// colors 以 PALETTE_FIELDS 的字段名为键, 只存与 base 不同的颜色; 未填字段
/// 解析时从 base 继承 —— 以后调整预设计算逻辑时, 自定义方案会自动跟着改善,
/// 而不是冻结旧色板。
///
/// bg/fg/accent 是旧版格式的遗留字段, 只为读取旧配置保留; 新写入的方案不再
/// 产出这三个字段, 解析时 colors 优先于它们。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomTheme {
    /// 设置页展示的中文名。
    pub name: String,
    /// 写进配置的稳定标识, 形如 custom-1。
    pub slug: String,
    /// 颜色所继承的主题 slug(预设或内置主题)。
    pub base: String,
    /// 旧版底色; 仅用于兼容旧配置。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bg: String,
    /// 旧版正文色; 仅用于兼容旧配置。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fg: String,
    /// 旧版强调色; 仅用于兼容旧配置。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub accent: String,
    /// Palette 字段名 -> #rrggbb; 只覆盖与 base 不同的字段。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
}

impl CustomTheme {
    /// 生成一个不与预设、内置主题及已有自定义方案冲突的 slug。
    ///
    /// 不用「数量 + 1」: 删掉中间一项后数量会回退, 新 slug 可能撞上仍存在的旧项。
    /// 从 1 开始找第一个空位, 保证重启后配置里的 slug 仍唯一。
    pub fn next_slug(existing: &[Self]) -> String {
        (1u32..)
            .map(|n| format!("custom-{n}"))
            .find(|slug| {
                !existing.iter().any(|theme| &theme.slug == slug)
                    && ThemeChoice::from_slug(slug).is_none()
            })
            .expect("u32 范围内总能找到空闲 slug")
    }

    /// 以 base 的完整色板为底, 逐字段覆盖 PALETTE_FIELDS 中配置过的颜色。
    ///
    /// 未知 base(手改配置、删掉的预设)回落默认主题, 而不是 panic 或返回全 Reset;
    /// 无效 hex 与未知字段名同样静默忽略, 让一份脏配置仍能启动。旧版的
    /// bg/fg/accent 先应用, colors 中同名键再覆盖它们。
    pub fn palette(&self) -> Palette {
        let mut palette = ThemeChoice::from_slug(&self.base)
            .map(ThemeChoice::palette)
            .unwrap_or_else(|| ThemeName::TokyoNight.into_kit_palette());

        let mut has_on_accent = false;
        let mut has_border_active = false;
        {
            let mut apply = |palette: &mut Palette, key: &str, hex: &str| {
                let Some(color) = parse_hex_color(hex) else {
                    return;
                };
                let Some(slot) = palette_field_mut(palette, key) else {
                    return;
                };
                *slot = color;
                if key == "on_accent" {
                    has_on_accent = true;
                }
                if key == "border_active" {
                    has_border_active = true;
                }
            };
            apply(&mut palette, "bg", &self.bg);
            apply(&mut palette, "fg", &self.fg);
            apply(&mut palette, "accent", &self.accent);
            for (key, hex) in &self.colors {
                apply(&mut palette, key, hex);
            }
        }

        // on_accent 与 border_active 原本是从 accent 派生的语义色: 用户没有显式
        // 配置时仍按强调色重算; 显式配置了就以用户的取色为准。
        if !has_on_accent {
            palette.on_accent = readable_foreground(&[palette.accent, palette.selection]);
        }
        if !has_border_active {
            palette.border_active = palette.accent;
        }
        palette
    }
}

/// 设置页统一的列表项: 预设、内置主题、用户方案。
///
/// 与 [`ThemeChoice`] 分开是因为自定义方案含 `String`, 无法保持 `Copy`;
/// 而配色解析仍走 [`ThemeChoice`], 两处职责不混。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeEntry {
    Preset(ReadingTheme),
    Builtin(ThemeName),
    Custom(CustomTheme),
}

impl ThemeEntry {
    /// 顺序固定为「预设 → 内置 → 自定义」, 用户方案追加在既有主题之后。
    pub fn all(custom: &[CustomTheme]) -> Vec<Self> {
        ThemeChoice::all()
            .into_iter()
            .map(|choice| match choice {
                ThemeChoice::Preset(theme) => Self::Preset(theme),
                ThemeChoice::Builtin(name) => Self::Builtin(name),
            })
            .chain(custom.iter().cloned().map(Self::Custom))
            .collect()
    }

    pub fn slug(&self) -> &str {
        match self {
            Self::Preset(theme) => theme.slug(),
            Self::Builtin(name) => name.slug(),
            Self::Custom(theme) => &theme.slug,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Preset(theme) => theme.display_name(),
            Self::Builtin(name) => name.display_name(),
            Self::Custom(theme) => &theme.name,
        }
    }

    pub fn palette(&self) -> Palette {
        match self {
            Self::Preset(theme) => theme.palette(),
            Self::Builtin(name) => name.into_kit_palette(),
            Self::Custom(theme) => theme.palette(),
        }
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
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

    /// 配置里存的是 `#rrggbb`: 解析与回写必须严格往返, 否则用户保存后重启,
    /// 颜色会悄悄变一个值。
    #[test]
    fn hex_colors_round_trip_through_config_text() {
        for color in [
            Color::Rgb(0x00, 0x00, 0x00),
            Color::Rgb(0xff, 0xff, 0xff),
            Color::Rgb(0xf5, 0xee, 0xe0),
            Color::Rgb(0x12, 0xab, 0x7d),
        ] {
            let hex = color_to_hex(color).unwrap();
            assert_eq!(hex.len(), 7);
            assert_eq!(parse_hex_color(&hex), Some(color));
        }

        // 非法输入只返回 None, 由调用方保留 base 颜色, 不 panic。
        for invalid in ["", "#fff", "#gggggg", "f5eee0", "#f5eee00"] {
            assert_eq!(parse_hex_color(invalid), None, "{invalid} 不该被接受");
        }
        assert_eq!(color_to_hex(Color::Reset), None);
    }

    /// 新建方案时用当前主题的色板预填最近色格, 让"从现有配色出发"真的省事。
    #[test]
    fn nearest_custom_color_finds_exact_palette_member() {
        for (index, entry) in CUSTOM_COLORS.iter().enumerate() {
            assert_eq!(nearest_custom_color(entry.color), index);
        }
        // 非 RGB 色没有距离可言, 回落到第一个色格而不是越界。
        assert_eq!(nearest_custom_color(Color::Reset), 0);
    }

    /// 列表顺序是设置页的分组依据: 自定义方案必须排在所有既有主题之后。
    #[test]
    fn theme_entries_append_custom_themes_after_builtin_ones() {
        let custom = vec![CustomTheme {
            name: "我的夜间".to_string(),
            slug: "custom-1".to_string(),
            base: "midnight-blue".to_string(),
            colors: BTreeMap::new(),
            bg: "#101826".to_string(),
            fg: "#e6e1d6".to_string(),
            accent: "#d2a85c".to_string(),
        }];
        let entries = ThemeEntry::all(&custom);
        assert_eq!(entries.len(), ThemeChoice::all().len() + 1);
        assert!(entries.last().unwrap().is_custom());
        assert_eq!(entries.last().unwrap().display_name(), "我的夜间");
        assert_eq!(entries.last().unwrap().slug(), "custom-1");
        // 自定义项按 base 解析后再覆盖, 与 AppearanceConfig::palette 保持一致。
        assert_eq!(
            entries.last().unwrap().palette().bg,
            parse_hex_color("#101826").unwrap()
        );
    }

    /// 旧版自定义方案只有 bg/fg/accent, 也必须能解析, 且未写的字段继承 base。
    #[test]
    fn legacy_custom_theme_json_still_parses_and_falls_back() {
        let theme: CustomTheme = serde_json::from_str(
            r##"{"name":"旧方案","slug":"custom-1","base":"rice-paper","bg":"#000000","fg":"#ffffff","accent":"#ff0000"}"##,
        )
        .unwrap();
        assert!(theme.colors.is_empty());

        let palette = theme.palette();
        let base = ThemeChoice::from_slug("rice-paper").unwrap().palette();
        assert_eq!(palette.bg, Color::Rgb(0, 0, 0));
        assert_eq!(palette.fg, Color::Rgb(255, 255, 255));
        assert_eq!(palette.accent, Color::Rgb(255, 0, 0));
        // 旧配置没写的字段仍继承 base, 不会被默认色板冲掉。
        assert_eq!(palette.surface, base.surface);
        assert_eq!(palette.selection, base.selection);
    }

    /// colors 里不认识的字段名不参与渲染, 也不能让整份配置解析失败。
    #[test]
    fn unknown_color_fields_are_ignored() {
        let theme: CustomTheme = serde_json::from_str(
            r##"{"base":"ink-black","colors":{"bg":"#123456","no_such_field":"#ffffff"}}"##,
        )
        .unwrap();
        assert_eq!(
            theme.colors.get("no_such_field").map(String::as_str),
            Some("#ffffff")
        );

        let palette = theme.palette();
        let base = ThemeChoice::from_slug("ink-black").unwrap().palette();
        assert_eq!(palette.bg, Color::Rgb(0x12, 0x34, 0x56));
        assert_eq!(palette.surface, base.surface);
    }

    /// Palette 的每个字段都应能被覆盖; 非法 hex 静默回落 base, 不 panic。
    #[test]
    fn every_palette_field_can_be_overridden_and_invalid_hex_falls_back() {
        let base = ThemeChoice::from_slug("warm-gray").unwrap().palette();
        let mut colors: BTreeMap<String, String> = PALETTE_FIELDS
            .iter()
            .map(|field| (field.key.to_string(), "#123456".to_string()))
            .collect();
        colors.insert("bg".to_string(), "not-a-color".to_string());

        let theme = CustomTheme {
            base: "warm-gray".to_string(),
            colors,
            ..CustomTheme::default()
        };
        let palette = theme.palette();
        assert_eq!(palette.bg, base.bg, "非法颜色应回落 base");
        for field in PALETTE_FIELDS {
            let actual = palette_field(&palette, field.key).unwrap();
            if field.key == "bg" {
                assert_eq!(actual, base.bg);
            } else {
                assert_eq!(actual, Color::Rgb(0x12, 0x34, 0x56), "{} 未生效", field.key);
            }
        }
    }

    /// 字段表必须覆盖 Palette 全部字段, 且每个字段都有中文标签与使用场景。
    #[test]
    fn palette_field_table_covers_every_palette_field() {
        let palette = ThemeName::TokyoNight.into_kit_palette();
        let mut keys: Vec<&str> = PALETTE_FIELDS.iter().map(|field| field.key).collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "字段名有重复");
        assert_eq!(total, 15, "Palette 共有 15 个字段");

        for field in PALETTE_FIELDS {
            assert!(palette_field(&palette, field.key).is_some());
            assert!(!field.label.is_empty(), "{} 缺少中文标签", field.key);
            assert!(!field.usage.is_empty(), "{} 缺少使用场景", field.key);
        }
    }

    /// 同一字段同时出现在旧字段与 colors 时, 以新的 colors 为准。
    #[test]
    fn colors_take_precedence_over_legacy_fields() {
        let mut colors = BTreeMap::new();
        colors.insert("bg".to_string(), "#ffffff".to_string());
        let theme = CustomTheme {
            base: "ink-black".to_string(),
            bg: "#000000".to_string(),
            colors,
            ..CustomTheme::default()
        };
        assert_eq!(theme.palette().bg, Color::Rgb(255, 255, 255));
    }
}
