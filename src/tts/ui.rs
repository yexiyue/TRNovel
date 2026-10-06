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
    text::Line,
    widgets::Block,
};
use ratatui_kit::prelude::*;
use tts_protocol::ConfigPatch;

#[derive(Props, Default)]
pub struct TTSManagerProps {
    pub open: bool,
    pub is_editing: bool,
}
const ITEM_COUNT: usize = 7;

#[component]
pub fn TTSManager(props: &TTSManagerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let context = hooks.use_context::<TtsContext>().clone();
    let snapshot = context.snapshot.read().clone();
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let open = props.open;
    let editing = props.is_editing;
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
    let scroll = hooks.use_state(ScrollViewState::default);
    let (_, height) = hooks.use_terminal_size();
    hooks.use_effect(
        move || scroll.write().scroll_to_index(index.get()),
        (index.get(), height),
    );
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
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
                    index.set((index.get() + 1).min(ITEM_COUNT - 1))
                }
                KeyCode::Up | KeyCode::Char('k') => index.set(index.get().saturating_sub(1)),
                KeyCode::Enter => match index.get() {
                    0 => handle.prepare(),
                    5 => handle.restart(),
                    6 => handle.release(),
                    _ => return EventResult::Ignored,
                },
                KeyCode::Esc if index.get() == 0 => handle.cancel_prepare(),
                _ => return EventResult::Ignored,
            }
            EventResult::Consumed
        }
    });
    let status = snapshot.error.clone().unwrap_or(snapshot.progress);
    element!(Modal(width:Constraint::Percentage(80),height:Constraint::Percentage(80),open:open,blocks_lower:false,margin:Margin::new(1,1),style:Style::default().dim()) {
        View(margin:Margin::new(1,1)) {
            ScrollView(active:false,state:scroll,block:Block::bordered().border_style(theme.border.not_dim()).title_top(Line::from("听书设置").centered().style(theme.title))) {
                View(height:Constraint::Length(3)) {
                    SettingItem(is_editing:editing && index.get()==0,top_title:"Enter 启用模型 / Esc 取消准备".to_string()) {
                        widget(Line::from(status))
                    }
                }
                ListeningSetting(kind:Setting::Voice,is_editing:editing && index.get()==1)
                ListeningSetting(kind:Setting::Speed,is_editing:editing && index.get()==2)
                ListeningSetting(kind:Setting::Volume,is_editing:editing && index.get()==3)
                ListeningSetting(kind:Setting::AutoPlay,is_editing:editing && index.get()==4)
                View(height:Constraint::Length(3)) {
                    SettingItem(is_editing:editing && index.get()==5) {widget(Line::from("Enter 从本章开头重新播放（忽略恢复点）"))}
                }
                View(height:Constraint::Length(3)) {
                    SettingItem(is_editing:editing && index.get()==6) {widget(Line::from("Enter 停播并释放模型、音频和子进程"))}
                }
            }
        }
    })
}

#[derive(Clone, Copy, Default)]
enum Setting {
    #[default]
    Voice,
    Speed,
    Volume,
    AutoPlay,
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
        Setting::Speed => "播放速度",
        Setting::Volume => "音量",
        Setting::AutoPlay => "自动续章",
    };
    let value = snapshot.config.as_ref().map_or_else(
        || "未连接".to_string(),
        |config| match kind {
            Setting::Voice => config.voice.clone(),
            Setting::Speed => format!("{}x", config.speed),
            Setting::Volume => format!("{}x", config.volume),
            Setting::AutoPlay => config.auto_play.to_string(),
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
    }
    context.handle.update(patch);
}
