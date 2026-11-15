//! Playback controls and staged single-voice preferences.
use super::{Snapshot, TtsContext};
use crate::{
    components::settings::{self, Control as UiControl, Edit, view::Focus},
    config::SoundConfig,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Margin},
    text::Line,
};
use ratatui_kit::prelude::*;
use std::sync::Arc;
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
#[derive(Clone, Copy, PartialEq, Eq)]
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
        ],
    }
}
const SPEED_MIN: f32 = 0.5;
const SPEED_MAX: f32 = 2.0;
const VOLUME_MAX: f32 = 10.0;
const STYLES: [&str; 4] = [
    "",
    "温暖沉稳，适合小说旁白。",
    "轻松自然，清晰流畅。",
    "富有感情，突出人物对话。",
];
fn options(snapshot: &Snapshot, setting: Setting) -> (Vec<String>, usize) {
    match setting {
        Setting::Playback => (
            vec!["流式播放".into(), "整章生成后播放".into()],
            usize::from(snapshot.playback == crate::config::PlaybackPreference::AfterChapterReady),
        ),
        Setting::Style => {
            let values = style_values(snapshot);
            let value = snapshot
                .sound
                .as_ref()
                .and_then(|s| s.style.as_deref())
                .unwrap_or("");
            let index = values.iter().position(|s| s == value).unwrap_or(0);
            (
                values
                    .iter()
                    .map(|s| {
                        if s.is_empty() {
                            "自然".into()
                        } else {
                            s.chars().take(16).collect()
                        }
                    })
                    .collect(),
                index,
            )
        }
        Setting::Device => {
            let values = devices(snapshot);
            let value = snapshot.sound.as_ref().map_or(Device::Auto, |s| s.device);
            let index = values.iter().position(|v| *v == value).unwrap_or(0);
            (values.iter().map(|d| format!("{d:?}")).collect(), index)
        }
        _ => (Vec::new(), 0),
    }
}
fn style_values(snapshot: &Snapshot) -> Vec<String> {
    let mut values = STYLES.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    if let Some(value) = snapshot.sound.as_ref().and_then(|s| s.style.as_ref())
        && !values.contains(value)
    {
        values.push(value.clone());
    }
    values
}
fn devices(snapshot: &Snapshot) -> Vec<Device> {
    let mut result = vec![Device::Auto];
    if let Some(caps) = snapshot.sound_capabilities() {
        for device in &caps.compiled_devices {
            if !result.contains(device) {
                result.push(*device);
            }
        }
    }
    result
}
fn choose(context: &TtsContext, setting: Setting, index: usize) {
    let snapshot = context.snapshot.read();
    if snapshot.config.is_none() {
        return;
    }
    let mut patch = ConfigPatch::default();
    match setting {
        Setting::Playback => {
            drop(snapshot);
            context.handle.set_playback(if index == 0 {
                crate::config::PlaybackPreference::Streaming
            } else {
                crate::config::PlaybackPreference::AfterChapterReady
            });
            return;
        }
        Setting::Style => {
            let Some(value) = style_values(&snapshot).get(index).cloned() else {
                return;
            };
            patch.style = Some(value);
        }
        Setting::Device => {
            let Some(value) = devices(&snapshot).get(index).copied() else {
                return;
            };
            patch.tts_device = Some(value);
        }
        _ => return,
    }
    drop(snapshot);
    context.handle.update(patch);
}
fn change(context: &TtsContext, setting: Setting, increase: bool) {
    let snapshot = context.snapshot.read();
    let Some(config) = &snapshot.config else {
        return;
    };
    let mut patch = ConfigPatch::default();
    let delta = if increase { 0.1 } else { -0.1 };
    match setting {
        Setting::Speed => {
            patch.speed = Some(
                ((config.speed + delta) * 10.0)
                    .round()
                    .clamp(SPEED_MIN * 10.0, SPEED_MAX * 10.0)
                    / 10.0,
            )
        }
        Setting::Volume => {
            patch.volume = Some(
                ((config.volume + delta) * 10.0)
                    .round()
                    .clamp(0.0, VOLUME_MAX * 10.0)
                    / 10.0,
            )
        }
        Setting::AutoPlay => patch.auto_play = Some(increase),
        _ => {
            let (values, index) = options(&snapshot, setting);
            let next = if increase {
                (index + 1).min(values.len().saturating_sub(1))
            } else {
                index.saturating_sub(1)
            };
            drop(snapshot);
            choose(context, setting, next);
            return;
        }
    }
    drop(snapshot);
    context.handle.update(patch);
}
fn activate(context: &TtsContext, action: Action, mut picker: State<Option<PickerKind>>) {
    match action {
        Action::Pick(kind) => picker.set(Some(kind)),
        Action::Apply => context.handle.restart(),
        Action::ApplyFromStart => context.handle.restart_from_start(),
        Action::Prepare => context.handle.prepare(),
        Action::CancelPrepare => context.handle.cancel_prepare(),
        Action::Release => context.handle.release(),
        Action::Playback => context.handle.toggle_current(),
    }
}
fn form_rows(snapshot: &Snapshot, panel: Panel) -> Vec<settings::Row<Control>> {
    rows(snapshot, panel)
        .into_iter()
        .map(|row| {
            let control = match row.control {
                Control::Setting(_) if snapshot.config.is_none() => UiControl::ReadOnly,
                Control::Setting(Setting::Speed) => {
                    let speed = snapshot.config.as_ref().map_or(1.0, |c| c.speed);
                    UiControl::Number {
                        decrease: speed > SPEED_MIN,
                        increase: speed < SPEED_MAX,
                    }
                }
                Control::Setting(Setting::Volume) => {
                    let volume = snapshot.config.as_ref().map_or(1.0, |c| c.volume);
                    UiControl::Number {
                        decrease: volume > 0.0,
                        increase: volume < VOLUME_MAX,
                    }
                }
                Control::Setting(Setting::AutoPlay) => {
                    UiControl::Toggle(snapshot.config.as_ref().is_some_and(|c| c.auto_play))
                }
                Control::Setting(_) => UiControl::Choice,
                Control::Action(Action::Pick(_)) => UiControl::Choice,
                Control::Action(_) => UiControl::Action,
            };
            let group = match row.control {
                Control::Action(Action::Apply | Action::ApplyFromStart) => "应用",
                _ => match panel {
                    Panel::Playback => "播放",
                    Panel::Sound => "声音",
                    Panel::Advanced => "高级",
                },
            };
            let value = match row.control {
                Control::Action(Action::Apply) => "应用并重播".into(),
                Control::Action(Action::ApplyFromStart) => "从头重听".into(),
                Control::Action(Action::Prepare) => "准备模型".into(),
                Control::Action(Action::CancelPrepare) => "取消准备".into(),
                Control::Action(Action::Release) => "释放资源".into(),
                _ => row.value.clone(),
            };
            settings::Row {
                group,
                key: row.control,
                label: row.label.into(),
                value,
                detail: row.value,
                control,
            }
        })
        .collect()
}
fn panel_at(index: usize) -> Panel {
    match index {
        1 => Panel::Sound,
        2 => Panel::Advanced,
        _ => Panel::Playback,
    }
}
#[component]
pub fn TTSManager(props: &mut TTSManagerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let context = hooks.use_context::<TtsContext>().clone();
    let snapshot = context.snapshot.read().clone();
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let (width, height) = hooks.use_terminal_size();
    let mut panel = hooks.use_state(Panel::default);
    let mut picker = hooks.use_state(|| None::<PickerKind>);
    let mut choice = hooks.use_state(|| None::<Setting>);
    let mut index = hooks.use_state(|| 0usize);
    let mut focus = hooks.use_state(|| Focus::Tabs);
    let mut revision = hooks.use_state(|| 0u64);
    let close = settings::use_callback(&mut hooks, &mut props.on_close);
    let open = props.open;
    let active = open && props.is_editing && picker.get().is_none() && choice.get().is_none();
    let own_layer = hooks.use_input_layer(open && props.layer.is_none(), true);
    let layer = props.layer.unwrap_or(own_layer);
    hooks.use_effect(
        {
            let context = context.clone();
            move || {
                if open {
                    context.handle.open();
                    focus.set(Focus::Tabs);
                    revision.set(revision.get().wrapping_add(1));
                } else {
                    panel.set(Panel::Playback);
                    picker.set(None);
                    choice.set(None);
                    index.set(0);
                }
            }
        },
        open,
    );
    let items = Arc::new(form_rows(&snapshot, panel.get()));
    let last = items.len().saturating_sub(1);
    hooks.use_effect(move || index.set(index.get().min(last)), last);
    let editor = settings::callback({
        let context = context.clone();
        move |edit| {
            let (i, delta) = match edit {
                Edit::Focus(i) => {
                    index.set(i);
                    focus.set(Focus::Form);
                    return;
                }
                Edit::Activate(i) => (i, None),
                Edit::Adjust(i, d) => (i, Some(d)),
            };
            let items = form_rows(&context.snapshot.read(), panel.get());
            let Some(row) = items.get(i) else {
                return;
            };
            match (row.key, row.control) {
                (Control::Action(action), _) if delta.is_none() => {
                    activate(&context, action, picker)
                }
                (Control::Setting(setting), UiControl::Toggle(value)) => {
                    change(&context, setting, delta.map(|d| d > 0).unwrap_or(!value))
                }
                (Control::Setting(setting), UiControl::Number { decrease, increase }) => {
                    if let Some(d) = delta
                        && ((d < 0 && decrease) || (d > 0 && increase))
                    {
                        change(&context, setting, d > 0);
                    }
                }
                (Control::Setting(setting), UiControl::Choice) => {
                    if let Some(d) = delta {
                        change(&context, setting, d > 0);
                    } else {
                        choice.set(Some(setting));
                    }
                }
                _ => {}
            }
        }
    });
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let editor = editor.clone();
        let close = close.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if !active || key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            let selected = index.get().min(last);
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => focus.set(if focus.get() == Focus::Tabs {
                    Focus::Form
                } else {
                    Focus::Tabs
                }),
                KeyCode::Left | KeyCode::Char('h') if focus.get() == Focus::Tabs => {
                    panel.set(panel_at((panel.get() as usize + 2) % 3));
                    index.set(0);
                }
                KeyCode::Right | KeyCode::Char('l') if focus.get() == Focus::Tabs => {
                    panel.set(panel_at((panel.get() as usize + 1) % 3));
                    index.set(0);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if focus.get() == Focus::Tabs {
                        focus.set(Focus::Form);
                    } else {
                        index.set((selected + 1).min(last));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') if focus.get() == Focus::Form => {
                    index.set(selected.saturating_sub(1))
                }
                KeyCode::Home if focus.get() == Focus::Form => index.set(0),
                KeyCode::End if focus.get() == Focus::Form => index.set(last),
                KeyCode::Left | KeyCode::Char('h') => {
                    (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Adjust(selected, -1))
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Adjust(selected, 1))
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if focus.get() == Focus::Tabs {
                        focus.set(Focus::Form);
                    } else {
                        (editor.lock().unwrap_or_else(|e| e.into_inner()))(Edit::Activate(
                            selected,
                        ));
                    }
                }
                KeyCode::Esc => (close.lock().unwrap_or_else(|e| e.into_inner()))(()),
                _ => return EventResult::Ignored,
            }
            revision.set(revision.get().wrapping_add(1));
            EventResult::Consumed
        }
    });
    if !open {
        return element!(View(height:Constraint::Length(0))).into_any();
    }
    let enum_view = if let Some(setting) = choice.get() {
        let (options, initial) = options(&snapshot, setting);
        let context = context.clone();
        element!(settings::SettingsPicker(
            ..settings::PickerProps {
                colors: None,
                title: "选择设置".into(),
                options: Arc::new(options),
                initial,
                on_select: settings::callback(move |index| {
                    choose(&context, setting, index);
                    choice.set(None);
                }),
                on_cancel: settings::callback(move |_| choice.set(None))
            }
        ))
        .into_any()
    } else {
        element!(View(height:Constraint::Length(0))).into_any()
    };
    let status = snapshot
        .error
        .clone()
        .unwrap_or_else(|| format!("{} · {}", state_label(snapshot.state), snapshot.progress));
    let current_sound = snapshot
        .playback_sound
        .as_ref()
        .map(|s| sound_label(&snapshot, Some(s)))
        .unwrap_or_else(|| "未开始朗读".into());
    let pending = if snapshot.has_pending_sound() {
        "新声音已保存，下次播放生效；应用并重播可立即切换"
    } else {
        "声音设置用于下次播放；速度、音量与续章立即生效"
    };
    let detail = items
        .get(index.get().min(last))
        .map(|r| r.detail.clone())
        .unwrap_or_default();
    let compact = height < 24;
    element!(Modal(open:true,layer:Some(layer),width:Constraint::Length(width.saturating_sub(4).min(92)),height:Constraint::Percentage(85)){
        Border(border_style:theme.border){View(margin:Margin::new(1,u16::from(!compact))){
            View(height:Constraint::Length(1),flex_direction:Direction::Horizontal){View(width:Constraint::Fill(1)){widget(Line::from("听书设置").style(theme.title))}settings::SettingsButton(..settings::ButtonProps{label:"关闭".into(),on_press:close})}
            settings::SettingsTabs(..settings::TabsProps{labels:Arc::new(vec!["播放".into(),"声音".into(),"高级".into()]),selected:panel.get() as usize,focused:focus.get()==Focus::Tabs,active,on_select:settings::callback(move|value|{panel.set(panel_at(value));index.set(0);focus.set(Focus::Tabs);})})
            View(height:Constraint::Length(1)){Text(text:status,style:if snapshot.error.is_some(){theme.error}else{theme.muted})}
            if !compact {View(height:Constraint::Length(1)){Text(text:format!("当前声音：{current_sound}"),style:theme.muted)}}
            View(height:Constraint::Length(if height<24{1}else{2})){Text(text:pending,style:theme.muted,wrap:true)}
            if !compact && !snapshot.buffer.is_empty(){View(height:Constraint::Length(1)){Text(text:snapshot.buffer.clone(),style:theme.muted)}}
            if !compact && snapshot.sound_capabilities().is_some_and(|caps|caps.backend=="voxcpm"&&caps.model.as_deref()==Some("2b-bf16")){View(height:Constraint::Length(1)){Text(text:"实验模型 · 数值与长期稳定性验收待完成",style:theme.highlight)}}
            settings::SettingsForm<Control>(..settings::FormProps{rows:items,selected:index.get().min(last),focused:focus.get()==Focus::Form,active,identity:panel.get() as usize,keyboard_revision:revision.get(),editor})
            View(height:Constraint::Length(if compact{1}else{2})){Text(text:detail,style:theme.muted,wrap:true)}
        }}
        {enum_view}
        if let Some(kind)=picker.get(){picker::SoundPicker(kind:Some(kind),on_close:move |_|picker.set(None))}
    }).into_any()
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
