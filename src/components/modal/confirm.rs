use crate::{THEME_CONFIG, hooks::UseThemeConfig};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Flex, Layout, Margin},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEvents, UseState, component, element,
    prelude::{Border, Modal, Text, View},
};

#[derive(Debug, Clone)]
pub struct Confirm {
    pub title: String,
    pub content: String,
}

impl Confirm {
    pub fn new(title: &str, content: &str) -> Self {
        Self {
            title: title.to_string(),
            content: content.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfirmState {
    pub show: bool,
    pub confirm: bool,
}

impl ConfirmState {
    pub fn confirm(&mut self) {
        self.confirm = true;
    }

    pub fn toggle(&mut self) {
        self.confirm = !self.confirm;
    }

    pub fn hide(&mut self) {
        self.show = false;
        self.confirm = false;
    }

    pub fn is_confirm(&self) -> bool {
        self.confirm
    }

    pub fn show(&mut self) {
        self.show = true;
    }
}

impl StatefulWidget for Confirm {
    type State = ConfirmState;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        if state.show {
            Block::new().dim().render(area, buf);

            let [horizontal] = Layout::horizontal([Constraint::Percentage(70)])
                .flex(Flex::Center)
                .areas(area);

            let [block_area] = Layout::vertical([Constraint::Max(10)])
                .flex(Flex::Center)
                .areas(horizontal);

            Clear.render(block_area, buf);

            let block = Block::bordered()
                .title(self.title.as_str())
                .title_alignment(Alignment::Center)
                .title_style(THEME_CONFIG.warning_modal.border_title)
                .border_style(THEME_CONFIG.warning_modal.border);

            let inner_area = block.inner(block_area);

            block.render(block_area.inner(Margin::new(2, 0)), buf);

            let [content_area, bottom_area] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(inner_area);

            Paragraph::new(self.content.as_str())
                .style(THEME_CONFIG.warning_modal.text)
                .wrap(Wrap { trim: true })
                .centered()
                .render(content_area.inner(Margin::new(2, 2)), buf);

            let [left, right] =
                Layout::horizontal([Constraint::Length(10), Constraint::Length(10)])
                    .flex(Flex::SpaceAround)
                    .areas(bottom_area);

            let (cancel_style, confirm_style) = if state.confirm {
                (
                    Style::default(),
                    Style::default().fg(THEME_CONFIG.colors.error_color),
                )
            } else {
                (
                    Style::default().fg(THEME_CONFIG.colors.primary_color),
                    Style::default(),
                )
            };

            Paragraph::new("取消")
                .alignment(Alignment::Center)
                .block(Block::bordered())
                .style(cancel_style)
                .render(left, buf);

            Paragraph::new("确认")
                .alignment(Alignment::Center)
                .block(Block::bordered())
                .style(confirm_style)
                .render(right, buf);
        }
    }
}

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
                    KeyCode::Esc => {
                        on_confirm(());
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
