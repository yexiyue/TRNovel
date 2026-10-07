//! Listening settings compose ordinary reader widgets and protocol snapshots.
use super::TtsContext;
use crate::{
    components::{AdjustableSettingItem, SettingItem},
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Margin},
    style::Style,
    text::{Line, Text},
    widgets::Block,
};
use ratatui_kit::prelude::*;
use tts_protocol::ConfigPatch;

#[cfg(test)]
mod tests;

#[derive(Props, Default)]
pub struct TTSManagerProps {
    pub open: bool,
    pub is_editing: bool,
}

#[component]
pub fn TTSManager(props: &TTSManagerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let context = hooks.use_context::<TtsContext>().clone();
    let snapshot = context.snapshot.read().clone();
    let alignment_enabled = snapshot
        .config
        .as_ref()
        .is_some_and(|config| config.alignment_enabled);
    let style_enabled = snapshot
        .capabilities
        .as_ref()
        .is_some_and(|caps| caps.style);
    let style_index = if alignment_enabled { 10 } else { 9 };
    let restart_index = style_index + usize::from(style_enabled);
    let release_index = restart_index + 1;
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let open = props.open;
    let editing = props.is_editing;
    let layer = hooks.use_input_layer(open, false);
    hooks.use_effect(
        {
            let handle = context.handle.clone();
            move || {
                if open {
                    handle.open();
                }
            }
        },
        open,
    );
    let mut index = hooks.use_state(|| 0usize);
    hooks.use_effect(
        move || index.set(index.get().min(release_index)),
        release_index,
    );
    let scroll = hooks.use_state(ScrollViewState::default);
    let (_, height) = hooks.use_terminal_size();
    hooks.use_effect(
        move || scroll.write().scroll_to_index(index.get()),
        (index.get(), height),
    );
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let handle = context.handle.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if !open || !editing || key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    index.set((index.get() + 1).min(release_index))
                }
                KeyCode::Up | KeyCode::Char('k') => index.set(index.get().saturating_sub(1)),
                KeyCode::Enter => match index.get() {
                    0 => handle.prepare(),
                    row if row == restart_index => handle.restart(),
                    row if row == release_index => handle.release(),
                    _ => return EventResult::Ignored,
                },
                KeyCode::Esc if index.get() == 0 => handle.cancel_prepare(),
                _ => return EventResult::Ignored,
            }
            EventResult::Consumed
        }
    });
    let status = snapshot
        .error
        .clone()
        .unwrap_or_else(|| format!("{} · {}", snapshot.progress, snapshot.alignment));
    element!(Modal(layer:Some(layer),width:Constraint::Percentage(80),height:Constraint::Percentage(80),open:open,blocks_lower:false,margin:Margin::new(1,1),style:Style::default().dim()) {
        View(margin:Margin::new(1,1)) {
            ScrollView(active:false,state:scroll,block:Block::bordered().border_style(theme.border.not_dim()).title_top(Line::from("听书设置").centered().style(theme.title))) {
                View(height:Constraint::Length(4)) {
                    SettingItem(is_editing:editing && index.get()==0,top_title:"Enter 启用模型 / Esc 取消准备".to_string()) {
                        widget(Text::from(status))
                    }
                }
                ListeningSetting(kind:Setting::Backend,is_editing:editing && index.get()==1)
                ListeningSetting(kind:Setting::Model,is_editing:editing && index.get()==2)
                ListeningSetting(kind:Setting::Voice,is_editing:editing && index.get()==3)
                ListeningSetting(kind:Setting::Speed,is_editing:editing && index.get()==4)
                ListeningSetting(kind:Setting::Volume,is_editing:editing && index.get()==5)
                ListeningSetting(kind:Setting::AutoPlay,is_editing:editing && index.get()==6)
                ListeningSetting(kind:Setting::TtsDevice,is_editing:editing && index.get()==7)
                ListeningSetting(kind:Setting::AlignmentEnabled,is_editing:editing && index.get()==8)
                if alignment_enabled { ListeningSetting(kind:Setting::AlignmentDevice,is_editing:editing && index.get()==9) }
                if style_enabled { ListeningSetting(kind:Setting::Style,is_editing:editing && index.get()==style_index) }
                View(height:Constraint::Length(3)) {
                    SettingItem(is_editing:editing && index.get()==restart_index) {widget(Line::from("Enter 从本章开头重新播放（忽略恢复点）"))}
                }
                View(height:Constraint::Length(3)) {
                    SettingItem(is_editing:editing && index.get()==release_index) {widget(Line::from("Enter 停播并释放模型、音频和子进程"))}
                }
            }
        }
    })
}

