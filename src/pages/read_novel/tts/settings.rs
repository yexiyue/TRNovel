use crate::{TTSConfig, theme::AppChromeTheme};
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
    let theme = hooks.use_component_theme::<AppChromeTheme>();

    let mut top_title = Line::from(props.top_title.clone());
    let mut bottom_title = Line::from(props.bottom_title.clone());

    if props.is_editing {
        top_title = top_title.not_dim();
        bottom_title = bottom_title.not_dim();
    }

    let border_style = if props.is_editing {
        theme.border.patch(theme.highlight)
    } else {
        theme.border
    };

    element!(Border(
        top_title: top_title,
        border_style: border_style,
        bottom_title: bottom_title,
        style: if props.is_editing {
            theme.border.not_dim()
        } else {
            theme.border
        }
    ) {
        { std::mem::take(&mut props.children) }
    })
}

#[component]
pub fn SpeedSetting(
    props: &mut SettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let is_editing = props.is_editing;

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        if !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                tts_config.write().decrease_speed();
                EventResult::Consumed
            }
            KeyCode::Right | KeyCode::Char('l') => {
                tts_config.write().increase_speed();
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction:Direction::Horizontal,justify_content:Flex::SpaceBetween) {
            widget(Line::from("播放速度:").style(theme.text))
            widget(Line::from(format!("{}x", tts_config.read().speed)).style(theme.text))
        }
    })
}

#[component]
pub fn VolumeSetting(
    props: &mut SettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let is_editing = props.is_editing;

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        if !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                tts_config.write().decrease_volume();
                EventResult::Consumed
            }
            KeyCode::Right | KeyCode::Char('l') => {
                tts_config.write().increase_volume();
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction:Direction::Horizontal,justify_content:Flex::SpaceBetween) {
            widget(Line::from("音量:").style(theme.text))
            widget(Line::from(format!("{}x", tts_config.read().volume)).style(theme.text))
        }
    })
}

#[component]
pub fn AutoPlaySetting(
    props: &mut SettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let is_editing = props.is_editing;

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        if !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                tts_config.write().auto_play = false;
                EventResult::Consumed
            }
            KeyCode::Right | KeyCode::Char('l') => {
                tts_config.write().auto_play = true;
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(SettingItem(
        is_editing: props.is_editing,
    ) {
        View(flex_direction:Direction::Horizontal,justify_content:Flex::SpaceBetween) {
            widget(Line::from("自动播放:").style(theme.text))
            widget(Line::from(format!("{}", tts_config.read().auto_play)).style(theme.text))
        }
    })
}
