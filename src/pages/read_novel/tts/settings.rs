use crate::{TTSConfig, hooks::UseThemeConfig};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Direction, Flex},
    style::Stylize,
    text::Line,
};
use ratatui_kit::prelude::*;

#[derive(Props, Default)]
pub struct SettingItemProps {
    pub is_editing: bool,
    pub top_title: String,
    pub bottom_title: String,
    pub children: Vec<AnyElement<'static>>,
}

#[component]
pub fn SettingItem(props: &mut SettingItemProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();

    let mut top_title = Line::from(props.top_title.clone());
    let mut bottom_title = Line::from(props.bottom_title.clone());

    if props.is_editing {
        top_title = top_title.not_dim();
        bottom_title = bottom_title.not_dim();
    }

    let border_style = if props.is_editing {
        theme.basic.border.patch(theme.highlight)
    } else {
        theme.basic.border
    };

    element!(Border(
        top_title: top_title,
        border_style: border_style,
        bottom_title: bottom_title,
        style: if props.is_editing {
            theme.basic.border.not_dim()
        } else {
            theme.basic.border
        }
    ) {
        #(&mut props.children)
    })
}

#[component]
pub fn SpeedSetting(
    props: &mut SettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let is_editing = props.is_editing;

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_editing
        {
            match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    tts_config.write().decrease_speed();
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    tts_config.write().increase_speed();
                }
                _ => {}
            }
        }
    });

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction:Direction::Horizontal,justify_content:Flex::SpaceBetween) {
            $Line::from("播放速度:").style(theme.basic.text)
            $Line::from(format!("{}x", tts_config.read().speed)).style(theme.basic.text)
        }
    })
}

#[component]
pub fn VolumeSetting(
    props: &mut SettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let is_editing = props.is_editing;

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_editing
        {
            match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    tts_config.write().decrease_volume();
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    tts_config.write().increase_volume();
                }
                _ => {}
            }
        }
    });

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction:Direction::Horizontal,justify_content:Flex::SpaceBetween) {
            $Line::from("音量:").style(theme.basic.text)
            $Line::from(format!("{}x", tts_config.read().volume)).style(theme.basic.text)
        }
    })
}

#[component]
pub fn AutoPlaySetting(
    props: &mut SettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let is_editing = props.is_editing;

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_editing
        {
            match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    tts_config.write().auto_play = false;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    tts_config.write().auto_play = true;
                }
                _ => {}
            }
        }
    });

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction:Direction::Horizontal,justify_content:Flex::SpaceBetween) {
            $Line::from("自动播放:").style(theme.basic.text)
            $Line::from(format!("{}", tts_config.read().auto_play)).style(theme.basic.text)
        }
    })
}
