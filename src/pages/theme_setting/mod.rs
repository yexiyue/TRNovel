use crate::{
    ThemeConfig,
    components::{KeyShortcutInfo, ShortcutInfoModal, list_select::ListSelect},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    style::{Color, Stylize},
    text::Line,
    widgets::{Block, Padding, Widget, WidgetRef},
};
use ratatui_kit::prelude::*;
use tui_widget_list::ListBuildContext;

mod select_color;
use select_color::SelectColor;

#[derive(Debug, Clone)]
pub struct ListItem {
    pub name: String,
    pub color: Color,
    pub selected: bool,
    pub theme: ThemeConfig,
}

impl WidgetRef for ListItem {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let block = if self.selected {
            Block::bordered()
                .title(Line::from(self.name.clone()).style(self.theme.basic.border_title))
                .padding(Padding::horizontal(0))
                .style(self.theme.selected)
        } else {
            Block::bordered()
                .title(Line::from(self.name.clone()).style(self.theme.basic.border_title))
                .padding(Padding::horizontal(0))
        };

        let inner_area = block.inner(area);

        block.render(area, buf);

        let color = Line::from(format!("■ {}", self.color))
            .centered()
            .style(self.color);
        color.render(inner_area, buf);
    }
}

// 定义颜色项枚举，避免使用字符串匹配
#[derive(Debug, Clone, PartialEq)]
enum ColorItem {
    Text,
    Primary,
    Warning,
    Error,
    Success,
    Info,
}

impl ColorItem {
    fn name(&self) -> &'static str {
        match self {
            ColorItem::Text => "Text Color",
            ColorItem::Primary => "Primary Color",
            ColorItem::Warning => "Warning Color",
            ColorItem::Error => "Error Color",
            ColorItem::Success => "Success Color",
            ColorItem::Info => "Info Color",
        }
    }

    fn get_color(&self, theme: &ThemeConfig) -> Color {
        match self {
            ColorItem::Text => theme.colors.text_color,
            ColorItem::Primary => theme.colors.primary_color,
            ColorItem::Warning => theme.colors.warning_color,
            ColorItem::Error => theme.colors.error_color,
            ColorItem::Success => theme.colors.success_color,
            ColorItem::Info => theme.colors.info_color,
        }
    }

    fn set_color(&self, theme: &mut ThemeConfig, color: Color) {
        match self {
            ColorItem::Text => theme.colors.text_color = color,
            ColorItem::Primary => theme.colors.primary_color = color,
            ColorItem::Warning => theme.colors.warning_color = color,
            ColorItem::Error => theme.colors.error_color = color,
            ColorItem::Success => theme.colors.success_color = color,
            ColorItem::Info => theme.colors.info_color = color,
        }
    }

    fn all() -> Vec<ColorItem> {
        vec![
            ColorItem::Text,
            ColorItem::Primary,
            ColorItem::Warning,
            ColorItem::Error,
            ColorItem::Success,
            ColorItem::Info,
        ]
    }
}

#[component]
pub fn ThemeSetting(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut info_modal_open = hooks.use_state(|| false);
    let theme_config = *hooks.use_context::<State<ThemeConfig>>();
    let theme = theme_config.read().clone();
    let mut current = hooks.use_state(|| None::<ColorItem>);
    let is_inputting = *hooks.use_context::<State<bool>>();

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && !is_inputting.get()
        {
            match key.code {
                KeyCode::Char('i') | KeyCode::Char('I') if current.read().is_none() => {
                    info_modal_open.set(!info_modal_open.get());
                }
                KeyCode::Esc if current.read().is_some() => {
                    current.set(None);
                }
                _ => {}
            }
        }
    });

    // 使用ColorItem枚举来避免字符串匹配
    let color_items: Vec<ColorItem> = ColorItem::all();

    element!(Fragment{
        ListSelect<ColorItem>(
            items: ColorItem::all(),
            default_value: 0,
            top_title: Line::from("主题设置").style(theme.highlight).centered().not_dim(),
            render_item:move|ctx:&ListBuildContext|{
                let color_item=color_items[ctx.index].clone();
                let name = color_item.name().to_string();
                let color = color_item.get_color(&theme);

                (ListItem {
                    name,
                    color,
                    selected: ctx.is_selected,
                    theme: theme.clone(),
                }.into(),3)
            },
            is_editing: current.read().is_none() && !info_modal_open.get(),
            on_select: move |item:ColorItem| {
                current.set(Some(item));
            },
        )
        #(if let Some(selected_item) = current.read().clone() {
            element!(SelectColor(
                color: selected_item.get_color(&theme_config.read().clone()),
                is_editing: !info_modal_open.get(),
                on_change: move |color| {
                    let mut theme = theme_config.read().clone();
                    selected_item.set_color(&mut theme, color);
                    let new_theme = ThemeConfig::from_colors(theme.colors);
                    let _ = new_theme.save();
                    *theme_config.write() = new_theme;
                    // 更新current状态以刷新UI
                    current.set(None);
                }
            )).into_any()
        }else{
            element!(ShortcutInfoModal(
                key_shortcut_info: KeyShortcutInfo::new(vec![
                    ("选择下一个", "J / ▼"),
                    ("选择上一个", "K / ▲"),
                    ("确认选择", "Enter"),
                    ("显示帮助", "I"),
                ]),
                open: info_modal_open.get(),
            )).into_any()
        })
    })
}
