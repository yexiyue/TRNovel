//! Snapshot projection and option metadata for the listening settings UI.
use crate::{
    components::settings::{self, Control as UiControl},
    config::SoundConfig,
    tts::Snapshot,
};
use tts_protocol::{Device, SessionState};
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Panel {
    #[default]
    Playback,
    Sound,
    Advanced,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerKind {
    Model,
    Voice,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Setting {
    Speed,
    Volume,
    AutoPlay,
    Playback,
    Style,
    Device,
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Playback,
    Pick(PickerKind),
    Apply,
    ApplyFromStart,
    Prepare,
    Release,
    CancelPrepare,
}
pub(super) struct Row {
    pub label: &'static str,
    pub value: String,
    pub control: Control,
}
#[derive(Clone, Copy)]
pub(super) enum Control {
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
pub(super) fn sound_label(snapshot: &Snapshot, sound: Option<&SoundConfig>) -> String {
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
pub(super) fn rows(snapshot: &Snapshot, panel: Panel) -> Vec<Row> {
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
pub(super) const SPEED_MIN: f32 = 0.5;
pub(super) const SPEED_MAX: f32 = 2.0;
pub(super) const VOLUME_MAX: f32 = 10.0;
const STYLES: [&str; 4] = [
    "",
    "温暖沉稳，适合小说旁白。",
    "轻松自然，清晰流畅。",
    "富有感情，突出人物对话。",
];
pub(super) fn options(snapshot: &Snapshot, setting: Setting) -> (Vec<String>, usize) {
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
pub(super) fn style_values(snapshot: &Snapshot) -> Vec<String> {
    let mut values = STYLES.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    if let Some(value) = snapshot.sound.as_ref().and_then(|s| s.style.as_ref())
        && !values.contains(value)
    {
        values.push(value.clone());
    }
    values
}
pub(super) fn devices(snapshot: &Snapshot) -> Vec<Device> {
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
pub(super) fn form_rows(snapshot: &Snapshot, panel: Panel) -> Vec<settings::Row<Control>> {
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
                swatch: None,
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
pub(super) fn panel_at(index: usize) -> Panel {
    match index {
        1 => Panel::Sound,
        2 => Panel::Advanced,
        _ => Panel::Playback,
    }
}
pub(super) fn state_label(state: SessionState) -> &'static str {
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
