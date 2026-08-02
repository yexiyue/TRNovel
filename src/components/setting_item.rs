//! 设置面板的通用条目,听书设置(`pages/read_novel/tts`)与阅读设置
//! (`pages/read_novel/settings`)共用,分两层:
//!
//! - [`SettingItem`] —— 纯容器:带边框的一行,聚焦(编辑中)时边框与文字高亮。
//!   供交互方式特殊的条目使用(如下载进度的 Enter/Esc、音色选择的列表导航)。
//! - [`AdjustableSettingItem`] —— 容器 + 「←/→ 调整」的按键协议。绝大多数设置项
//!   都是「标签 + 值 + 左右增减」,这层把那套事件样板收在一处,调用方只给数据与两个回调。
//!
//! 按键协议只此一份实现很重要:同层多个条目都监听 ←/→,「仅聚焦者可消费」的门控
//! 一旦散落到各组件,漏写一处就会让一次按键被多个条目同时响应。

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Direction, Flex},
    style::Stylize,
    text::Line,
};
use ratatui_kit::prelude::*;

use crate::theme::AppChromeTheme;

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

#[derive(Props, Default)]
pub struct AdjustableSettingItemProps {
    pub is_editing: bool,
    /// 左侧说明文字。
    pub label: String,
    /// 右侧当前值(已格式化好;开关型自行给「开」/「关」这类文案)。
    pub value: String,
    pub on_decrease: Handler<'static, ()>,
    pub on_increase: Handler<'static, ()>,
}

/// 「标签 + 值 + ←/→ 调整」的设置条目。
#[component]
pub fn AdjustableSettingItem(
    props: &mut AdjustableSettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let is_editing = props.is_editing;
    let mut on_decrease = props.on_decrease.take();
    let mut on_increase = props.on_increase.take();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        // 同层所有条目都收到这些键,只有聚焦者可消费。
        if !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                on_decrease(());
                EventResult::Consumed
            }
            KeyCode::Right | KeyCode::Char('l') => {
                on_increase(());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(SettingItem(is_editing: is_editing) {
        View(flex_direction: Direction::Horizontal, justify_content: Flex::SpaceBetween) {
            widget(Line::from(props.label.clone()).style(theme.text))
            widget(Line::from(props.value.clone()).style(theme.text))
        }
    })
}
