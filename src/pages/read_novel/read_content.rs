use crate::{
    TTSConfig,
    components::Loading,
    hooks::{UseScrollbar, UseThemeConfig},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Flex, Margin},
    style::Style,
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
    // let highlight_range = hooks.use_state(|| (0usize, 0usize));

    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let novel_tts = *hooks.use_context::<State<Option<novel_tts::NovelTTS>>>();
    let mut chapter_tts = hooks.use_state(|| None::<novel_tts::ChapterTTS>);
    let mut player = hooks.use_state(|| None::<novel_tts::Player>);

    hooks.use_effect(
        move || {
            if let Some(player) = player.write().take() {
                player.sink.stop();
            }
            if let Some(chapter_tts) = chapter_tts.write().take() {
                chapter_tts.cancel();
            }
            is_listening.set(false);
        },
        props.content.clone(),
    );

    hooks.use_effect(
        || {
            if let Some(player) = player.write().as_mut() {
                player.set_speed(tts_config.read().speed);
                player.set_volume(tts_config.read().volume);
            }
        },
        format!("{}-{}", tts_config.read().speed, tts_config.read().volume),
    );

    hooks.use_async_effect(
        {
            let content = props.content.clone();
            async move {
                if let Some(tts) = novel_tts.read().as_ref()
                    && tts_config.read().auto_play
                    && player.read().is_none()
                    && chapter_tts.read().is_none()
                {
                    let mut chapter = tts.chapter_tts();
                    let (queue_output, _receiver) =
                        chapter.stream(content, tts_config.read().voice.into(), |e| {
                            eprintln!("{e:?}");
                        });

                    // tokio::spawn(async move {
                    //     while let Some(highlight) = receiver.recv().await {
                    //         highlight_range.set(highlight);
                    //     }
                    // });
                    let p = tts.player(queue_output);
                    p.set_speed(tts_config.read().speed);
                    p.set_volume(tts_config.read().volume);
                    is_listening.set(true);
                    player.set(Some(p));
                    chapter_tts.set(Some(chapter));
                }
            }
        },
        (
            props.content.clone(),
            novel_tts.read().is_some(),
            // tts_config.read().voice, (暂时不支持立即切换声音)
        ),
    );

    let paragraph = hooks.use_memo(
        || {
            let content = textwrap::fill(&props.content, (props.width as usize).saturating_sub(2));
            // let highlight_range = highlight_range.get();

            // if is_listening.get() {
            //     Paragraph::new(highlight_text(
            //         content,
            //         highlight_range.0,
            //         highlight_range.1,
            //         Style::default().green(),
            //     ))
            // } else {
            //     Paragraph::new(content)
            // }
            Paragraph::new(content)
        },
        (
            props.content.clone(),
            props.width,
            // is_listening.get(),
            // highlight_range.get(),
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

    let props_content = props.content.clone();
    hooks.use_events(move |event| {
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
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if current_line < line_count {
                        current_line += 1;
                        line_percent.set(current_line as f64 / line_count as f64);
                    } else {
                        on_next(());
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    on_prev(false);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    on_next(());
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
                KeyCode::Char('+') => {
                    tts_config.write().increase_volume();
                }
                KeyCode::Char('-') => {
                    tts_config.write().decrease_volume();
                }
                KeyCode::Char('p') => {
                    if let Some(player) = player.read().as_ref() {
                        if is_listening.get() {
                            player.pause();
                            is_listening.set(false);
                        } else {
                            player.play();
                            is_listening.set(true);
                        }
                    } else if let Some(tts) = novel_tts.read().as_ref() {
                        let mut chapter = tts.chapter_tts();
                        let (queue_output, _receiver) = chapter.stream(
                            props_content.clone(),
                            tts_config.read().voice.into(),
                            |e| {
                                eprintln!("{e:?}");
                            },
                        );

                        // tokio::spawn(async move {
                        //     while let Some(highlight) = receiver.recv().await {
                        //         highlight_range.set(highlight);
                        //     }
                        // });
                        let p = tts.player(queue_output);
                        p.set_speed(tts_config.read().speed);
                        p.set_volume(tts_config.read().volume);
                        is_listening.set(true);
                        player.set(Some(p));
                        chapter_tts.set(Some(chapter));
                    }
                }
                _ => {}
            }
        }
    });

    element!(Border(
        border_style: theme.basic.border,
        top_title: Line::from(props.chapter_name.to_string()).style(theme.novel.chapter).centered(),
        bottom_title: (if is_listening.get(){
            Line::from(
                format!(
                    "播放中: 播放速度{} / 音量{}",
                    tts_config.read().speed,
                    tts_config.read().volume,
                )
            )
            .style(theme.novel.page)
        }else{
            Line::from("按 p 播放/暂停").style(theme.novel.page)
        }).style(theme.novel.page).centered(),
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

pub fn highlight_text(
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
