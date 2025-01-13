use crate::{app::State, components::Component, THEME_CONFIG};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    style::{Style, Stylize},
    text::Line,
    widgets::Block,
};
use tui_textarea::{Input, Key, TextArea};

type SearchFn = Box<dyn FnMut(String) + Send + Sync + 'static>;
type ValidatorFn = Box<dyn FnMut(&str) -> (bool, &str) + Send + Sync + 'static>;

pub struct Search<'a> {
    pub textarea: TextArea<'a>,
    pub is_focus: bool,
    pub on_search: SearchFn,
    pub validator: ValidatorFn,
    pub is_valid: bool,
    pub error_msg: String,
}

impl Search<'_> {
    pub fn new<T, V>(place_holder: &str, on_search: T, mut validator: V) -> Self
    where
        T: FnMut(String) + Send + Sync + 'static,
        V: FnMut(&str) -> (bool, &str) + Send + Sync + 'static,
    {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_text(place_holder);

        let (is_valid, error_msg) = validator("");
        Self {
            textarea,
            is_focus: false,
            on_search: Box::new(on_search),
            is_valid,
            validator: Box::new(validator),
            error_msg: error_msg.into(),
        }
    }

    pub fn set_value(&mut self, value: &str) {
        let (is_valid, error_msg) = (self.validator)(value);
        self.is_valid = is_valid;
        self.error_msg = error_msg.into();

        let mut textarea = TextArea::new(vec![value.to_string()]);
        textarea.set_placeholder_text(self.textarea.placeholder_text());
        self.textarea = textarea;
    }

    pub fn validate_input(&mut self) -> bool {
        let (is_valid, error_msg) = (self.validator)(self.textarea.lines()[0].as_str());
        self.is_valid = is_valid;
        self.error_msg = error_msg.into();
        self.is_valid
    }

    pub fn get_value(&self) -> &str {
        self.textarea.lines()[0].as_str()
    }
}

#[async_trait]
impl Component for Search<'_> {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        self.textarea.set_style(THEME_CONFIG.search.text);
        self.textarea
            .set_placeholder_style(THEME_CONFIG.search.placeholder);
        if self.is_focus {
            if !self.is_valid {
                self.textarea.set_block(
                    Block::bordered()
                        .title(
                            Line::from(self.error_msg.clone())
                                .style(THEME_CONFIG.search.error_border_info),
                        )
                        .border_style(THEME_CONFIG.search.error_border),
                );
            } else {
                self.textarea
                    .set_block(Block::bordered().border_style(THEME_CONFIG.search.success_border));
            }
            self.textarea
                .set_cursor_style(Style::default().on_dark_gray());
        } else {
            self.textarea
                .set_block(Block::bordered().border_style(THEME_CONFIG.basic.border));
            self.textarea.set_cursor_style(Style::default());
        }

        frame.render_widget(&self.textarea, area);
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        _state: State,
    ) -> crate::Result<Option<KeyEvent>> {
        if self.is_focus {
            match Input::from(key) {
                Input { key: Key::Esc, .. } => {
                    self.is_focus = false;
                    Ok(None)
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    if self.is_valid {
                        self.is_focus = false;
                        let res = self.textarea.lines()[0].clone();
                        (self.on_search)(res);
                    }
                    Ok(None)
                }
                input => {
                    self.textarea.input(input);
                    self.validate_input();
                    Ok(None)
                }
            }
        } else if key.code == KeyCode::Char('s') && key.kind == KeyEventKind::Press {
            self.is_focus = true;
            Ok(None)
        } else {
            Ok(Some(key))
        }
    }
}
