use crate::Result;
use crate::config::ConfigStore;
use crate::theme::presets::{CustomTheme, ThemeChoice};
use ratatui_kit::Palette;
use ratatui_kit_themes::{IntoKitPalette, ThemeName, terminal_background};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct AppearanceConfig {
    pub theme_slug: String,
    pub background: BackgroundMode,
    pub show_header: bool,
    /// 退出前是否弹确认框。Q 紧挨着翻页键,误触就要重新找进度,因此默认开;
    /// 习惯一键退出的用户可在设置里关掉。
    #[serde(default = "default_confirm_exit")]
    pub confirm_exit: bool,
    /// 用户在主题设置页创建的方案。旧配置没有这一项时为空,不影响既有主题解析。
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMode {
    Theme,
    Terminal,
}

impl AppearanceConfig {
    const DEFAULT_THEME: ThemeName = ThemeName::TokyoNight;

    pub fn load() -> Result<Self> {
        Ok(ConfigStore::user_default()?.load()?.appearance)
    }
    pub fn save(&self) -> Result<()> {
        ConfigStore::user_default()?.update(|config| config.appearance = self.clone())?;
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
        // 解析顺序: 自定义方案 → 中文阅读预设 → 上游命名主题。自定义 slug 只由
        // `custom-<n>` 生成, 不会与后两者撞名; 命中自定义时它的 palette() 再按
        // base 回查预设/内置主题, 因此这里不必展开 base。
        let palette = self
            .custom_theme(&self.theme_slug)
            .map(CustomTheme::palette)
            .or_else(|| ThemeChoice::from_slug(&self.theme_slug).map(|choice| choice.palette()))
            .unwrap_or_else(|| self.theme_name().into_kit_palette());
        match self.background {
            BackgroundMode::Theme => palette,
            BackgroundMode::Terminal => terminal_background(palette),
        }
    }

    /// 按 slug 查用户方案。列表渲染、配色解析与删除都从这里取, 避免各自遍历。
    pub fn custom_theme(&self, slug: &str) -> Option<&CustomTheme> {
        self.custom_themes.iter().find(|theme| theme.slug == slug)
    }

    /// 删除一个自定义方案; 若它正是当前主题, 同步回落到默认主题。
    ///
    /// 返回是否真的删掉了某一项。调用方据此决定要不要写盘, 配置层不替 UI 决定提示文案。
    pub fn remove_custom_theme(&mut self, slug: &str) -> bool {
        let before = self.custom_themes.len();
        self.custom_themes.retain(|theme| theme.slug != slug);
        let removed = self.custom_themes.len() != before;
        if removed && self.theme_slug == slug {
            self.theme_slug = Self::DEFAULT_THEME.slug().to_string();
        }
        removed
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme_slug: Self::DEFAULT_THEME.slug().to_string(),
            background: BackgroundMode::Terminal,
            show_header: true,
            confirm_exit: default_confirm_exit(),
            custom_themes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReaderDisplayConfig {
    /// Follow actual played text; saved in the reader section of config.toml.
    #[serde(default = "default_show_title")]
    pub follow_tts: bool,
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
    /// 是否在正文下方常显一行快捷键提示。默认开:不点开帮助也知道键位。
    #[serde(default = "default_show_hint_bar")]
    pub show_hint_bar: bool,
    /// 正文每行最多显示多少个汉字(一个汉字按两列算)。0 表示跟随终端宽度。
    #[serde(default = "default_lines_per_row")]
    pub lines_per_row: u16,
    /// 段落首行缩进的字符数,0 为不缩进。
    #[serde(default = "default_first_line_indent")]
    pub first_line_indent: u16,
    /// 段落之间补几个空行。0 为紧凑,1 相当于旧版「段落间距」开关。
    #[serde(default = "default_paragraph_gap")]
    pub paragraph_gap: u16,
    /// 正文左右两侧各留白的列数。
    #[serde(default = "default_h_margin")]
    pub h_margin: u16,
    /// 阅读页边框样式。
    #[serde(default = "default_frameline")]
    pub frameline: Frameline,
    /// 是否在边框上显示小说标题。
    #[serde(default = "default_show_frameline_title")]
    pub show_frameline_title: bool,
    /// 把 ↑/↓ 当作整页翻页(PgUp/PgDn),而不是逐行滚动。
    ///
    /// 默认关:方向键逐行滚是既有习惯,改成翻页会让「想微调一行」的人无从下手。
    /// 但一屏能看的行数多的终端里,逐行挪一屏要按十几次,故给一个开关。
    /// 打开后 ↑/↓ 等价于 PageUp/PageDown,含章首/章末的二次确认语义。
    #[serde(default = "default_arrows_page")]
    pub arrows_page: bool,
}

/// 阅读页边框样式。`none` 时正文铺满整个可用区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frameline {
    None,
    Single,
    Double,
    Rounded,
    Thick,
}

impl Frameline {
    /// 设置面板里循环切换的顺序:由实到无,最后一项是关闭。
    pub const ALL: [Self; 5] = [
        Self::Single,
        Self::Rounded,
        Self::Double,
        Self::Thick,
        Self::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "无边框",
            Self::Single => "单线",
            Self::Double => "双线",
            Self::Rounded => "圆角",
            Self::Thick => "粗线",
        }
    }

    /// 边框占用的行列数;无边框时不占位,正文可用区域随之变大。
    pub fn inset(self) -> u16 {
        match self {
            Self::None => 0,
            _ => 1,
        }
    }

    /// 边框字符集。`None` 也返回单线集,由调用方同时把 `Borders` 设成空 ——
    /// 光有字符集、边还在,框线照样会画出来。
    pub fn border_set(self) -> ratatui::symbols::border::Set<'static> {
        use ratatui::symbols::border;
        match self {
            Self::None | Self::Single => border::PLAIN,
            Self::Double => border::DOUBLE,
            Self::Rounded => border::ROUNDED,
            Self::Thick => border::THICK,
        }
    }

    pub fn next(self) -> Self {
        let all = Self::ALL;
        let index = all.iter().position(|item| *item == self).unwrap_or(0);
        all[(index + 1) % all.len()]
    }

    pub fn previous(self) -> Self {
        let all = Self::ALL;
        let index = all.iter().position(|item| *item == self).unwrap_or(0);
        all[(index + all.len() - 1) % all.len()]
    }
}

