use async_trait::async_trait;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Flex, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{List, ListState, Paragraph, Wrap},
};
use tui_big_text::{BigText, PixelSize};

use crate::{
    components::{Component, KeyShortcutInfo},
    RoutePage, Router,
};

use super::{
    local_novel::local_novel_first_page, network_novel::network_novel_first_page,
    select_history::SelectHistory, Page, PageWrapper,
};

pub struct Home {
    pub state: ListState,
    pub navigator: crate::Navigator,
}

impl Home {
    pub fn to_page_route() -> Box<dyn RoutePage> {
        Box::new(PageWrapper::<Self, ()>::new((), None))
    }
}

#[async_trait]
impl Component for Home {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        let [center] = Layout::vertical([Constraint::Length(10)])
            .flex(Flex::Center)
            .areas(area);

        let [left_top, left_bottom, other] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Max(2),
            Constraint::Fill(1),
        ])
        .areas(center);

        let [list_area] = Layout::horizontal([Constraint::Length(10)])
            .flex(Flex::Center)
            .areas(other);

        let big_txt = BigText::builder()
            .pixel_size(PixelSize::Quadrant)
            .lines(vec!["TRNovel".into()])
            .centered()
            .style(Style::new().light_green())
            .build();
        let info_txt = Paragraph::new(vec!["终端小说阅读器 (Terminal reader for novel)".into()])
            .wrap(Wrap { trim: true })
            .centered();

        frame.render_widget(big_txt, left_top);
        frame.render_widget(info_txt, left_bottom);

        let list = List::new(vec![
            Line::from("本地小说").centered(),
            Line::from("网络小说").centered(),
            Line::from("历史记录").centered(),
        ])
        .highlight_style(Style::new().bold().on_light_cyan().black());
        frame.render_stateful_widget(list, list_area, &mut self.state);

        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _state: crate::app::State,
    ) -> crate::Result<Option<crossterm::event::KeyEvent>> {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(Some(key));
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.select_next();
                Ok(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.select_previous();
                Ok(None)
            }
            KeyCode::Enter => {
                if let Some(index) = self.state.selected() {
                    match index {
                        0 => {
                            self.navigator.push(local_novel_first_page(None))?;
                        }
                        1 => {
                            self.navigator.push(network_novel_first_page()?)?;
                        }
                        2 => {
                            self.navigator.push(SelectHistory::to_page_route())?;
                        }
                        _ => {}
                    }
                }

                Ok(None)
            }
            _ => {
                return Ok(Some(key));
            }
        }
    }

    fn key_shortcut_info(&self) -> crate::components::KeyShortcutInfo {
        KeyShortcutInfo::new(vec![
            ("选择下一个", "J / ▼"),
            ("选择上一个", "K / ▲"),
            ("确认选择", "Enter"),
        ])
    }
}

#[async_trait]
impl Page for Home {
    type Msg = ();
    async fn init(
        _arg: (),
        _sender: tokio::sync::mpsc::Sender<Self::Msg>,
        navigator: crate::Navigator,
        _state: crate::app::State,
    ) -> crate::Result<Self> {
        let mut state = ListState::default();
        state.select(Some(0));

        Ok(Self { state, navigator })
    }
}

impl Router for Home {}
