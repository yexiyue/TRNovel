//! A modal-local live filter; typing never reaches the reader's shortcuts.
use super::{PickerKind, Snapshot, TtsContext};
use crate::{components::settings, theme::AppChromeTheme};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction},
    text::Line,
};
use ratatui_kit::prelude::*;
use std::sync::Arc;
use tts_protocol::ConfigPatch;
use tui_input::backend::crossterm::EventHandler;

#[derive(Clone)]
pub(super) struct Choice {
    pub(super) label: String,
    patch: ConfigPatch,
    selected: bool,
}
pub(super) fn choices(snapshot: &Snapshot, kind: PickerKind, query: &str) -> Vec<Choice> {
    let mut choices = Vec::new();
    let Some(sound) = &snapshot.sound else {
        return choices;
    };
    match kind {
        PickerKind::Model => {
            for caps in &snapshot.backends {
                let current = snapshot
                    .sound_capabilities()
                    .is_some_and(|selected| selected == caps);
                choices.push(Choice {
                    label: format!(
                        "{} / {} [{}]",
                        caps.backend,
                        caps.model_name,
                        caps.model.as_deref().unwrap_or("default")
                    ),
                    patch: ConfigPatch {
                        backend: Some(caps.backend.clone()),
                        model: caps.model.clone(),
                        voice: Some(caps.default_voice.clone()),
                        style: Some(String::new()),
                        tts_device: Some(if caps.compiled_devices.contains(&sound.device) {
                            sound.device
                        } else {
                            tts_protocol::Device::Auto
                        }),
                        ..Default::default()
                    },
                    selected: current,
                });
            }
            choices.sort_by(|a, b| a.label.cmp(&b.label));
        }
        PickerKind::Voice => {
            if let Some(caps) = snapshot.sound_capabilities() {
                for voice in &caps.voices {
                    let name = caps.voice_names.get(voice).unwrap_or(voice);
                    choices.push(Choice {
                        label: format!("{name} [{voice}]"),
                        patch: ConfigPatch {
                            voice: Some(voice.clone()),
                            ..Default::default()
                        },
                        selected: *voice == sound.voice,
                    });
                }
            }
        }
    }
    let query = query.trim().to_lowercase();
    choices.retain(|choice| choice.label.to_lowercase().contains(&query));
    choices
}

#[derive(Default, Props)]
pub(super) struct SoundPickerProps {
    pub kind: Option<PickerKind>,
    pub on_close: Handler<'static, ()>,
}
#[component]
pub(super) fn SoundPicker(
    props: &mut SoundPickerProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let context = hooks.use_context::<TtsContext>().clone();
    let kind = props.kind.unwrap_or(PickerKind::Model);
    let input = hooks.use_state(tui_input::Input::default);
    let layer = hooks.use_input_layer(true, true);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let callbacks = settings::use_callback(&mut hooks, &mut props.on_close);
    let query = input.read().value().to_owned();
    let items = choices(&context.snapshot.read(), kind, &query);
    let initial = items.iter().position(|choice| choice.selected).unwrap_or(0);
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let callbacks = callbacks.clone();
        move |event| {
            match &event {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Esc => {
                        callbacks.lock().unwrap_or_else(|e| e.into_inner())(());
                    }
                    KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Enter => return EventResult::Ignored,
                    _ => {
                        input.write().handle_event(&event);
                    }
                },
                Event::Paste(text) => {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        input
                            .write()
                            .handle(tui_input::InputRequest::InsertChar(ch));
                    }
                }
                _ => return EventResult::Ignored,
            }
            EventResult::Consumed
        }
    });
    let title = if kind == PickerKind::Model {
        "选择模型 · 按后端分组"
    } else {
        "选择音色"
    };
    element!(Modal(open:true, layer:Some(layer), width:Constraint::Percentage(80), height:Constraint::Percentage(75)) {
        Border(top_title:Line::from(title), border_style:theme.border, bottom_title:Line::from("输入即过滤 · ↑/↓ 选择 · Enter 保存 · Esc 取消")) {
            View(flex_direction:Direction::Vertical) {
                View(height:Constraint::Length(3)) {
                    Border(top_title:Line::from("搜索名称或 ID"), border_style:theme.border) {
                        Input(input:input.read().clone(), placeholder:"输入关键词…", on_seek:move |cursor| { input.write().handle(tui_input::InputRequest::SetCursor(cursor)); })
                    }
                }
                settings::SettingsChoiceList(..settings::ChoiceListProps {
                    colors:None,
                    title:"可选项".into(),options:Arc::new(items.iter().map(|c|format!("{}{}",if c.selected{"✓ "}else{"  "},c.label)).collect()),initial,
                    on_select:settings::callback({let callbacks=callbacks.clone();move|index: usize|{if let Some(choice)=items.get(index){if !choice.selected{context.handle.update(choice.patch.clone());}callbacks.lock().unwrap_or_else(|e|e.into_inner())(());}}}),
                    on_cancel:callbacks.clone(),dismiss_outside:false,
                })
            }
        }
    })
}