impl ReaderDisplayConfig {
    /// 翻页重叠行数上限。再大就会把「翻一页」压成「滚几行」,属配置错误而非偏好。
    const PAGE_OVERLAP_MAX: u16 = 10;
    const PARAGRAPH_GAP_MAX: u16 = 4;
    const INDENT_MAX: u16 = 8;
    const H_MARGIN_MAX: u16 = 6;
    const LINES_PER_ROW_MIN: u16 = 10;
    /// 100 汉字 = 200 列,已超过任何常见终端宽度,再大只会撞到「跟随窗口」。
    const LINES_PER_ROW_MAX: u16 = 100;

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

    /// 正文折行宽度(列)。`lines_per_row` 按**汉字数**存,一个汉字占两列;
    /// 0 表示跟随窗口。窗口比设定窄时以窗口为准,否则正文会溢出边框。
    pub fn wrap_width(&self, content_width: usize) -> usize {
        match self.lines_per_row {
            0 => content_width,
            chars => content_width.min(chars as usize * 2),
        }
    }

    pub fn increase_page_overlap(&mut self) {
        self.page_overlap = (self.page_overlap + 1).min(Self::PAGE_OVERLAP_MAX);
    }

    pub fn decrease_page_overlap(&mut self) {
        self.page_overlap = self.page_overlap.saturating_sub(1);
    }

    /// 面板里「←/→」的增减入口。数值全部在此钳制,面板代码只报方向、不各自 clamp。
    pub fn increase_paragraph_gap(&mut self) {
        self.paragraph_gap = (self.paragraph_gap + 1).min(Self::PARAGRAPH_GAP_MAX);
        self.paragraph_spacing = self.paragraph_gap > 0;
    }

