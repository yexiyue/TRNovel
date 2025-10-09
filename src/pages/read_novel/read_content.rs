use crate::{
    components::Loading,
    hooks::{UseScrollbar, UseThemeConfig},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Flex, Margin},
    text::Line,
    widgets::Paragraph,
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, State, UseEvents, UseFuture, UseMemo, UseState, component,
    element,
    prelude::{Border, Text, View},
};
use std::time::Duration;

#[derive(Default, Props)]
pub struct ReadContentProps {
    pub content: String,
    pub is_scroll: bool,
    pub is_loading: bool,
    pub width: u16,
    pub height: u16,
    pub on_prev: Handler<'static, bool>,
    pub on_next: Handler<'static, ()>,
    pub chapter_name: String,
    pub chapter_percent: f64,
    pub line_percent: Option<State<f64>>,
}

#[component]
pub fn ReadContent(
    props: &mut ReadContentProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();

    let paragraph = hooks.use_memo(
        || {
            let content = textwrap::fill(&props.content, (props.width as usize).saturating_sub(2));
            Paragraph::new(content)
        },
        (props.content.clone(), props.width),
    );
    let line_percent = hooks.use_state(|| 0.0);
    let mut line_percent = props.line_percent.unwrap_or(line_percent);

    let is_scroll = props.is_scroll;
    let line_count = paragraph
        .line_count(props.width.saturating_sub(2))
        .saturating_sub(props.height as usize - 1);

    let mut current_line = hooks.use_memo(
        || (line_percent.get() * line_count as f64 * 1000.0).round() as usize / 1000,
        format!("{}-{}", line_count, line_percent.get()),
    );
    let mut current_time = hooks.use_state(String::default);

    hooks.use_future(async move {
        current_time.set(chrono::Local::now().format("%H:%M").to_string());
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    hooks.use_scrollbar(line_count, Some(current_line));

    let mut on_prev = props.on_prev.take();
    let mut on_next = props.on_next.take();

    hooks.use_local_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_scroll
        {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if current_line > 0 {
                        current_line -= 1;
                        line_percent.set(current_line as f64 / line_count as f64);
                    } else {
                        on_prev(true);
                        line_percent.set(1.0);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if current_line < line_count {
                        current_line += 1;
                        line_percent.set(current_line as f64 / line_count as f64);
                    } else {
                        on_next(());
                        line_percent.set(0.0);
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    on_prev(false);
                    line_percent.set(0.0);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    on_next(());
                    line_percent.set(0.0);
                }
                KeyCode::PageUp => {
                    current_line = current_line.saturating_sub(5);
                    line_percent.set(current_line as f64 / line_count as f64);
                }
                KeyCode::PageDown => {
                    current_line = (current_line + 5).min(line_count);
                    line_percent.set(current_line as f64 / line_count as f64);
                }
                KeyCode::Home => {
                    line_percent.set(0.0);
                }
                KeyCode::End => {
                    line_percent.set(1.0);
                }
                _ => {}
            }
        }
    });

    element!(Border(
        border_style: theme.basic.border,
        top_title: Line::from(format!("{}", props.chapter_name)).style(theme.novel.chapter).centered()
    ){
        #(if props.is_loading {
            element!(Loading(tip:"加载内容中...")).into_any()
        }else{
            element!(Text(text: paragraph, scroll: (current_line as u16,0))).into_any()
        })
        View(
            flex_direction: Direction::Horizontal,
            justify_content: Flex::SpaceBetween,
            height: Constraint::Length(1),
            margin: Margin::new(1,0),
        ){
            $Line::from(format!("{current_line}/{line_count} 行")).style(theme.novel.page)
            $Line::from(format!("{:.2}% {}",props.chapter_percent, current_time.read().clone())).style(theme.novel.progress).right_aligned()
        }
    })
}
