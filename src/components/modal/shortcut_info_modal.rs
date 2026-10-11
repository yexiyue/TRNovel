use std::ops::{Deref, DerefMut};

use ratatui::{
    layout::{Constraint, Margin},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Row, Table},
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEventHandler, UseTheme, component, element,
    prelude::{
        EventOptions, EventPriority, EventResult, EventScope, Modal, ScrollView, Scrollbars, View,
    },
};

use crossterm::event::{Event, MouseButton, MouseEventKind};

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
    /// 打开时点击任意处关闭(键盘关闭键仍由各页面 root handler 处理)。
    /// 不传则保持纯展示,点击事件放行。
    pub on_close: Handler<'static, ()>,
}

#[component]
pub fn ShortcutInfoModal(
    props: &mut ShortcutInfoModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let open = props.open;
    let mut on_close = props.on_close.take();

    // 本浮层非阻塞(无输入层),关闭键 i/Esc 在页面 root 层。点击关闭注册为
    // Low + hit_test=false:任何 Left Down 都能到,又不与页面 Normal 级处理器抢键。
    // 浮层本身只有静态表格,点击它唯一合理的动作就是关闭。
    hooks.use_event_handler_with_options(
        EventScope::Current,
        EventPriority::Low,
        EventOptions { hit_test: false },
        move |event| {
            if open
                && !on_close.is_default()
                && let Event::Mouse(m) = event
                && m.kind == MouseEventKind::Down(MouseButton::Left)
            {
                on_close(());
                return EventResult::Consumed;
            }
            EventResult::Ignored
        },
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
        ("显示/隐藏快捷键信息", "I"),
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
        // 非阻塞浮层:本组件不自带 close handler,关闭键 i/Esc 由各页面 root 层 handler 处理。
        // 若用默认 blocks_lower=true 会截断 root 层 → 帮助浮层一开就再也关不掉(全应用键盘卡死)。
        // 各页已用 `is_editing: !info_modal_open` 门控背景列表,故非阻塞不会引入背景误响应。
        blocks_lower: false,
        style:Style::new().dim(),
    ){
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
