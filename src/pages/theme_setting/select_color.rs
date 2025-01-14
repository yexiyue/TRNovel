use crate::{
    app::State,
    components::{Component, Search},
    Result, THEME_CONFIG,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout},
    style::Color,
    text::{Line, ToText},
    widgets::{Block, List, ListState, Padding, Scrollbar, ScrollbarState},
};
use std::str::FromStr;

pub struct SelectColor {
    pub search: Search<'static>,
    pub items: Vec<Color>,
    pub list_state: ListState,
    pub on_select: Box<dyn FnMut(Color) + Send + Sync + 'static>,
}

impl SelectColor {
    pub fn new<T>(on_select: T) -> Self
    where
        T: FnMut(Color) -> () + Send + Sync + 'static + Clone,
    {
        let mut on_select_clone = on_select.clone();
        let search = Search::new(
            "请输入颜色",
            move |color: String| {
                on_select_clone(Color::from_str(color.trim()).unwrap());
            },
            |color: &str| match Color::from_str(color.trim()) {
                Ok(_) => (true, ""),
                Err(_) => (false, "Invalid color"),
            },
        );

        Self {
            search,
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
            on_select: Box::new(on_select),
        }
    }

    pub fn select(&mut self, color: Color) {
        let index = self.items.iter().position(|c| *c == color);
        if let Some(index) = index {
            self.list_state.select(Some(index));
        } else {
            self.search.set_value(&color.to_string());
        }
    }

    pub fn reset(&mut self) {
        self.list_state.select(None);
        self.search.set_value("");
    }
}

#[async_trait]
impl Component for SelectColor {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        let [top, content] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        let block = Block::bordered()
            .title(
                Line::from("选择颜色")
                    .centered()
                    .style(THEME_CONFIG.basic.border_title),
            )
            .border_style(THEME_CONFIG.basic.border)
            .padding(Padding::horizontal(1));

        self.search.render(frame, top)?;

        let items = self
            .items
            .iter()
            .map(|color| color.to_text().style(THEME_CONFIG.basic.text.fg(*color)));

        let mut scrollbar_state =
            ScrollbarState::new(items.len()).position(self.list_state.selected().unwrap_or(0));

        let list = List::new(items).highlight_symbol("> ");
        frame.render_stateful_widget(list, block.inner(content), &mut self.list_state);

        frame.render_widget(block, content);

        frame.render_stateful_widget(Scrollbar::default(), content, &mut scrollbar_state);
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent, state: State) -> Result<Option<KeyEvent>> {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(Some(key));
        }
        let Some(key) = self.search.handle_key_event(key, state).await? else {
            return Ok(None);
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.list_state.select_next();
                Ok(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.list_state.select_previous();
                Ok(None)
            }
            KeyCode::Enter => {
                if let Some(index) = self.list_state.selected() {
                    let color = self.items[index];
                    (self.on_select)(color);
                }

                Ok(None)
            }
            _ => {
                return Ok(Some(key));
            }
        }
    }
}
