use crate::{
    components::Loading,
    hooks::{UseScrollbar, UseThemeConfig},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Flex, Margin},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_kit::prelude::*;
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
    let mut is_listening = hooks.use_state(|| false);
    let highlight_range = hooks.use_state(|| (0usize, 0usize));

    let novel_tts = *hooks.use_context::<State<Option<novel_tts::NovelTTS>>>();
    let chapter_tts = hooks.use_state(|| None::<novel_tts::ChapterTTS>);
    let player = hooks.use_state(|| None::<novel_tts::Player>);

    // hooks.use_async_effect(
    //     {
    //         let content = props.content.clone();
    //         async move {
    //             if let Some(tts) = novel_tts.read().as_ref() {
    //                 chapter_tts.write().as_ref().map(|c| c.cancel());
    //                 player.write_no_update().as_ref().map(|p| p.stop());

    //                 let mut chapter = tts.chapter_tts();
    //                 let (queue_output, mut receiver) =
    //                     chapter.stream(content, Voice::Zf001(1), |e| {
    //                         eprintln!("{e:?}");
    //                     });

    //                 tokio::spawn(async move {
    //                     while let Some(highlight) = receiver.recv().await {
    //                         highlight_range.set(highlight);
    //                     }
    //                 });
    //                 let p = tts.player(queue_output);
    //                 is_listening.set(true);
    //                 player.set(Some(p));

    //                 chapter_tts.set(Some(chapter));
    //             }
    //         }
    //     },
    //     (props.content.clone(), novel_tts.read().is_some()),
    // );

    let paragraph = hooks.use_memo(
        || {
            let content = textwrap::fill(&props.content, (props.width as usize).saturating_sub(2));
            let highlight_range = highlight_range.get();

            if is_listening.get() {
                Paragraph::new(highlight_text(
                    content,
                    highlight_range.0,
                    highlight_range.1,
                    Style::default().green(),
                ))
            } else {
                Paragraph::new(content)
            }
        },
        (
            props.content.clone(),
            props.width,
            is_listening.get(),
            highlight_range.get(),
        ),
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
                KeyCode::Char('p') => {
                    player.read().as_ref().map(|p| {
                        if is_listening.get() {
                            p.pause();
                            is_listening.set(false);
                        } else {
                            p.play();
                            is_listening.set(true);
                        }
                    });
                }
                _ => {}
            }
        }
    });

    element!(Border(
        border_style: if is_listening.get() {
            theme.basic.border.patch(theme.colors.success_color)
        }else{
            theme.basic.border
        },
        top_title: Line::from(props.chapter_name.to_string()).style(theme.novel.chapter).centered(),
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

fn highlight_text(
    text: String,
    start: usize,
    end: usize,
    highlight_style: Style,
) -> Vec<Line<'static>> {
    let mut last_index = 0;

    let mut lines = vec![];
    for line in text.lines() {
        let mut spans = vec![];
        let count = line.chars().count();
        let new_index = last_index + count;

        if start >= last_index && start < new_index {
            let start_in_line = start - last_index;
            let end_in_line = end.min(new_index) - last_index;

            // 安全地获取字节索引位置
            let char_indices: Vec<_> = line.char_indices().collect();

            // 获取起始字节位置
            let start_byte = if start_in_line < char_indices.len() {
                char_indices[start_in_line].0
            } else {
                line.len()
            };

            // 获取结束字节位置
            let end_byte = if end_in_line < char_indices.len() {
                char_indices[end_in_line].0
            } else {
                line.len()
            };

            let (before, rest) = line.split_at(start_byte);
            let (highlighted, after) = rest.split_at(end_byte - start_byte);

            spans.push(Span::from(before.to_string()));
            spans.push(Span::styled(highlighted.to_string(), highlight_style));
            spans.push(Span::from(after.to_string()));
            lines.push(Line::from(spans));
        } else if end > last_index && end < new_index {
            // 同样需要处理字节索引
            let char_indices: Vec<_> = line.char_indices().collect();
            let end_byte = if end - last_index < char_indices.len() {
                char_indices[end - last_index].0
            } else {
                line.len()
            };

            let (before, rest) = line.split_at(end_byte);
            spans.push(Span::styled(before.to_string(), highlight_style));
            spans.push(Span::from(rest.to_string()));
            lines.push(Line::from(spans));
        } else if last_index > start && new_index < end {
            spans.push(Span::styled(line.to_string(), highlight_style));
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(line.to_string()));
        }

        last_index = new_index;
    }

    lines
}
