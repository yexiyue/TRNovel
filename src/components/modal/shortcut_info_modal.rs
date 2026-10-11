use std::ops::{Deref, DerefMut};

use ratatui::{
    layout::{Constraint, Margin},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Row, Table},
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEventHandler, UseInputLayer, UseTheme, component,
    element,
    prelude::{
        EventOptions, EventPriority, EventResult, EventScope, Modal, ScrollView, Scrollbars, View,
    },
};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEventKind};

use crate::theme::AppChromeTheme;

#[derive(Debug, Clone, Default)]
pub struct KeyShortcutInfo(pub Vec<(String, String)>);

impl Deref for KeyShortcutInfo {
    type Target = Vec<(String, String)>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for KeyShortcutInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<KeyShortcutInfo> for Vec<(String, String)> {
    fn from(value: KeyShortcutInfo) -> Self {
        value.0
    }
}

impl KeyShortcutInfo {
    pub fn new(data: Vec<(&str, &str)>) -> Self {
        Self(
            data.into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    pub fn rows(&self) -> Vec<Row<'static>> {
        self.0
            .iter()
            .map(|(key, info)| Row::new(vec![key.clone(), info.clone()]))
            .collect::<Vec<_>>()
    }
}

impl From<Vec<(&str, &str)>> for KeyShortcutInfo {
    fn from(data: Vec<(&str, &str)>) -> Self {
        Self::new(data)
    }
}

#[derive(Props, Default)]
pub struct ShortcutInfoModalProps {
    pub key_shortcut_info: KeyShortcutInfo,
    pub open: bool,
    /// Close callback for the button, escape key and scrim.
    pub close_shortcut: String,
    pub close_on_key: Handler<'static, KeyEvent, bool>,
    pub on_close: Handler<'static, ()>,
}

#[component]
pub fn ShortcutInfoModal(
    props: &mut ShortcutInfoModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let open = props.open;
    let on_close = std::sync::Arc::new(std::sync::Mutex::new(props.on_close.take()));
    let mut close_on_key = props.close_on_key.take();
    let close_shortcut = if props.close_shortcut.is_empty() {
        "I".to_string()
    } else {
        props.close_shortcut.clone()
    };
    let layer = hooks.use_input_layer(open, true);
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::Normal, {
        let on_close = on_close.clone();
        move |event| {
            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
                && (key.code == KeyCode::Esc
                    || if close_on_key.is_default() {
                        matches!(key.code, KeyCode::Char('i' | 'I'))
                    } else {
                        close_on_key(key)
                    })
            {
                on_close.lock().unwrap_or_else(|e| e.into_inner())(());
                EventResult::Consumed
            } else {
                EventResult::Ignored
            }
        }
    });

    hooks.use_event_handler_with_options(
        EventScope::Layer(layer), EventPriority::Low, EventOptions { hit_test: false },
        { let on_close = on_close.clone(); move |event| {
            if open && matches!(event, Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left)) {
                on_close.lock().unwrap_or_else(|e| e.into_inner())(());
                EventResult::Consumed
            } else { EventResult::Ignored }
        } },
    );

    let widths = [Constraint::Fill(1), Constraint::Fill(1)];

    let header = Row::new(vec!["描述", "快捷键"]).style(theme.highlight);

    let key_table = Table::new(props.key_shortcut_info.rows(), widths)
        .header(header.clone())
        .block(
            Block::bordered()
                .style(theme.border)
                .not_dim()
                .title(Line::from("当前页").centered().style(theme.title)),
        )
        .style(theme.text);

    let global_shortcut_info = KeyShortcutInfo::new(vec![
        ("退出程序", "Q / Ctrl + C"),
        ("后退", "B"),
        ("前进", "G"),
        ("显示/隐藏快捷键信息", close_shortcut.as_str()),
    ]);

    let global_table = Table::new(global_shortcut_info.rows(), widths)
        .header(header)
        .block(
            Block::bordered()
                .style(theme.border)
                .not_dim()
                .title(Line::from("全局").centered().style(theme.title)),
        );

    let global_height = global_shortcut_info.0.len() as u16;
    let key_height = props.key_shortcut_info.0.len() as u16;

    element!(Modal(
        width:Constraint::Percentage(60),
        height:Constraint::Percentage(50),
        open:props.open,
        layer: layer,
        style:Style::new().dim(),
    ){
        crate::components::ActionBar {
            crate::components::ActionButton(label: "关闭帮助", shortcut: format!("Esc / {close_shortcut}"), on_press: move |_| on_close.lock().unwrap_or_else(|e| e.into_inner())(()))
        }
        ScrollView(margin:Margin::new(1,1),scrollbars:Scrollbars {
            vertical_scrollbar:crate::theme::scrollbar_style(theme.border.not_dim(),theme.title),
            horizontal_scrollbar:crate::theme::scrollbar_style(theme.border.not_dim(),theme.title),
            ..Default::default()
        }){
            View(height:Constraint::Length(key_height+3)){
                widget(key_table)
            }
            View(height:Constraint::Length(global_height+3)){
                widget(global_table)
            }
        }
    })
}
