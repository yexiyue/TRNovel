


use crate::{
    cache::{ThemeConfig, ThemeColors},
    components::{modal::shortcut_info_modal::KeyShortcutInfo},
    Result,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Flex, Layout},
    style::{Color, Stylize},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListState, Padding, Paragraph, Scrollbar, ScrollbarState, Widget},
};
use ratatui_kit::{Component, Props};
use ratatui_kit::prelude::*;

use tokio::sync::mpsc::Sender;
use tui_widget_list::{ListBuilder, ListState as TuiListState, ListView};
use crate::hooks::UseThemeConfig;

mod select_color;
use select_color::SelectColor;

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
        let theme_config = ThemeConfig::default();
        let block = if self.selected {
            Block::bordered()
                .title(Line::from(self.name).style(theme_config.basic.border_title))
                .padding(Padding::horizontal(0))
                .style(theme_config.selected)
        } else {
            Block::bordered()
                .title(Line::from(self.name).style(theme_config.basic.border_title))
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
    pub state: TuiListState,
    pub list: Vec<(String, Color)>,
    pub theme_colors: ThemeColors,
    pub sender: Sender<ThemeSettingPageMsg>,
    pub select_color: SelectColor,
    pub show_select_color: bool,
}

#[derive(Default, Props)]
pub struct ThemeSettingPageProps {
    // 可以添加需要的属性
}

impl Component for ThemeSettingPage {
    type Props<'a> = ThemeSettingPageProps;

    fn new(_props: &Self::Props<'_>) -> Self {
        Self::new(tokio::sync::mpsc::channel(100).0)
    }
}

impl ThemeSettingPage {
    pub fn new(sender: Sender<ThemeSettingPageMsg>) -> Self {
        let theme_config = ThemeConfig::default();
        let mut this = Self {
            select_color: SelectColor::new(),
            sender,
            state: TuiListState::default(),
            list: vec![
                ("Text Color".to_string(), theme_config.colors.text_color),
                ("Primary Color".to_string(), theme_config.colors.primary_color),
                ("Warning Color".to_string(), theme_config.colors.warning_color),
                ("Error Color".to_string(), theme_config.colors.error_color),
                ("Success Color".to_string(), theme_config.colors.success_color),
                ("Info Color".to_string(), theme_config.colors.info_color),
            ],
            theme_colors: theme_config.colors,
            show_select_color: false,
        };
        // 确保初始有选中项，避免第三方列表在 None 时不渲染或内部断言
        this.state.selected = Some(0);
        this
    }

    pub async fn update(&mut self, msg: ThemeSettingPageMsg) -> Result<()> {
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

    pub fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        let theme_config = ThemeConfig::default();
        let block = Block::bordered()
            .title(
                Line::from("主题设置")
                    .centered()
                    .style(theme_config.basic.border_title),
            )
            .title_bottom(
                Line::from("设置成功后请重启应用")
                    .centered()
                    .style(theme_config.detail_info),
            )
            .border_style(theme_config.basic.border);

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

    pub fn handle_key_event(
        &mut self, 
        key: KeyEvent
    ) -> Result<Option<KeyEvent>> {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(Some(key));
        }

        let key = if self.show_select_color {
            if let Some(color) = self.select_color.handle_key_event(key)? {
                let index = self.state.selected.ok_or("请选择要配置的主题色")?;
                let (name, item_color) = self.list.get_mut(index).ok_or("请选择要配置的主题色")?;
                *item_color = color;

                match name.as_str() {
                    "Text Color" => self.theme_colors.text_color = color,
                    "Primary Color" => self.theme_colors.primary_color = color,
                    "Warning Color" => self.theme_colors.warning_color = color,
                    "Error Color" => self.theme_colors.error_color = color,
                    "Success Color" => self.theme_colors.success_color = color,
                    "Info Color" => self.theme_colors.info_color = color,
                    _ => {}
                }

                self.select_color.reset();
                self.show_select_color = false;
                return Ok(None);
            }
            return Ok(None);
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
                if self.state.selected.is_some() {
                    self.show_select_color = true;
                    let index = self.state.selected.ok_or("请选择要配置的主题色")?;
                    let (_, color) = self.list.get(index).ok_or("请选择要配置的主题色")?;
                    self.select_color.select(*color);
                }
                Ok(None)
            }
            KeyCode::Esc => {
                self.select_color.reset();
                self.show_select_color = false;
                Ok(None)
            }
            _ => {
                Ok(Some(key))
            }
        }
    }

    pub fn key_shortcut_info(&self) -> KeyShortcutInfo {
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

    pub fn save_theme(&self) -> Result<()> {
        let new_theme = ThemeConfig::from_colors(self.theme_colors);
        new_theme.save()?;
        Ok(())
    }
}