    pub fn decrease_paragraph_gap(&mut self) {
        self.paragraph_gap = self.paragraph_gap.saturating_sub(1);
        self.paragraph_spacing = self.paragraph_gap > 0;
    }

    pub fn increase_first_line_indent(&mut self) {
        self.first_line_indent = (self.first_line_indent + 1).min(Self::INDENT_MAX);
    }

    pub fn decrease_first_line_indent(&mut self) {
        self.first_line_indent = self.first_line_indent.saturating_sub(1);
    }

    /// 0 是「跟随终端宽度」这个特殊值,不参与数值增减:往下减到底就回到 0,
    /// 往上加则从最小字数起步,不会卡在 1 这种窄到没法读的宽度。
    pub fn increase_lines_per_row(&mut self) {
        self.lines_per_row = match self.lines_per_row {
            0 => Self::LINES_PER_ROW_MIN,
            n => (n + 1).min(Self::LINES_PER_ROW_MAX),
        };
    }

    pub fn decrease_lines_per_row(&mut self) {
        self.lines_per_row = if self.lines_per_row <= Self::LINES_PER_ROW_MIN {
            0
        } else {
            self.lines_per_row - 1
        };
    }

    pub fn increase_h_margin(&mut self) {
        self.h_margin = (self.h_margin + 1).min(Self::H_MARGIN_MAX);
    }

    pub fn decrease_h_margin(&mut self) {
        self.h_margin = self.h_margin.saturating_sub(1);
    }

    pub(crate) fn normalize(&mut self) {
        self.page_overlap = self.page_overlap.min(Self::PAGE_OVERLAP_MAX);
        // 旧版只有「段落间距」开关:开着等价于补 1 行,读到的旧配置据此换算。
        if self.paragraph_spacing {
            self.paragraph_gap = self.paragraph_gap.max(1);
        }
        self.paragraph_gap = self.paragraph_gap.min(Self::PARAGRAPH_GAP_MAX);
        self.paragraph_spacing = self.paragraph_gap > 0;
        self.first_line_indent = self.first_line_indent.min(Self::INDENT_MAX);
        self.h_margin = self.h_margin.min(Self::H_MARGIN_MAX);
        // 折行宽度太窄会把正文压成一列,按下限修正而不是拒绝启动。
        if self.lines_per_row > 0 {
            self.lines_per_row = self
                .lines_per_row
                .clamp(Self::LINES_PER_ROW_MIN, Self::LINES_PER_ROW_MAX);
        }
    }

    /// 段落之间实际补的空行数,面板与渲染共用,避免两处各算一遍。
    pub fn effective_paragraph_gap(&self) -> usize {
        self.paragraph_gap as usize
    }
    pub fn load() -> Result<Self> {
        Ok(ConfigStore::user_default()?.load()?.reader)
    }
    pub fn save(&self) -> Result<()> {
        ConfigStore::user_default()?.update(|config| config.reader = *self)?;
        Ok(())
    }
}

