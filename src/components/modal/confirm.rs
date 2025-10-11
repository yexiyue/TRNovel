use crate::hooks::UseThemeConfig;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Flex, Margin},
    style::{Style, Stylize},
    text::Line,
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEvents, UseState, component, element,
    prelude::{Border, Modal, Text, View},
};

#[derive(Props, Default)]
pub struct ConfirmModalProps {
    pub title: String,
    pub content: String,
    pub open: bool,
    pub on_confirm: Handler<'static, ()>,
    pub on_cancel: Handler<'static, ()>,
}

#[component]
pub fn ConfirmModal(
    props: &mut ConfirmModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let confirm = hooks.use_state(|| false);

    hooks.use_events({
        let mut on_confirm = props.on_confirm.take();
        let mut on_cancel = props.on_cancel.take();
        let open = props.open;

        move |event: Event| {
            if !open {
                return;
            }

            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Left | KeyCode::Right => {
                        *confirm.write() = !confirm.get();
                    }
                    KeyCode::Enter => {
                        if confirm.get() {
                            on_confirm(());
                        } else {
                            on_cancel(());
                        }
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        on_confirm(());
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        on_cancel(());
                    }
                    _ => {}
                }
            }
        }
    });

    let (cancel_style, confirm_style) = if *confirm.read() {
        (
            Style::default(),
            Style::default().fg(theme.colors.error_color),
        )
    } else {
        (
            Style::default().fg(theme.colors.primary_color),
            Style::default(),
        )
    };

    element!(Modal(
        open: props.open,
        width: Constraint::Percentage(50),
        height: Constraint::Length(10),
        style: Style::default().dim()
    ){
        Border(
            border_style: theme.warning_modal.border,
            top_title: Some(Line::from(props.title.clone()).style(theme.warning_modal.border_title).centered())
        ){
            View{
                View(
                    height: Constraint::Fill(1),
                    margin: Margin::new(2, 2),
                ){
                    Text(
                        text: props.content.clone(),
                        style: theme.warning_modal.text,
                        alignment: Alignment::Center,
                    )
                }
                View(
                    justify_content: Flex::SpaceAround,
                    height: Constraint::Length(3),
                    flex_direction: Direction::Horizontal,
                ){
                    View(
                        width: Constraint::Length(10),
                    ){
                        Border(
                            style: cancel_style,
                        ){
                            Text(
                                text: "取消".to_string(),
                                alignment: Alignment::Center,
                            )
                        }
                    }
                    View(
                        width: Constraint::Length(10),
                    ){
                        Border(
                            style: confirm_style,
                        ){
                            Text(
                                text: "确认".to_string(),
                                alignment: Alignment::Center,
                            )
                        }
                    }
                }
            }
        }
    })
}
