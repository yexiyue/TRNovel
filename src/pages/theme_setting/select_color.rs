use crate::{
    cache::ThemeConfig,
    components::search_input::SearchInputProps,
    Result,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, ToText},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Scrollbar, ScrollbarState},
};


 

pub struct SelectColor {
    pub search_props: SearchInputProps,
    pub items: Vec<Color>,
    pub list_state: ListState,
}

impl SelectColor {
    pub fn new() -> Self {
        let mut search_props = SearchInputProps::default();
        search_props.placeholder = "请输入颜色".to_string();

        Self {
            search_props,
            items: vec![
                Color::Reset,
                Color::Black,
                Color::White,
                Color::Red,
                Color::Green,
                Color::Yellow,
                Color::Blue,
                Color::Magenta,
                Color::Gray,
                Color::DarkGray,
                Color::LightRed,
                Color::LightGreen,
                Color::LightYellow,
                Color::LightBlue,
                Color::LightMagenta,
                Color::LightCyan,
            ],
            list_state: ListState::default(),
        }
    }

    pub fn select(&mut self, color: Color) {
        let index = self.items.iter().position(|c| *c == color);
        if let Some(index) = index {
            self.list_state.select(Some(index));
        }
    }

    pub fn reset(&mut self) {
        self.list_state.select(None);
    }

    pub fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        let theme_config = ThemeConfig::default();
        let [top, content] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        let block = Block::bordered()
            .title(
                Line::from("选择颜色")
                    .centered()
                    .style(theme_config.basic.border_title),
            )
            .border_style(theme_config.basic.border)
            .padding(Padding::horizontal(1));

        // 使用基本组件渲染搜索框
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("搜索颜色");
            
        let input_widget = Paragraph::new(self.search_props.value.clone())
            .block(input_block)
            .style(Style::default());
            
        frame.render_widget(input_widget, top);

        // Prepare list items once so we can get length and reuse
        let list_items: Vec<ListItem> = self
            .items
            .iter()
            .map(|color| ListItem::new(color.to_text().style(theme_config.basic.text.fg(*color))))
            .collect();

        let mut scrollbar_state =
            ScrollbarState::new(list_items.len()).position(self.list_state.selected().unwrap_or(0));

        // Render the list inside a bordered block so it shows correctly
        let list = List::new(list_items)
            .block(block)
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, content, &mut self.list_state);

        // Render scrollbar aligned with the list area
        frame.render_stateful_widget(Scrollbar::default(), content, &mut scrollbar_state);
        Ok(())
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Color>> {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(None);
        }
        
        // 处理搜索框的键盘事件
        match key.code {
            KeyCode::Down => {
                let next_index = match self.list_state.selected() {
                    Some(i) => (i + 1).min(self.items.len().saturating_sub(1)),
                    None => 0,
                };
                self.list_state.select(Some(next_index));
                Ok(None)
            }
            KeyCode::Up => {
                let prev_index = match self.list_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.list_state.select(Some(prev_index));
                Ok(None)
            }
            KeyCode::Enter => {
                if let Some(index) = self.list_state.selected() {
                    if let Some(color) = self.items.get(index) {
                        Ok(Some(*color))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            _ => {
                Ok(None)
            }
        }
    }
}