impl Default for ReaderDisplayConfig {
    fn default() -> Self {
        Self {
            follow_tts: true,
            show_title: default_show_title(),
            page_overlap: default_page_overlap(),
            paragraph_spacing: default_paragraph_spacing(),
            show_hint_bar: default_show_hint_bar(),
            lines_per_row: default_lines_per_row(),
            first_line_indent: default_first_line_indent(),
            paragraph_gap: default_paragraph_gap(),
            h_margin: default_h_margin(),
            frameline: default_frameline(),
            show_frameline_title: default_show_frameline_title(),
            arrows_page: default_arrows_page(),
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

fn default_confirm_exit() -> bool {
    true
}

fn default_show_hint_bar() -> bool {
    true
}

/// 0 = 不按字数折行,直接用终端宽度。
fn default_lines_per_row() -> u16 {
    0
}

fn default_first_line_indent() -> u16 {
    2
}

fn default_paragraph_gap() -> u16 {
    0
}

fn default_h_margin() -> u16 {
    0
}

fn default_frameline() -> Frameline {
    Frameline::Single
}

fn default_show_frameline_title() -> bool {
    true
}

/// 默认关:方向键逐行滚是既有习惯,想整页翻的用户在设置里一眼可开。
fn default_arrows_page() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn existing_appearance_config_keeps_header_visible() {
        let config: AppearanceConfig = toml::from_str("theme_slug = 'tokyo-night'").unwrap();
        assert!(config.show_header);
    }

    #[test]
    fn hidden_header_preference_round_trips() {
        let config = AppearanceConfig {
            show_header: false,
            ..Default::default()
        };
        let restored: AppearanceConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert!(!restored.show_header);
    }

    #[test]
    fn unknown_theme_slug_falls_back_to_default_theme() {
        let config = AppearanceConfig {
            theme_slug: "missing-theme".to_string(),
            background: BackgroundMode::Terminal,
            show_header: true,
            ..Default::default()
        };

        assert_eq!(config.theme_name(), AppearanceConfig::DEFAULT_THEME);
    }

    #[test]
    fn page_overlap_defaults_to_two() {
        assert_eq!(ReaderDisplayConfig::default().page_overlap, 2);
    }

    #[test]
    fn partial_config_without_page_overlap_takes_default() {
        // 旧版本写出的文件只有 show_title;缺字段走默认,既有字段不受影响。
        let config: ReaderDisplayConfig = serde_json::from_str(r#"{"show_title": false}"#).unwrap();

        assert!(!config.show_title);
        assert_eq!(config.page_overlap, 2);
        assert!(!config.paragraph_spacing);
    }

    #[test]
    fn paragraph_spacing_defaults_to_off_and_round_trips() {
        assert!(!ReaderDisplayConfig::default().paragraph_spacing);

        // 显式开启必须能被读回来,不能被 default 压回关。
        let config: ReaderDisplayConfig =
            serde_json::from_str(r#"{"paragraph_spacing": true}"#).unwrap();
        assert!(config.paragraph_spacing);
    }

    #[test]
    fn follow_defaults_on_and_round_trips_off() {
        let legacy: ReaderDisplayConfig = serde_json::from_str(r#"{"show_title":false}"#).unwrap();
        assert!(legacy.follow_tts);
        let config = ReaderDisplayConfig {
            follow_tts: false,
            ..Default::default()
        };
        let restored: ReaderDisplayConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert!(!restored.follow_tts);
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
            show_header: true,
            ..Default::default()
        };

        let palette = config.palette();
        let themed = ThemeName::Dracula.into_kit_palette();

        assert_eq!(palette.bg, Color::Reset);
        assert_eq!(palette.surface, Color::Reset);
        assert_eq!(palette.overlay, Color::Reset);
        assert_eq!(palette.accent, themed.accent);
        assert_eq!(palette.fg, themed.fg);
    }
    #[test]
    fn legacy_reader_config_gets_reading_defaults() {
        // 旧版本只写了 show_title:新字段一律走默认,不能因为缺字段就报错或归零。
        let config: ReaderDisplayConfig = serde_json::from_str(r#"{"show_title": false}"#).unwrap();

        assert!(!config.show_title);
        assert!(config.show_hint_bar);
        assert_eq!(config.frameline, Frameline::Single);
        assert!(config.show_frameline_title);
        assert_eq!(config.lines_per_row, 0);
        assert_eq!(config.first_line_indent, 2);
        assert_eq!(config.paragraph_gap, 0);
        assert_eq!(config.h_margin, 0);
    }

    #[test]
    fn arrows_page_is_opt_in_and_round_trips() {
        // 旧配置没有这一项:默认必须是「逐行滚动」,不能悄悄把方向键改成翻页。
        let config: ReaderDisplayConfig = serde_json::from_str(r#"{"show_title": true}"#).unwrap();
        assert!(!config.arrows_page);

        let config = ReaderDisplayConfig {
            arrows_page: true,
            ..Default::default()
        };
        let restored: ReaderDisplayConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert!(restored.arrows_page);
    }

    #[test]
    fn appearance_gains_confirm_exit_by_default() {
        // 旧配置文件没有这一项:确认默认开,防止 Q 误触退出丢进度。
        let config: AppearanceConfig = toml::from_str("theme_slug = 'tokyo-night'").unwrap();
        assert!(config.confirm_exit);

        let config = AppearanceConfig {
            confirm_exit: false,
            ..Default::default()
        };
        let restored: AppearanceConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert!(!restored.confirm_exit);
    }

    #[test]
    fn frameline_cycles_through_every_style_and_round_trips() {
        let mut style = Frameline::Single;
        let mut seen = Vec::new();
        for _ in 0..Frameline::ALL.len() {
            seen.push(style);
            let json = serde_json::to_string(&style).unwrap();
            assert_eq!(serde_json::from_str::<Frameline>(&json).unwrap(), style);
            assert_eq!(style.previous().next(), style);
            style = style.next();
        }
        // 循环一圈必须正好回到起点,否则面板里会漏掉某种样式。
        assert_eq!(style, Frameline::Single);
        assert_eq!(seen.len(), Frameline::ALL.len());

        assert_eq!(Frameline::None.inset(), 0);
        assert_eq!(Frameline::Double.inset(), 1);
    }

    #[test]
    fn layout_controls_clamp_at_their_bounds() {
        let mut config = ReaderDisplayConfig::default();

        for _ in 0..10 {
            config.increase_paragraph_gap();
            config.increase_first_line_indent();
            config.increase_h_margin();
        }
        assert_eq!(config.paragraph_gap, ReaderDisplayConfig::PARAGRAPH_GAP_MAX);
        assert_eq!(config.first_line_indent, ReaderDisplayConfig::INDENT_MAX);
        assert_eq!(config.h_margin, ReaderDisplayConfig::H_MARGIN_MAX);

        for _ in 0..10 {
            config.decrease_first_line_indent();
            config.decrease_h_margin();
        }
        assert_eq!(config.first_line_indent, 0);
        assert_eq!(config.h_margin, 0);

        // 磁盘脏值在 normalize 里修正,而不是渲染时现场截断。
        config.lines_per_row = 3;
        config.normalize();
        assert_eq!(config.lines_per_row, ReaderDisplayConfig::LINES_PER_ROW_MIN);
        config.lines_per_row = 999;
        config.normalize();
        assert_eq!(config.lines_per_row, ReaderDisplayConfig::LINES_PER_ROW_MAX);
    }

    #[test]
    fn lines_per_row_zero_means_follow_terminal() {
        let mut config = ReaderDisplayConfig::default();
        assert_eq!(config.lines_per_row, 0);

        // 0 是特殊值,往上加要从最小可读字数起步,而不是变成 1 列。
        config.increase_lines_per_row();
        assert_eq!(config.lines_per_row, ReaderDisplayConfig::LINES_PER_ROW_MIN);

        for _ in 0..5 {
            config.decrease_lines_per_row();
        }
        assert_eq!(config.lines_per_row, 0);
    }

    #[test]
    fn lines_per_row_counts_han_cells() {
        let mut config = ReaderDisplayConfig::default();
        // 0 = 跟随窗口:可用宽度原样返回。
        assert_eq!(config.wrap_width(72), 72);

        // 设定值按汉字计:30 字 = 60 列,不再出现「设 30 只显示 16 个汉字」。
        config.lines_per_row = 30;
        assert_eq!(config.wrap_width(72), 60);

        // 窗口更窄时以窗口为准,避免正文溢出边框。
        assert_eq!(config.wrap_width(40), 40);
    }

    fn sample_custom(slug: &str, base: &str) -> CustomTheme {
        CustomTheme {
            name: format!("我的方案 {slug}"),
            slug: slug.to_string(),
            base: base.to_string(),
            bg: "#000000".to_string(),
            fg: "#ffffff".to_string(),
            accent: "#ff0000".to_string(),
        }
    }

    #[test]
    fn custom_theme_overrides_only_base_semantic_colors() {
        let custom = sample_custom("custom-1", "rice-paper");
        let config = AppearanceConfig {
            theme_slug: custom.slug.clone(),
            background: BackgroundMode::Theme,
            custom_themes: vec![custom],
            ..Default::default()
        };

        let palette = config.palette();
        let base = ThemeChoice::from_slug("rice-paper").unwrap().palette();

        assert_eq!(palette.bg, Color::Rgb(0, 0, 0));
        assert_eq!(palette.fg, Color::Rgb(255, 255, 255));
        assert_eq!(palette.accent, Color::Rgb(255, 0, 0));
        // 只覆盖三个语义色, 其余字段仍沿用 base, 自定义方案不会悄悄变成另一套配色。
        assert_eq!(palette.selection, base.selection);
        assert_eq!(palette.success, base.success);
        // 派生色跟着 accent 走, 否则焦点框还停留在 base 的强调色上。
        assert_eq!(palette.border_active, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn custom_theme_with_unknown_base_falls_back_to_default_theme() {
        let custom = sample_custom("custom-1", "no-such-base");
        let config = AppearanceConfig {
            theme_slug: custom.slug.clone(),
            background: BackgroundMode::Theme,
            custom_themes: vec![custom],
            ..Default::default()
        };

        let palette = config.palette();
        let fallback = ThemeName::TokyoNight.into_kit_palette();
        assert_eq!(palette.selection, fallback.selection);
        assert_eq!(palette.success, fallback.success);
        assert_eq!(palette.bg, Color::Rgb(0, 0, 0));
    }

    #[test]
    fn custom_theme_slugs_avoid_existing_and_builtin_slugs() {
        let existing = vec![sample_custom("custom-1", "rice-paper")];
        let slug = CustomTheme::next_slug(&existing);
        assert_eq!(slug, "custom-2");
        assert!(!existing.iter().any(|theme| theme.slug == slug));
        assert!(ThemeChoice::from_slug(&slug).is_none());
    }

    #[test]
    fn removing_custom_theme_drops_it_and_resets_current_theme() {
        let mut config = AppearanceConfig {
            theme_slug: "custom-2".to_string(),
            background: BackgroundMode::Theme,
            custom_themes: vec![
                sample_custom("custom-1", "rice-paper"),
                sample_custom("custom-2", "ink-black"),
            ],
            ..Default::default()
        };

        assert!(config.remove_custom_theme("custom-2"));
        assert_eq!(config.theme_slug, AppearanceConfig::DEFAULT_THEME.slug());
        assert!(config.custom_theme("custom-2").is_none());

        let restored: AppearanceConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored.custom_themes.len(), 1);
        assert!(restored.custom_theme("custom-2").is_none());
    }

    #[test]
    fn legacy_config_without_custom_themes_still_loads() {
        // 旧版本写出的 [appearance] 没有 custom_themes: 缺字段必须走默认空表,
        // 既有主题解析与终端背景行为都不能受影响。
        let config: AppearanceConfig = toml::from_str("theme_slug = 'tokyo-night'").unwrap();
        assert!(config.custom_themes.is_empty());
        assert_eq!(config.palette().bg, Color::Reset);
    }

    #[test]
    fn paragraph_gap_drives_legacy_spacing_flag() {
        let mut config = ReaderDisplayConfig::default();
        config.increase_paragraph_gap();
        assert!(config.paragraph_spacing);
        assert_eq!(config.effective_paragraph_gap(), 1);

        config.decrease_paragraph_gap();
        assert!(!config.paragraph_spacing);

        // 旧配置只有开关:读到后换算成 1 行空行,再写回时两个字段保持一致。
        let mut legacy: ReaderDisplayConfig =
            serde_json::from_str(r#"{"paragraph_spacing": true}"#).unwrap();
        legacy.normalize();
        assert_eq!(legacy.paragraph_gap, 1);
        assert!(legacy.paragraph_spacing);
    }
}
