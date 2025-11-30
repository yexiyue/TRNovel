use crate::{TTSConfig, ThemeConfig, components::Loading, hooks::UseScrollbar};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use novel_tts::utils::TextSegment;
use ratatui::{
    layout::{Constraint, Direction, Flex, Margin},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_kit::prelude::*;
use std::{ops::Not, time::Duration};

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
    let theme_config = *hooks.use_context::<State<ThemeConfig>>();
    let theme = theme_config.read().clone();
    let mut is_listening = hooks.use_state(|| false);
    let mut highlight_range = hooks.use_state(|| None::<TextSegment>);
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let novel_tts = *hooks.use_context::<State<Option<novel_tts::NovelTTS>>>();
    let mut chapter_tts = hooks.use_state(|| None::<novel_tts::ChapterTTS>);
    let mut player = hooks.use_state(|| None::<novel_tts::Player>);
    let mut is_listening_done = hooks.use_state(|| false);
    let mut on_prev = props.on_prev.take();
    let mut on_next = props.on_next.take();

    // 自动播放下一章节
    if is_listening_done.get() && tts_config.read().auto_play {
        on_next(());
        is_listening_done.set(false);
    }

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
                    && chapter_tts.read().is_none()
                {
                    let mut chapter = if let Some(chapter_tts) = chapter_tts.read().as_ref() {
                        chapter_tts.cancel();
                        chapter_tts.clone()
                    } else {
                        tts.chapter_tts(&content)
                    };

                    let (queue_output, mut receiver) =
                        chapter.stream(tts_config.read().voice.into(), |e| {
                            eprintln!("{e:?}");
                        });

                    let texts = chapter.texts.clone();
                    tokio::spawn(async move {
                        while let Some(index) = receiver.recv().await {
                            if let Some(index) = index {
                                highlight_range.set(Some(texts[index].clone()));
                            } else {
                                is_listening_done.set(true);
                            }
                        }
                    });

                    let p = tts.player(queue_output);
                    p.set_speed(tts_config.read().speed);
                    p.set_volume(tts_config.read().volume);

                    is_listening.set(true);
                    player.set(Some(p));
                    chapter_tts.set(Some(chapter));
                }
            }
        },
        (props.content.clone(), novel_tts.read().is_some()),
    );

    hooks.use_async_effect(
        async move {
            if let Some(tts) = novel_tts.read().as_ref()
                && let Some(chapter) = chapter_tts.write().as_mut()
            {
                let (queue_output, mut receiver) =
                    chapter.stream(tts_config.read().voice.into(), |e| {
                        eprintln!("{e:?}");
                    });

                let texts = chapter.texts.clone();
                tokio::spawn(async move {
                    while let Some(index) = receiver.recv().await {
                        if let Some(index) = index {
                            highlight_range.set(Some(texts[index].clone()));
                        } else {
                            is_listening_done.set(true);
                        }
                    }
                });

                let p = tts.player(queue_output);
                p.set_speed(tts_config.read().speed);
                p.set_volume(tts_config.read().volume);

                is_listening.set(true);
                player.set(Some(p));
            }
        },
        tts_config.read().voice,
    );

    let paragraph = hooks.use_memo(
        || {
            if let Some(segment) = highlight_range.read().as_ref()
                && is_listening.get()
            {
                Paragraph::new(highlight(
                    &props.content,
                    segment,
                    (props.width as usize).saturating_sub(2),
                    Style::from(theme.colors.success_color),
                ))
            } else {
                Paragraph::new(textwrap::fill(
                    &props.content,
                    (props.width as usize).saturating_sub(2),
                ))
            }
        },
        (
            is_listening.get(),
            highlight_range.read().clone(),
            &props.content.clone(),
            props.width,
        ),
    );

    let line_percent = hooks.use_state(|| 0.0);
    let mut line_percent = props.line_percent.unwrap_or(line_percent);

    let is_scroll = props.is_scroll;
    let line_count = paragraph
        .line_count(props.width.saturating_sub(2))
        .saturating_sub((props.height as usize) - 3);

    let mut current_line = hooks.use_memo(
        || ((line_percent.get() * (line_count as f64) * 1000.0).round() as usize) / 1000,
        format!("{}-{}", line_count, line_percent.get()),
    );
    let mut current_time = hooks.use_state(String::default);

    hooks.use_future(async move {
        current_time.set(chrono::Local::now().format("%H:%M").to_string());
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    hooks.use_scrollbar(line_count, Some(current_line));

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
                        line_percent.set((current_line as f64) / (line_count as f64));
                    } else {
                        on_prev(true);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if current_line < line_count {
                        current_line += 1;
                        line_percent.set((current_line as f64) / (line_count as f64));
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
                    line_percent.set((current_line as f64) / (line_count as f64));
                }
                KeyCode::PageDown => {
                    current_line = (current_line + 5).min(line_count);
                    line_percent.set((current_line as f64) / (line_count as f64));
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
                        let mut chapter = tts.chapter_tts(&props_content);
                        let (queue_output, mut receiver) =
                            chapter.stream(tts_config.read().voice.into(), |e| {
                                eprintln!("{e:?}");
                            });

                        let texts = chapter.texts.clone();
                        tokio::spawn(async move {
                            while let Some(index) = receiver.recv().await {
                                if let Some(index) = index {
                                    highlight_range.set(Some(texts[index].clone()));
                                } else {
                                    is_listening_done.set(true);
                                }
                            }
                        });
                        let p = tts.player(queue_output);
                        p.set_speed(tts_config.read().speed);
                        p.set_volume(tts_config.read().volume);
                        is_listening.set(true);
                        player.set(Some(p));
                        chapter_tts.set(Some(chapter));
                    }
                }
                KeyCode::Char('v') => {
                    theme_config.write().novel.show_title = theme.novel.show_title.not();
                }
                _ => {}
            }
        }
    });

    element!(Border(
        border_style: theme.basic.border,
        top_title: if theme.novel.show_title {
            Some(Line::from(props.chapter_name.to_string()).style(theme.novel.chapter).centered())
        }else{
            None
        },
        bottom_title: if theme.novel.show_title {
           Some((if is_listening.get(){
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
            }).style(theme.novel.page).centered())
        }else{
            None
        },
    ){
        #(if props.is_loading {
            element!(Loading(tip:"加载内容中...")).into_any()
        }else{
            element!(Text(
                text: paragraph,
                style:theme.basic.text,
                scroll: (current_line as u16,0))
            ).into_any()
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

pub fn highlight(
    text: &str,
    segment: &TextSegment,
    width: usize,
    highlight_style: Style,
) -> Vec<Line<'static>> {
    let pattern: String = regex::escape(&segment.text);
    let regex = regex::Regex::new(&pattern).unwrap();
    let res = regex.find_at(text, segment.start);

    if let Some(mat) = res {
        let marked = format!(
            "{}\u{001E}{}\u{002E}{}",
            &text[..mat.start()],
            mat.as_str(),
            &text[mat.end()..]
        );
        let highlighted = textwrap::fill(&marked, width);
        // let re_mark = Regex::new(r"(?ms)<b>(.*?)</b>").unwrap();
        // let mat = re_mark.find(&highlighted).unwrap();
        highlight_text(&highlighted, highlight_style)
    } else {
        let texts = textwrap::fill(text, width);
        texts
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect::<Vec<_>>()
    }
}

fn highlight_text(text: &str, highlight_style: Style) -> Vec<Line<'static>> {
    let mut lines = vec![];
    let mut matched = 0;

    for line in text.lines() {
        let mut spans = vec![];
        let mut highlight = line.to_string();
        if let Some((start, rest)) = line.split_once('\u{001E}') {
            matched += 1;
            spans.push(Span::from(start.to_string()));
            highlight = rest.to_string();
        }

        if matched > 0 {
            if let Some((highlight, end)) = &highlight.split_once('\u{002E}') {
                matched -= 1;
                spans.push(Span::from(highlight.to_string()).style(highlight_style));
                spans.push(Span::from(end.to_string()));
            } else {
                spans.push(Span::from(highlight).style(highlight_style));
            }
        } else {
            spans.push(Span::from(line.to_string()));
        }
        lines.push(Line::from(spans));
    }

    lines
}
