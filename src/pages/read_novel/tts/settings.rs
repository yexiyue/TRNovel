//! 听书设置面板的条目。三项都是「标签 + 值 + ←/→ 调整」,按键协议全部走
//! [`AdjustableSettingItem`],这里只提供数据与两个回调。

use crate::{TTSConfig, components::AdjustableSettingItem};
use ratatui_kit::prelude::*;

#[derive(Props, Default)]
pub struct TTSSettingProps {
    pub is_editing: bool,
}

#[component]
pub fn SpeedSetting(props: &TTSSettingProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let speed = tts_config.read().speed;

    element!(AdjustableSettingItem(
        is_editing: props.is_editing,
        label: "播放速度:".to_string(),
        value: format!("{speed}x"),
        on_decrease: move |_| tts_config.write().decrease_speed(),
        on_increase: move |_| tts_config.write().increase_speed(),
    ))
}

#[component]
pub fn VolumeSetting(props: &TTSSettingProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let volume = tts_config.read().volume;

    element!(AdjustableSettingItem(
        is_editing: props.is_editing,
        label: "音量:".to_string(),
        value: format!("{volume}x"),
        on_decrease: move |_| tts_config.write().decrease_volume(),
        on_increase: move |_| tts_config.write().increase_volume(),
    ))
}

#[component]
pub fn AutoPlaySetting(props: &TTSSettingProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let auto_play = tts_config.read().auto_play;

    element!(AdjustableSettingItem(
        is_editing: props.is_editing,
        label: "自动播放:".to_string(),
        value: auto_play.to_string(),
        on_decrease: move |_| tts_config.write().auto_play = false,
        on_increase: move |_| tts_config.write().auto_play = true,
    ))
}
