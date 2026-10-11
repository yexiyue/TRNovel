//! 设置面板的通用条目,听书设置(`pages/read_novel/tts`)与阅读设置
//! (`pages/read_novel/settings`)共用,分两层:
//!
//! - [`SettingItem`] —— 纯容器:带边框的一行,聚焦(编辑中)时边框与文字高亮。
//!   供交互方式特殊的条目使用(如下载进度的 Enter/Esc、音色选择的列表导航)。
//!   鼠标:点击 = `on_click`(聚焦);已聚焦再点 = `on_activate`(执行该条的 Enter 动作)。
//! - [`AdjustableSettingItem`] —— 容器 + 「←/→ 调整」的按键协议。绝大多数设置项
//!   都是「标签 + 值 + 左右增减」,这层把那套事件样板收在一处,调用方只给数据与两个回调。
//!   鼠标:首次点击聚焦;已聚焦后点左半 = 减、右半 = 加。
//!
//! 按键协议只此一份实现很重要:同层多个条目都监听 ←/→,「仅聚焦者可消费」的门控
//! 一旦散落到各组件,漏写一处就会让一次按键被多个条目同时响应。
//! 鼠标点击经 use_pointer 的命中过滤天然只落在被点的那个条目上,无需门控。

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Direction, Flex},
    style::Stylize,
    text::Line,
};
use ratatui_kit::prelude::*;
use std::{cell::RefCell, rc::Rc};

use crate::theme::AppChromeTheme;

/// 构造「点击某行 → 面板焦点移到该行」的回调,听书/阅读两个设置面板的接线共用。
pub(crate) fn focus_row(mut index: ratatui_kit::State<usize>, row: usize) -> Handler<'static, ()> {
    Handler::from(move |_: ()| index.set(row))
}

#[derive(Props, Default)]
pub struct SettingItemProps {
    pub is_editing: bool,
    pub top_title: String,
    pub bottom_title: String,
    /// 点击本条目的回调(通常用来把面板焦点移到这里)。不传则点击放行。
    pub on_click: Handler<'static, ()>,
    /// 已聚焦时再次点击的激活回调(如「Enter 启用模型」这类动作条目)。不传则不激活。
    pub on_activate: Handler<'static, ()>,
    pub children: Vec<AnyElement<'static>>,
}

#[component]
pub fn SettingItem(
    props: &mut SettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let is_editing = props.is_editing;
    let mut on_click = props.on_click.take();
    let mut on_activate = props.on_activate.take();

    // 鼠标点击:先请求聚焦;已聚焦且有激活动作则激活。两回调都未接的条目点击放行,
    // 不吞事件(面板上还有别的处理器,无动作的吞噬只会让「点了没反应」更难排查)。
    hooks.use_pointer(PointerOptions::default(), move |ev| {
        if let PointerKind::Click { .. } = ev.kind {
            if on_click.is_default() && on_activate.is_default() {
                return EventResult::Ignored;
            }
            on_click(());
            if is_editing && !on_activate.is_default() {
                on_activate(());
            }
            EventResult::Consumed
        } else {
            EventResult::Ignored
        }
    });

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
    /// 点击本条目的回调(面板用来移动焦点)。不传则点击只调整已聚焦条目。
    pub on_click: Handler<'static, ()>,
}

/// 「标签 + 值 + ←/→ 调整」的设置条目。
///
/// 鼠标:首次点击聚焦本条目;已聚焦后点击**左半 = 减、右半 = 加**(与 ←/→ 同一套回调,
/// 边界钳制自然继承)。两步式让「点哪儿都是先聚焦」,不会一上来就改值。
#[component]
pub fn AdjustableSettingItem(
    props: &mut AdjustableSettingItemProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let is_editing = props.is_editing;
    // 键盘(←/→)与鼠标(左/右半点击)两个闭包共享同一对回调(Handler 不 Clone)。
    let on_decrease = Rc::new(RefCell::new(props.on_decrease.take()));
    let on_increase = Rc::new(RefCell::new(props.on_increase.take()));
    let mut on_click = props.on_click.take();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let on_decrease = on_decrease.clone();
        let on_increase = on_increase.clone();
        move |event| {
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
                    (on_decrease.borrow_mut())(());
                    EventResult::Consumed
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    (on_increase.borrow_mut())(());
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });

    hooks.use_pointer(PointerOptions::default(), {
        let on_decrease = on_decrease.clone();
        let on_increase = on_increase.clone();
        move |ev| {
            if let PointerKind::Click { .. } = ev.kind {
                on_click(());
                if is_editing {
                    let mid = ev.area.x + ev.area.width / 2;
                    if ev.position.x < mid {
                        (on_decrease.borrow_mut())(());
                    } else {
                        (on_increase.borrow_mut())(());
                    }
                }
                EventResult::Consumed
            } else {
                EventResult::Ignored
            }
        }
    });

    element!(SettingItem(is_editing: is_editing) {
        View(flex_direction: Direction::Horizontal, justify_content: Flex::SpaceBetween) {
            widget(Line::from(props.label.clone()).style(theme.text))
            widget(Line::from(props.value.clone()).style(theme.text))
        }
    })
}