// 辅助函数：格式化颜色显示
fn format_color_display(color: Color) -> String {
    match color {
        Color::Reset => "Reset".to_string(),
        Color::Black => "Black".to_string(),
        Color::White => "White".to_string(),
        Color::Red => "Red".to_string(),
        Color::Green => "Green".to_string(),
        Color::Yellow => "Yellow".to_string(),
        Color::Blue => "Blue".to_string(),
        Color::Magenta => "Magenta".to_string(),
        Color::Gray => "Gray".to_string(),
        Color::DarkGray => "DarkGray".to_string(),
        Color::LightRed => "LightRed".to_string(),
        Color::LightGreen => "LightGreen".to_string(),
        Color::LightYellow => "LightYellow".to_string(),
        Color::LightBlue => "LightBlue".to_string(),
        Color::LightMagenta => "LightMagenta".to_string(),
        Color::LightCyan => "LightCyan".to_string(),
        _ => format!("{:?}", color),
    }
}

#[component]
pub fn ThemeSetting(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();

    let 
     main_state = hooks.use_state(|| {
        let mut s = ListState::default();
        s.select(Some(0));
        s
    });
    let mut color_state = hooks.use_state(|| {
        let mut s = ListState::default();
        s.select(Some(0));
        s
    });
    let mut show_picker = hooks.use_state(|| false);
    
    // 维护当前主题颜色配置
    let mut theme_colors_state = hooks.use_state(|| theme.colors);

    // 颜色列表
    let colors: Vec<(String, Color)> = vec![
        ("Reset".into(), Color::Reset),
        ("Black".into(), Color::Black),
        ("White".into(), Color::White),
        ("Red".into(), Color::Red),
        ("Green".into(), Color::Green),
        ("Yellow".into(), Color::Yellow),
        ("Blue".into(), Color::Blue),
        ("Magenta".into(), Color::Magenta),
        ("Gray".into(), Color::Gray),
        ("DarkGray".into(), Color::DarkGray),
        ("LightRed".into(), Color::LightRed),
        ("LightGreen".into(), Color::LightGreen),
        ("LightYellow".into(), Color::LightYellow),
        ("LightBlue".into(), Color::LightBlue),
        ("LightMagenta".into(), Color::LightMagenta),
        ("LightCyan".into(), Color::LightCyan),
    ];

    hooks.use_events({
        let mut show_picker = show_picker.clone();
        let mut main_state = main_state.clone();
        let mut color_state = color_state.clone();
        let mut theme_colors_state = theme_colors_state.clone();
        let colors = colors.clone();
        move |event| {
            if let crossterm::event::Event::Key(key) = event
                && key.kind == crossterm::event::KeyEventKind::Press
            {
                if show_picker.get() {
                    match key.code {
                        crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                            color_state.write().select_next();
                        }
                        crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                            color_state.write().select_previous();
                        }
                        crossterm::event::KeyCode::Esc => {
                            show_picker.set(false);
                        }
                        crossterm::event::KeyCode::Enter => {
                            // 应用颜色并保存
                            if let Some(color_idx) = color_state.read().selected() {
                                if let Some(selected_color) = colors.get(color_idx) {
                                    let color = selected_color.1;
                                    if let Some(main_idx) = main_state.read().selected() {
                                        let mut theme_colors = *theme_colors_state.read();
                                        match main_idx {
                                            0 => theme_colors.text_color = color,
                                            1 => theme_colors.primary_color = color,
                                            2 => theme_colors.warning_color = color,
                                            3 => theme_colors.error_color = color,
                                            4 => theme_colors.success_color = color,
                                            5 => theme_colors.info_color = color,
                                            _ => {}
                                        }
                                        theme_colors_state.set(theme_colors);
                                        
                                        // 保存到文件
                                        let new_theme = ThemeConfig::from_colors(*theme_colors_state.read());
                                        let _ = new_theme.save();
                                    }
                                }
                            }
                            show_picker.set(false);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                            main_state.write().select_next();
                        }
                        crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                            main_state.write().select_previous();
                        }
                        crossterm::event::KeyCode::Enter => {
                            // 进入颜色选择器，预设当前颜色为选中状态
                            if let Some(main_idx) = main_state.read().selected() {
                                let current_color = match main_idx {
                                    0 => theme_colors_state.read().text_color,
                                    1 => theme_colors_state.read().primary_color,
                                    2 => theme_colors_state.read().warning_color,
                                    3 => theme_colors_state.read().error_color,
                                    4 => theme_colors_state.read().success_color,
                                    5 => theme_colors_state.read().info_color,
                                    _ => Color::Reset,
                                };
                                
                                // 找到当前颜色在列表中的索引
                                let color_idx = colors.iter()
                                    .position(|(_, c)| *c == current_color)
                                    .unwrap_or(0);
                                color_state.write().select(Some(color_idx));
                                show_picker.set(true);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    let title = Line::from("主题设置")
        .centered()
        .style(theme.basic.border_title);

    // 基于当前状态动态生成列表项
    let main_list_items: Vec<Line> = {
        let theme_colors = *theme_colors_state.read();
        vec![
            Line::from(vec![
                Span::from("Text Color"),
                Span::from(" "),
                Span::from("■ ").fg(theme_colors.text_color),
                Span::from(format_color_display(theme_colors.text_color)).fg(theme_colors.text_color),
            ]),
            Line::from(vec![
                Span::from("Primary Color"),
                Span::from(" "),
                Span::from("■ ").fg(theme_colors.primary_color),
                Span::from(format_color_display(theme_colors.primary_color)).fg(theme_colors.primary_color),
            ]),
            Line::from(vec![
                Span::from("Warning Color"),
                Span::from(" "),
                Span::from("■ ").fg(theme_colors.warning_color),
                Span::from(format_color_display(theme_colors.warning_color)).fg(theme_colors.warning_color),
            ]),
            Line::from(vec![
                Span::from("Error Color"),
                Span::from(" "),
                Span::from("■ ").fg(theme_colors.error_color),
                Span::from(format_color_display(theme_colors.error_color)).fg(theme_colors.error_color),
            ]),
            Line::from(vec![
                Span::from("Success Color"),
                Span::from(" "),
                Span::from("■ ").fg(theme_colors.success_color),
                Span::from(format_color_display(theme_colors.success_color)).fg(theme_colors.success_color),
            ]),
            Line::from(vec![
                Span::from("Info Color"),
                Span::from(" "),
                Span::from("■ ").fg(theme_colors.info_color),
                Span::from(format_color_display(theme_colors.info_color)).fg(theme_colors.info_color),
            ]),
        ]
    };
    
    let main_list = List::new(main_list_items)
        .style(theme.basic.text)
        .highlight_style(theme.selected);

    let color_list = List::new(
        colors
            .iter()
            .map(|(label, c)| Line::from(label.clone()).centered().style(theme.basic.text.fg(*c)))
            .collect::<Vec<_>>()
    )
    .style(theme.basic.text)
    .highlight_style(theme.selected);

    let title_widget = Paragraph::new(vec![title]);

    element!(
        Center{
            View(height:Constraint::Length(1)){
                $title_widget
            }
            #(if show_picker.get() {
                element!(View(height:Constraint::Fill(1)){
                    $(color_list,color_state)
                }).into_any()
            } else {
                element!(View(height:Constraint::Fill(1)){
                    $(main_list,main_state)
                }).into_any()
            })
            View(height:Constraint::Length(1)){
                $Line::from("设置成功后请重启应用").centered().style(theme.detail_info)
            }
        }
    )
}