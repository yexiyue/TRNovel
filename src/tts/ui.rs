//! Playback controls and staged single-voice preferences.
use super::{Snapshot, TtsContext};
use crate::{
    components::{AdjustableSettingItem, SettingItem, setting_item::focus_row},
    config::SoundConfig,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::{
    layout::{Constraint, Direction},
    text::Line,
};
use ratatui_kit::prelude::*;
use tts_protocol::{ConfigPatch, Device, SessionState};
mod picker;
#[cfg(test)]
mod tests;

#[derive(Props, Default)]
pub struct TTSManagerProps {
    pub open: bool,
    pub layer: Option<InputLayer>,
    pub on_close: Handler<'static, ()>,
    pub is_editing: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Panel {
    #[default]
    Playback,
    Sound,
    Advanced,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerKind {
    Model,
    Voice,
}
#[derive(Clone, Copy)]
enum Setting {
    Speed,
    Volume,
    AutoPlay,
    Playback,
    Style,
    Device,
}
#[derive(Clone, Copy)]
enum Action {
    Playback,
    Sound,
    Advanced,
    Back,
    Pick(PickerKind),
    Apply,
    ApplyFromStart,
    Prepare,
    Release,
    CancelPrepare,
}
struct Row {
    label: &'static str,
    value: String,
    control: Control,
}
#[derive(Clone, Copy)]
enum Control {
    Setting(Setting),
    Action(Action),
}
impl Row {
    fn action(label: &'static str, value: impl Into<String>, action: Action) -> Self {
        Self {
            label,
            value: value.into(),
            control: Control::Action(action),
        }
    }
    fn setting(label: &'static str, value: String, setting: Setting) -> Self {
        Self {
            label,
            value,
            control: Control::Setting(setting),
        }
    }
}
fn sound_label(snapshot: &Snapshot, sound: Option<&SoundConfig>) -> String {
    let Some(sound) = sound else {
        return "尚未连接".into();
    };
    let caps = snapshot
        .backends
        .iter()
        .find(|caps| caps.matches(&sound.backend, sound.model.as_deref()));
    let model = caps
        .map(|caps| caps.model_name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(sound.model.as_deref().unwrap_or(&sound.backend));
    let voice = caps
        .and_then(|caps| caps.voice_names.get(&sound.voice))
        .unwrap_or(&sound.voice);
    format!("{model} · {voice}")
}
fn rows(snapshot: &Snapshot, panel: Panel) -> Vec<Row> {
    let config = snapshot.config.as_ref();
    let sound = snapshot.sound.as_ref();
    match panel {
        Panel::Playback => vec![
            Row::action(
                "播放控制",
                match snapshot.state {
                    SessionState::Paused => "继续播放",
                    SessionState::Preparing => "取消准备",
                    SessionState::Generating | SessionState::Buffering | SessionState::Playing => {
                        "暂停播放"
                    }
                    _ => "使用阅读页 P 开始朗读",
                },
                if snapshot.state == SessionState::Preparing {
                    Action::CancelPrepare
                } else {
                    Action::Playback
                },
            ),
            Row::setting(
                "播放速度",
                format!("{:.1}x", config.map_or(1.0, |c| c.speed)),
                Setting::Speed,
            ),
            Row::setting(
                "音量",
                format!("{:.1}x", config.map_or(1.0, |c| c.volume)),
                Setting::Volume,
            ),
            Row::setting(
                "自动续章",
                if config.is_some_and(|c| c.auto_play) {
                    "开启"
                } else {
                    "关闭"
                }
                .into(),
                Setting::AutoPlay,
            ),
            Row::setting(
                "播放策略",
                match snapshot.playback {
                    crate::config::PlaybackPreference::Streaming => "流式播放",
                    crate::config::PlaybackPreference::AfterChapterReady => "整章生成后播放",
                }
                .into(),
                Setting::Playback,
            ),
            Row::action("声音设置", sound_label(snapshot, sound), Action::Sound),
        ],
        Panel::Sound => {
            let mut result = vec![
                Row::action(
                    "模型",
                    snapshot
                        .sound_capabilities()
                        .map_or("尚未连接".into(), |caps| {
                            format!("{} · {}", caps.backend, caps.model_name)
                        }),
                    Action::Pick(PickerKind::Model),
                ),
                Row::action(
                    "音色",
                    sound
                        .and_then(|s| {
                            snapshot
                                .sound_capabilities()
                                .and_then(|caps| caps.voice_names.get(&s.voice))
                        })
                        .cloned()
                        .or_else(|| sound.map(|s| s.voice.clone()))
                        .unwrap_or_else(|| "尚未连接".into()),
                    Action::Pick(PickerKind::Voice),
                ),
            ];
            if snapshot.sound_capabilities().is_some_and(|caps| caps.style) {
                result.push(Row::setting(
                    "朗读风格",
                    sound
                        .and_then(|s| s.style.clone())
                        .unwrap_or_else(|| "自然".into()),
                    Setting::Style,
                ));
            }
            result.extend([
                Row::action(
                    "应用并重播",
                    "从最后确认播放的位置应用新声音",
                    Action::Apply,
                ),
                Row::action(
                    "从头重听",
                    "应用当前声音，从本章开头重新播放（对照试音用）",
                    Action::ApplyFromStart,
                ),
                Row::action("高级设置", "设备 · 模型准备 · 释放资源", Action::Advanced),
                Row::action("返回听书面板", "已保存的声音用于下一次播放", Action::Back),
            ]);
            result
        }
        Panel::Advanced => vec![
            Row::setting(
                "合成设备",
                format!("{:?}", sound.map_or(Device::Auto, |s| s.device)),
                Setting::Device,
            ),
            Row::action("模型准备", "停止状态下准备下次使用的模型", Action::Prepare),
            Row::action("取消准备", "停止下载或加载", Action::CancelPrepare),
            Row::action("释放资源", "停播并关闭 Talechime 子进程", Action::Release),
            Row::action("返回声音设置", "", Action::Back),
        ],
    }
}
fn activate(
    context: &TtsContext,
    action: Action,
    mut panel: State<Panel>,
    mut picker: State<Option<PickerKind>>,
    mut index: State<usize>,
) {
    match action {
        Action::Sound => {
            panel.set(Panel::Sound);
            index.set(0);
        }
        Action::Advanced => {
            panel.set(Panel::Advanced);
            index.set(0);
        }
        Action::Back => {
            panel.set(if panel.get() == Panel::Advanced {
                Panel::Sound
            } else {
                Panel::Playback
            });
            index.set(0);
        }
        Action::Pick(kind) => picker.set(Some(kind)),
        Action::Apply => context.handle.restart(),
        Action::ApplyFromStart => context.handle.restart_from_start(),
        Action::Prepare => context.handle.prepare(),
        Action::CancelPrepare => context.handle.cancel_prepare(),
        Action::Release => context.handle.release(),
        Action::Playback => context.handle.toggle_current(),
    }
}
#[component]
pub fn TTSManager(props: &mut TTSManagerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let context = hooks.use_context::<TtsContext>().clone();
    let snapshot = context.snapshot.read().clone();
    let mut panel = hooks.use_state(Panel::default);
    let mut picker = hooks.use_state(|| None::<PickerKind>);
    let mut index = hooks.use_state(|| 0usize);
    let scroll = hooks.use_state(ScrollViewState::default);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let open = props.open;
    let editing = props.is_editing && picker.get().is_none();
    let local_layer = hooks.use_input_layer(open && props.layer.is_none(), true);
    let layer = props.layer.unwrap_or(local_layer);
    hooks.use_effect(
        {
            let handle = context.handle.clone();
            move || {
                if open {
                    handle.open();
                } else {
                    panel.set(Panel::Playback);
                    picker.set(None);
                    index.set(0);
                }
            }
        },
        open,
    );
    let items = rows(&snapshot, panel.get());
    let last = items.len().saturating_sub(1);
    hooks.use_effect(move || index.set(index.get().min(last)), last);
    let (_, height) = hooks.use_terminal_size();
    hooks.use_effect(
        move || scroll.write().scroll_to_index(index.get()),
        (index.get(), panel.get(), height),
    );
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let context = context.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if !open || !editing || key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => index.set((index.get() + 1).min(last)),
                KeyCode::Up | KeyCode::Char('k') => index.set(index.get().saturating_sub(1)),
                KeyCode::Enter => {
                    let action = rows(&context.snapshot.read(), panel.get())
                        .get(index.get())
                        .map(|row| row.control);
                    if let Some(Control::Action(action)) = action {
                        activate(&context, action, panel, picker, index);
                    }
                }
                KeyCode::Esc if panel.get() != Panel::Playback => {
                    activate(&context, Action::Back, panel, picker, index)
                }
                _ => return EventResult::Ignored,
            }
            EventResult::Consumed
        }
    });
    hooks.use_event_handler_with_options(
        EventScope::Layer(layer),
        EventPriority::Low,
        EventOptions { hit_test: false },
        move |event| {
            if !open || !editing {
                return EventResult::Ignored;
            }
            match event {
                Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
                    index.set((index.get() + 1).min(last))
                }
                Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollUp => {
                    index.set(index.get().saturating_sub(1))
                }
                _ => return EventResult::Ignored,
            }
            EventResult::Consumed
        },
    );
    let title = match panel.get() {
        Panel::Playback => "听书",
        Panel::Sound => "声音设置",
        Panel::Advanced => "声音设置 / 高级",
    };
    let status = snapshot
        .error
        .clone()
        .unwrap_or_else(|| format!("{} · {}", state_label(snapshot.state), snapshot.progress));
    let status_height = status.lines().count().clamp(1, 3) as u16;
    let current = if snapshot.playback_sound.is_none() {
        "未开始朗读".into()
    } else {
        sound_label(&snapshot, snapshot.playback_sound.as_ref())
    };
    element!(Modal(open:open, layer:Some(layer), width:Constraint::Percentage(85), height:Constraint::Percentage(85)) {
        Border(top_title:Line::from(title), bottom_title:Line::from("↑/↓ 选择 · ←/→ 调整 · Enter 打开/执行 · Esc 返回"), border_style:theme.border) {
            View(flex_direction:Direction::Vertical) {
                View(height:Constraint::Length(status_height), flex_direction:Direction::Horizontal) {
                    View(width:Constraint::Fill(1)) { Text(text:status, style:if snapshot.error.is_some() { theme.highlight } else { theme.text }) }
                    crate::components::ActionButton(label:"关闭", on_press:props.on_close.take())
                }
                View(height:Constraint::Length(1)) { Text(text:format!("当前声音：{current}")) }
                View(height:Constraint::Length(1)) { Text(text:if snapshot.has_pending_sound() { "已保存新声音，下次播放生效；应用并重播可立即切换" } else { "声音设置保存到下次播放；速度、音量、续章立即生效" }, style:theme.empty) }
                if !snapshot.buffer.is_empty() { View(height:Constraint::Length(1)) { Text(text:snapshot.buffer.clone()) } }
                if snapshot.sound_capabilities().is_some_and(|caps| caps.backend == "voxcpm" && caps.model.as_deref() == Some("2b-bf16")) {
                    View(height:Constraint::Length(1)) { Text(text:"实验模型 · 数值与长期稳定性验收待完成", style:theme.highlight) }
                }
                ScrollView(state:scroll, scrollbars:Scrollbars { vertical_scrollbar:crate::theme::scrollbar_style(theme.border,theme.title), ..Default::default() }, active:false) {
                    for (row_index, row) in items.into_iter().enumerate() {
                        View(key:row_index, height:Constraint::Length(3)) {
                            if let Control::Setting(kind) = row.control {
                                AdjustableSettingItem(label:row.label, value:row.value, is_editing:editing && index.get()==row_index,
                                    on_click:focus_row(index,row_index),
                                    on_decrease:{let context=context.clone(); move |_| change(&context,kind,false)},
                                    on_increase:{let context=context.clone(); move |_| change(&context,kind,true)})
                            } else {
                                SettingItem(top_title:row.label, is_editing:editing && index.get()==row_index, on_click:focus_row(index,row_index),
                                    on_activate:{let context=context.clone(); move |_| {if let Control::Action(action)=row.control {activate(&context,action,panel,picker,index);}}}) { Text(text:row.value) }
                            }
                        }
                    }
                }
            }
        }
        if let Some(kind) = picker.get() { picker::SoundPicker(kind:Some(kind), on_close:move |_| picker.set(None)) }
    })
}
fn state_label(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle => "待播放",
        SessionState::Preparing => "准备中",
        SessionState::Generating => "生成中",
        SessionState::Buffering => "缓冲中",
        SessionState::Playing => "播放中",
        SessionState::Paused => "已暂停",
        SessionState::Stopped => "已停止",
        SessionState::Failed => "失败",
    }
}
fn change(context: &TtsContext, kind: Setting, increase: bool) {
    let snapshot = context.snapshot.read();
    let Some(config) = &snapshot.config else {
        return;
    };
    let mut patch = ConfigPatch::default();
    let delta = if increase { 0.1 } else { -0.1 };
    match kind {
        Setting::Speed => {
            patch.speed = Some(((config.speed + delta) * 10.0).round().clamp(5.0, 20.0) / 10.0)
        }
        Setting::Volume => {
            patch.volume = Some(((config.volume + delta) * 10.0).round().clamp(0.0, 100.0) / 10.0)
        }
        Setting::AutoPlay => patch.auto_play = Some(increase),
        Setting::Playback => {
            // Reader-owned plan preference; it applies when a session starts.
            let next = if increase {
                crate::config::PlaybackPreference::AfterChapterReady
            } else {
                crate::config::PlaybackPreference::Streaming
            };
            drop(snapshot);
            context.handle.set_playback(next);
            return;
        }
        Setting::Style => {
            let choices = [
                "",
                "温暖沉稳，适合小说旁白。",
                "轻松自然，清晰流畅。",
                "富有感情，突出人物对话。",
            ];
            let value = snapshot
                .sound
                .as_ref()
                .and_then(|sound| sound.style.as_deref())
                .unwrap_or("");
            let current = choices
                .iter()
                .position(|choice| *choice == value)
                .unwrap_or(0);
            let next = if increase {
                (current + 1).min(choices.len() - 1)
            } else {
                current.saturating_sub(1)
            };
            patch.style = Some(choices[next].into());
        }
        Setting::Device => {
            let mut choices = vec![Device::Auto];
            if let Some(caps) = snapshot.sound_capabilities() {
                choices.extend(
                    caps.compiled_devices
                        .iter()
                        .copied()
                        .filter(|d| *d != Device::Auto),
                );
            }
            let value = snapshot
                .sound
                .as_ref()
                .map_or(Device::Auto, |sound| sound.device);
            let current = choices.iter().position(|d| *d == value).unwrap_or(0);
            let next = if increase {
                (current + 1).min(choices.len() - 1)
            } else {
                current.saturating_sub(1)
            };
            patch.tts_device = Some(choices[next]);
        }
    }
    drop(snapshot);
    context.handle.update(patch);
}
