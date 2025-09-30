use ratatui::{
    layout::{Constraint, Margin},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Row, Table},
};
use ratatui_kit::{
    AnyElement, Hooks, Props, component, element,
    prelude::{Modal, ScrollView, View},
};

use crate::{components::KeyShortcutInfo, hooks::UseThemeConfig};

#[derive(Debug, Clone, Props, Default)]
pub struct ShortcutInfoModalProps {
    pub key_shortcut_info: KeyShortcutInfo,
    pub open: bool,
}

#[component]
pub fn ShortcutInfoModal(
    props: &ShortcutInfoModalProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let widths = [Constraint::Fill(1), Constraint::Fill(1)];

    let header = Row::new(vec!["描述", "快捷键"]).style(theme.highlight);

    let key_table = Table::new(props.key_shortcut_info.rows(), widths)
        .header(header.clone())
        .block(
            Block::bordered().style(theme.basic.border).not_dim().title(
                Line::from("当前页")
                    .centered()
                    .style(theme.basic.border_title),
            ),
        );

    let global_shortcut_info = KeyShortcutInfo::new(vec![
        ("退出程序", "Q / Ctrl + C"),
        ("后退", "B"),
        ("后退(不保存记录)", "Backspace"),
        ("前进", "G"),
        ("显示/隐藏快捷键信息", "I"),
    ]);

    let global_table = Table::new(global_shortcut_info.rows(), widths)
        .header(header)
        .block(
            Block::bordered().style(theme.basic.border).not_dim().title(
                Line::from("全局")
                    .centered()
                    .style(theme.basic.border_title),
            ),
        );

    let global_height = global_shortcut_info.0.len() as u16;
    let key_height = props.key_shortcut_info.0.len() as u16;

    element!(Modal(
        width:Constraint::Percentage(60),
        height:Constraint::Percentage(50),
        open:props.open,
        style:Style::new().dim(),
    ){
        ScrollView(margin:Margin::new(0,1)){
            View(height:Constraint::Length(key_height+3)){
                $key_table
            }
            View(height:Constraint::Length(global_height+3)){
                $global_table
            }
        }
    })
}
