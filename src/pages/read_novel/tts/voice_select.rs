use std::time::Duration;

use super::settings::{SettingItem, SettingItemProps};
use crate::{
    TTSConfig, Voices,
    hooks::{DebounceOptions, UseDebounceEffect, UseThemeConfig},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Flex},
    style::Stylize,
    text::Line,
};
use ratatui_kit::prelude::*;
use strum::IntoEnumIterator;

#[component]
pub fn VoiceSelect(props: &SettingItemProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let theme = hooks.use_theme_config();
    let mut current_voice = hooks.use_state(|| tts_config.read().voice);

    let is_editing = props.is_editing;

    let (prev, current, next) = hooks.use_memo(
        || {
            let data = Voices::iter().collect::<Vec<_>>();
            let index = Voices::iter()
                .position(|i| i == current_voice.get())
                .unwrap_or(0);
            let prev = if index == 0 {
                data.len() - 1
            } else {
                index - 1
            };
            let next = if index + 1 >= data.len() {
                0
            } else {
                index + 1
            };
            (data[prev], data[index], data[next])
        },
        current_voice,
    );

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_editing
        {
            match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    current_voice.set(prev);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    current_voice.set(next);
                }
                _ => {}
            }
        }
    });

    hooks.use_debounce_effect(
        move || {
            tts_config.write().voice = current_voice.get();
        },
        current_voice.get(),
        DebounceOptions::default().wait(Duration::from_secs(1)),
    );

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction: Direction::Horizontal, gap:1){
            View(width:Constraint::Length(18)) {
                $Line::from("选择声音: ".to_string()).style(theme.basic.text)
            }
            View(flex_direction: Direction::Horizontal, gap:1,justify_content:Flex::Center) {
                View(width:Constraint::Length(8)) {
                    $Line::from("<").style(theme.basic.text).dim().centered()
                }
                View(width:Constraint::Length(8)) {
                    $Line::from(prev.to_string()).style(theme.basic.text).dim().centered()
                }
                View(width:Constraint::Length(8)) {
                    $Line::from(current.to_string()).style(theme.basic.text).bold().centered()
                }
                View(width:Constraint::Length(8)) {
                    $Line::from(next.to_string()).style(theme.basic.text).dim().centered()
                }
                View(width:Constraint::Length(8)) {
                    $Line::from(">").style(theme.basic.text).dim().centered()
                }
            }
        }
    })
}
