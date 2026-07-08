use ratatui::style::{Modifier, Style};
use ratatui_kit::{ComponentTheme, Palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppChromeTheme {
    pub logo: Style,
    pub text: Style,
    pub muted: Style,
    pub title: Style,
    pub meta_label: Style,
    pub selected: Style,
    pub highlight: Style,
    pub success: Style,
    pub error: Style,
    pub empty: Style,
    pub border: Style,
    pub loading: Style,
}

impl ComponentTheme for AppChromeTheme {
    fn from_palette(palette: &Palette) -> Self {
        Self {
            logo: Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
            text: Style::new().fg(palette.fg),
            muted: Style::new().fg(palette.fg_dim),
            title: Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
            meta_label: Style::new().fg(palette.fg_dim),
            selected: Style::new()
                .fg(palette.on_accent)
                .bg(palette.selection)
                .add_modifier(Modifier::BOLD),
            highlight: Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
            success: Style::new().fg(palette.success),
            error: Style::new().fg(palette.error).add_modifier(Modifier::BOLD),
            empty: Style::new().fg(palette.placeholder),
            border: Style::new().fg(palette.border),
            loading: Style::new().fg(palette.accent),
        }
    }
}

impl Default for AppChromeTheme {
    fn default() -> Self {
        Self::from_palette(&Palette::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderTheme {
    pub content: Style,
    pub chapter: Style,
    pub footer: Style,
    pub progress: Style,
    pub border: Style,
    pub tts_highlight: Style,
}

impl ComponentTheme for ReaderTheme {
    fn from_palette(palette: &Palette) -> Self {
        Self {
            content: Style::new().fg(palette.fg),
            chapter: Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
            footer: Style::new().fg(palette.fg_dim),
            progress: Style::new().fg(palette.accent),
            border: Style::new().fg(palette.border),
            tts_highlight: Style::new()
                .fg(palette.success)
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl Default for ReaderTheme {
    fn default() -> Self {
        Self::from_palette(&Palette::default())
    }
}
