//! Playback controls and staged single-voice preferences.
use super::{Snapshot, TtsContext};
use crate::{
    components::settings::{self, Control as UiControl, Edit, Focus},
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Margin},
    text::Line,
};
use ratatui_kit::prelude::*;
use std::sync::Arc;
use tts_protocol::ConfigPatch;
mod model;
use model::*;
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
            let state = settings::navigation::Navigation {
                tab: panel.get() as usize,
                row: index.get(),
                focus: focus.get(),
            };
            let Some(command) = settings::navigation::command(key, state, 3, last + 1) else {
                return EventResult::Ignored;
            };
            use settings::navigation::Command;
            match command {
                Command::Tab(value) => {
                    panel.set(panel_at(value));
                    index.set(0);
                }
                Command::Focus(next) => focus.set(next),
                Command::Row(value) => index.set(value),
                Command::Edit(edit) => editor.call(edit),
                Command::Close => close.call(()),
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
    let tabs_props = settings::TabsProps {
        labels: Arc::new(vec!["播放".into(), "声音".into(), "高级".into()]),
        selected: panel.get() as usize,
        focused: focus.get() == Focus::Tabs,
        active,
        on_select: settings::callback(move |value| {
            panel.set(panel_at(value));
            index.set(0);
            focus.set(Focus::Tabs);
        }),
    };
    let form_props = settings::FormProps {
        rows: items,
        selected: index.get().min(last),
        focused: focus.get() == Focus::Form,
        active,
        identity: panel.get() as usize,
        keyboard_revision: revision.get(),
        editor,
    };
    element!(Modal(open:true,layer:Some(layer),width:Constraint::Length(width.saturating_sub(4).min(92)),height:Constraint::Percentage(85)){
        Border(border_style:theme.border){View(margin:Margin::new(1,u16::from(!compact))){
            View(height:Constraint::Length(1),flex_direction:Direction::Horizontal){View(width:Constraint::Fill(1)){widget(Line::from("听书设置").style(theme.title))}settings::SettingsButton(..settings::ButtonProps{label:"关闭".into(),on_press:close})}
            settings::SettingsTabs(..tabs_props)
            View(height:Constraint::Length(1)){Text(text:status,style:if snapshot.error.is_some(){theme.error}else{theme.muted})}
            if !compact {View(height:Constraint::Length(1)){Text(text:format!("当前声音：{current_sound}"),style:theme.muted)}}
            View(height:Constraint::Length(if height<24{1}else{2})){Text(text:pending,style:theme.muted,wrap:true)}
            if !compact && !snapshot.buffer.is_empty(){View(height:Constraint::Length(1)){Text(text:snapshot.buffer.clone(),style:theme.muted)}}
            if !compact && snapshot.sound_capabilities().is_some_and(|caps|caps.backend=="voxcpm"&&caps.model.as_deref()==Some("2b-bf16")){View(height:Constraint::Length(1)){Text(text:"实验模型 · 数值与长期稳定性验收待完成",style:theme.highlight)}}
            settings::SettingsForm<Control>(..form_props)
            View(height:Constraint::Length(if compact{1}else{2})){Text(text:detail,style:theme.muted,wrap:true)}
        }}
        {enum_view}
        if let Some(kind)=picker.get(){picker::SoundPicker(kind:Some(kind),on_close:move |_|picker.set(None))}
    }).into_any()
}
