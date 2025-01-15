use crate::{
    app::State,
    components::{Component, KeyShortcutInfo},
    Navigator, Result, RoutePage, Router, ThemeColors, ThemeConfig, THEME_CONFIG,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Flex, Layout},
    style::{Color, Stylize},
    text::Line,
    widgets::{Block, Clear, Padding, Scrollbar, ScrollbarState, Widget},
};
use select_color::SelectColor;
use tokio::sync::mpsc::Sender;
use tui_widget_list::{ListBuilder, ListState, ListView};

use super::{Page, PageWrapper};
mod select_color;

#[derive(Debug, Clone)]
pub struct ListItem {
    pub name: String,
    pub color: Color,
    pub selected: bool,
}

impl Widget for ListItem {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let block = if self.selected {
            Block::bordered()
                .title(Line::from(self.name).style(THEME_CONFIG.basic.border_title))
                .padding(Padding::horizontal(0))
                .style(THEME_CONFIG.selected)
        } else {
            Block::bordered()
                .title(Line::from(self.name).style(THEME_CONFIG.basic.border_title))
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

#[derive(Debug, Clone)]
pub enum ThemeSettingPageMsg {
    SelectColor(Color),
}

pub struct ThemeSettingPage {
    pub state: ListState,
    pub list: Vec<(String, Color)>,
    pub theme_colors: ThemeColors,
    pub sender: Sender<ThemeSettingPageMsg>,
    pub select_color: SelectColor,
    pub show_select_color: bool,
}

impl ThemeSettingPage {
    pub fn to_page_route() -> Box<dyn RoutePage> {
        Box::new(PageWrapper::<Self, (), ThemeSettingPageMsg>::new((), None))
    }
}

#[async_trait]
impl Page for ThemeSettingPage {
    type Msg = ThemeSettingPageMsg;

    async fn init(
        _arg: (),
        sender: Sender<Self::Msg>,
        _navigator: Navigator,
        _state: State,
    ) -> Result<Self> {
        let sender_clone = sender.clone();

        Ok(Self {
            select_color: SelectColor::new(move |color: Color| {
                sender_clone
                    .try_send(ThemeSettingPageMsg::SelectColor(color))
                    .unwrap();
            }),
            sender,
            state: ListState::default(),
            list: vec![
                ("Text Color".to_string(), THEME_CONFIG.colors.text_color),
                (
                    "Primary Color".to_string(),
                    THEME_CONFIG.colors.primary_color,
                ),
                (
                    "Warning Color".to_string(),
                    THEME_CONFIG.colors.warning_color,
                ),
                ("Error Color".to_string(), THEME_CONFIG.colors.error_color),
                (
                    "Success Color".to_string(),
                    THEME_CONFIG.colors.success_color,
                ),
                ("Info Color".to_string(), THEME_CONFIG.colors.info_color),
            ],
            theme_colors: THEME_CONFIG.colors.clone(),
            show_select_color: false,
        })
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            ThemeSettingPageMsg::SelectColor(color) => {
                let index = self.state.selected.ok_or("请选择要配置的主题色")?;
                let (name, item_color) = self.list.get_mut(index).ok_or("请选择要配置的主题色")?;
                *item_color = color;

                match name.as_str() {
                    "Text Color" => {
                        self.theme_colors.text_color = color;
                    }
                    "Primary Color" => {
                        self.theme_colors.primary_color = color;
                    }
                    "Warning Color" => {
                        self.theme_colors.warning_color = color;
                    }
                    "Error Color" => {
                        self.theme_colors.error_color = color;
                    }
                    "Success Color" => {
                        self.theme_colors.success_color = color;
                    }
                    "Info Color" => {
                        self.theme_colors.info_color = color;
                    }
                    _ => {}
                }

                self.select_color.reset();
                self.show_select_color = false;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Component for ThemeSettingPage {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        let block = Block::bordered()
            .title(
                Line::from("主题设置")
                    .centered()
                    .style(THEME_CONFIG.basic.border_title),
            )
            .title_bottom(
                Line::from("设置成功后请重启应用")
                    .centered()
                    .style(THEME_CONFIG.detail_info),
            )
            .border_style(THEME_CONFIG.basic.border);

        let inner_area = block.inner(area);
        let items = self.list.clone();

        let builder = ListBuilder::new(move |ctx| {
            let item = items[ctx.index].clone();
            (
                ListItem {
                    name: item.0,
                    color: item.1,
                    selected: ctx.is_selected,
                },
                3,
            )
        });
        let widget = ListView::new(builder, self.list.len()).infinite_scrolling(false);

        frame.render_widget(block, area);
        frame.render_stateful_widget(widget, inner_area, &mut self.state);

        if self.list.len() * 3 > inner_area.height as usize {
            let mut scrollbar_state =
                ScrollbarState::new(self.list.len()).position(self.state.selected.unwrap_or(0));
            frame.render_stateful_widget(Scrollbar::default(), area, &mut scrollbar_state);
        }

        if self.show_select_color {
            let block = Block::new().dim();

            frame.render_widget(block, area);
            let [horizontal] = Layout::horizontal([Constraint::Percentage(70)])
                .flex(Flex::Center)
                .areas(area);

            let [block_area] = Layout::vertical([Constraint::Max(10)])
                .flex(Flex::Center)
                .areas(horizontal);

            frame.render_widget(Clear, block_area);
            self.select_color.render(frame, block_area)?;
        }

        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent, state: State) -> Result<Option<KeyEvent>> {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(Some(key));
        }

        let key = if self.show_select_color {
            let Some(key) = self.select_color.handle_key_event(key, state).await? else {
                return Ok(None);
            };
            key
        } else {
            key
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.next();
                Ok(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.previous();
                Ok(None)
            }
            KeyCode::Enter => {
                self.show_select_color = true;
                let index = self.state.selected.ok_or("请选择要配置的主题色")?;
                let (_, color) = self.list.get(index).ok_or("请选择要配置的主题色")?;
                self.select_color.select(*color);
                Ok(None)
            }
            KeyCode::Esc => {
                self.select_color.reset();
                self.show_select_color = false;
                Ok(None)
            }
            _ => {
                return Ok(Some(key));
            }
        }
    }

    fn key_shortcut_info(&self) -> KeyShortcutInfo {
        let data = if self.show_select_color {
            vec![
                ("输入颜色", "S"),
                ("选择下一个", "J / ▼"),
                ("选择上一个", "K / ▲"),
                ("取消", "Esc"),
                ("确定", "Enter"),
            ]
        } else {
            vec![
                ("选择下一个", "J / ▼"),
                ("选择上一个", "K / ▲"),
                ("进入选择颜色模式", "Enter"),
            ]
        };
        KeyShortcutInfo::new(data)
    }
}

#[async_trait]
impl Router for ThemeSettingPage {
    async fn on_unmounted(&mut self, _state: State) -> Result<()> {
        let new_theme = ThemeConfig::from_colors(self.theme_colors.clone());
        new_theme.save()?;
        Ok(())
    }
}