#[derive(Clone, Copy, Default)]
enum Setting {
    #[default]
    Voice,
    Backend,
    Model,
    Style,
    Speed,
    Volume,
    AutoPlay,
    TtsDevice,
    AlignmentDevice,
    AlignmentEnabled,
}
#[derive(Props, Default)]
struct ListeningSettingProps {
    kind: Setting,
    is_editing: bool,
}
#[component]
fn ListeningSetting(props: &ListeningSettingProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let context = hooks.use_context::<TtsContext>().clone();
    let snapshot = context.snapshot.read();
    let kind = props.kind;
    let label = match kind {
        Setting::Voice => "音色",
        Setting::Backend => "语音后端",
        Setting::Model => "模型",
        Setting::Style => "朗读风格",
        Setting::Speed => "播放速度",
        Setting::Volume => "音量",
        Setting::AutoPlay => "自动续章",
        Setting::TtsDevice => "合成设备",
        Setting::AlignmentDevice => "对齐设备",
        Setting::AlignmentEnabled => "逐句高亮",
    };
    let value = snapshot.config.as_ref().map_or_else(
        || "未连接".to_string(),
        |config| match kind {
            Setting::Backend => config.backend.clone(),
            Setting::Style => config.style.clone().unwrap_or_else(|| "自然".into()),
            Setting::Model => snapshot
                .capabilities
                .as_ref()
                .map(|caps| caps.model_name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| config.model.clone().unwrap_or_else(|| "默认模型".into())),
            Setting::Voice => snapshot
                .capabilities
                .as_ref()
                .and_then(|caps| caps.voice_names.get(&config.voice))
                .cloned()
                .unwrap_or_else(|| config.voice.clone()),
            Setting::Speed => format!("{:.1}x", config.speed),
            Setting::Volume => format!("{:.1}x", config.volume),
            Setting::AutoPlay => config.auto_play.to_string(),
            Setting::AlignmentEnabled => if config.alignment_enabled {
                "开启"
            } else {
                "关闭（片段高亮）"
            }
            .into(),
            Setting::TtsDevice | Setting::AlignmentDevice => {
                let (component, requested) = if matches!(kind, Setting::TtsDevice) {
                    ("tts", config.tts_device)
                } else {
                    ("alignment", config.alignment_device)
                };
                snapshot.devices.get(component).map_or_else(
                    || format!("{requested:?}"),
                    |(_, status)| format!("{requested:?} → {status}"),
                )
            }
        },
    );
    drop(snapshot);
    let decrease = context.clone();
    element!(View(height:Constraint::Length(3)) {
        AdjustableSettingItem(is_editing:props.is_editing,label:label.to_string(),value:value,
            on_decrease:move |_| change(&decrease,kind,false),
            on_increase:move |_| change(&context,kind,true),
        )
    })
}
fn change(context: &TtsContext, kind: Setting, increase: bool) {
    let snapshot = context.snapshot.read();
    let Some(config) = &snapshot.config else {
        return;
    };
    let mut patch = ConfigPatch::default();
    let delta = if increase { 0.1 } else { -0.1 };
    match kind {
        Setting::Style => {
            let choices = [
                "",
                "温暖沉稳，适合小说旁白。",
                "轻松自然，清晰流畅。",
                "富有感情，突出人物对话。",
            ];
            let current = choices
                .iter()
                .position(|v| Some(*v) == config.style.as_deref())
                .unwrap_or(0);
            let next = if increase {
                (current + 1).min(choices.len() - 1)
            } else {
                current.saturating_sub(1)
            };
            patch.style = Some(choices[next].into());
        }
        Setting::Backend | Setting::Model => {
            let mut seen = std::collections::BTreeSet::new();
            let choices: Vec<_> = snapshot
                .backends
                .iter()
                .filter(|caps| {
                    if matches!(kind, Setting::Model) {
                        caps.backend == config.backend
                    } else {
                        seen.insert(&caps.backend)
                    }
                })
                .collect();
            if choices.is_empty() {
                return;
            }
            let current = choices
                .iter()
                .position(|caps| {
                    if matches!(kind, Setting::Backend) {
                        caps.backend == config.backend
                    } else {
                        caps.matches(&config.backend, config.model.as_deref())
                    }
                })
                .unwrap_or(0);
            let next = if increase {
                (current + 1).min(choices.len() - 1)
            } else {
                current.saturating_sub(1)
            };
            if next == current {
                return;
            }
            let caps = choices[next];
            patch.backend = Some(caps.backend.clone());
            patch.model = caps.model.clone();
            patch.voice = Some(caps.default_voice.clone());
            if caps.backend != config.backend
                || (config.tts_device != tts_protocol::Device::Auto
                    && !caps.compiled_devices.contains(&config.tts_device))
            {
                patch.tts_device = Some(tts_protocol::Device::Auto);
            }
        }
        Setting::Voice => {
            let Some(capabilities) = &snapshot.capabilities else {
                return;
            };
            let voices = &capabilities.voices;
            if voices.is_empty() {
                return;
            }
            let current = voices
                .iter()
                .position(|voice| voice == &config.voice)
                .unwrap_or(0);
            let next = if increase {
                (current + 1).min(voices.len() - 1)
            } else {
                current.saturating_sub(1)
            };
            patch.voice = Some(voices[next].clone());
        }
        Setting::Speed => {
            patch.speed = Some(((config.speed + delta) * 10.0).round().clamp(5.0, 20.0) / 10.0)
        }
        Setting::Volume => {
            patch.volume = Some(((config.volume + delta) * 10.0).round().clamp(0.0, 100.0) / 10.0)
        }
        Setting::AutoPlay => patch.auto_play = Some(increase),
        Setting::AlignmentEnabled => patch.alignment_enabled = Some(increase),
        Setting::TtsDevice | Setting::AlignmentDevice => {
            let (component, current) = if matches!(kind, Setting::TtsDevice) {
                ("tts", config.tts_device)
            } else {
                ("alignment", config.alignment_device)
            };
            let mut choices = vec![tts_protocol::Device::Auto];
            if let Some((compiled, _)) = snapshot.devices.get(component) {
                choices.extend(compiled.iter().copied());
            } else {
                choices.push(tts_protocol::Device::Cpu);
            }
            let current = choices.iter().position(|d| *d == current).unwrap_or(0);
            let next = if increase {
                (current + 1).min(choices.len() - 1)
            } else {
                current.saturating_sub(1)
            };
            if matches!(kind, Setting::TtsDevice) {
                patch.tts_device = Some(choices[next]);
            } else {
                patch.alignment_device = Some(choices[next]);
            }
        }
    }
    context.handle.update(patch);
}
