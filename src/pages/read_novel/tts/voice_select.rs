use super::settings::{SettingItem, SettingItemProps};
use crate::{TTSConfig, Voices, hooks::UseThemeConfig};
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
    let current_voice = tts_config.read().voice;
    let is_editing = props.is_editing;

    let (prev, current, next) = hooks.use_memo(
        || {
            let data = Voices::iter().collect::<Vec<_>>();
            let index = Voices::iter().position(|i| i == current_voice).unwrap_or(0);
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
                    tts_config.write().voice = prev;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    tts_config.write().voice = next;
                }
                _ => {}
            }
        }
    });

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction: Direction::Horizontal, gap:1){
            View(width:Constraint::Length(10)) {
                $Line::from("选择声音:").style(theme.basic.text)
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
