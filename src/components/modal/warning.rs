use crossterm::event::{Event, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Margin},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Padding, Paragraph, Widget, Wrap},
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEvents, UseExit, component, element,
    prelude::{Border, Modal, Text},
};

use crate::THEME_CONFIG;

#[derive(Debug, Clone)]
pub struct Warning {
    pub tip: String,
    pub is_error: bool,
}

impl Warning {
    pub fn new(tip: &str, is_error: bool) -> Self {
        Self {
            tip: tip.into(),
            is_error,
        }
    }
}

impl Widget for Warning {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let [vertical] = Layout::vertical([Constraint::Length(8)])
            .flex(Flex::Center)
            .areas(area);
        let [horizontal] = Layout::horizontal([Constraint::Percentage(70)])
            .flex(Flex::Center)
            .areas(vertical);

        if self.is_error {
            Clear.render(area, buf);
        } else {
            Clear.render(horizontal, buf);
        }

        let block = if self.is_error {
            Block::bordered()
                .title(Line::from("错误").style(THEME_CONFIG.error_modal.border_title))
                .title_alignment(Alignment::Center)
                .title_bottom(Line::from("按q退出").style(THEME_CONFIG.error_modal.border_info))
                .title_alignment(Alignment::Center)
                .border_style(THEME_CONFIG.error_modal.border)
                .padding(Padding::uniform(1))
        } else {
            Block::bordered()
                .title(Line::from("警告").style(THEME_CONFIG.warning_modal.border_title))
                .title_alignment(Alignment::Center)
                .title_bottom(
                    Line::from("按ESC键继续").style(THEME_CONFIG.warning_modal.border_info),
                )
                .title_alignment(Alignment::Center)
                .border_style(THEME_CONFIG.warning_modal.border)
                .padding(Padding::uniform(1))
        };

        Block::new().dim().render(area, buf);

        Paragraph::new(self.tip)
            .centered()
            .style(if self.is_error {
                THEME_CONFIG.error_modal.text
            } else {
                THEME_CONFIG.warning_modal.text
            })
            .wrap(Wrap { trim: true })
            .block(block)
            .not_dim()
            .render(horizontal.inner(Margin::new(2, 0)), buf);
    }
}

#[derive(Props, Default)]
pub struct WarningModalProps {
    pub tip: String,
    pub is_error: bool,
    pub open: bool,
    pub on_close: Handler<'static, ()>,
}

#[component]
pub fn WarningModal(
    props: &mut WarningModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let mut exit = hooks.use_exit();

    hooks.use_events({
        let is_error = props.is_error;
        let mut handle = props.on_close.take();

        move |event| {
            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    crossterm::event::KeyCode::Esc if !is_error => {
                        handle(());
                    }
                    crossterm::event::KeyCode::Char('q') if is_error => {
                        if handle.is_default() {
                            exit();
                        } else {
                            handle(());
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    element!(Modal(
        open: props.open,
        width: Constraint::Percentage(50),
        height: Constraint::Length(6),
        style: Style::new().dim(),
    ){
        Border(
            top_title: Some(Line::from(if props.is_error {"错误"} else {"警告"}).style(
                if props.is_error {
                    THEME_CONFIG.error_modal.border_title
                } else {
                    THEME_CONFIG.warning_modal.border_title
                }
            ).centered()),
            bottom_title: Some(Line::from(if props.is_error {"按q退出"} else {"按ESC键继续"}).style(
                if props.is_error {
                    THEME_CONFIG.error_modal.border_info
                } else {
                    THEME_CONFIG.warning_modal.border_info
                }
            ).centered()),
            style: if props.is_error {
                THEME_CONFIG.error_modal.border.not_dim()
            } else {
                THEME_CONFIG.warning_modal.border.not_dim()
            },
            padding: Padding::uniform(1),
        ){
            Text(
                text: props.tip.clone(),
                alignment:Alignment::Center,
            )
        }
    })
}
