use crossterm::event::{Event, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    style::{Style, Stylize},
    text::Line,
    widgets::Padding,
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEvents, UseExit, component, element,
    prelude::{Border, Modal, Text},
};

use crate::hooks::UseThemeConfig;

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
    let theme = hooks.use_theme_config();
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
                    theme.error_modal.border_title
                } else {
                    theme.warning_modal.border_title
                }
            ).centered()),
            bottom_title: Some(Line::from(if props.is_error {"按q退出"} else {"按ESC键继续"}).style(
                if props.is_error {
                    theme.error_modal.border_info
                } else {
                    theme.warning_modal.border_info
                }
            ).centered()),
            style: if props.is_error {
                theme.error_modal.border.not_dim()
            } else {
                theme.warning_modal.border.not_dim()
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
