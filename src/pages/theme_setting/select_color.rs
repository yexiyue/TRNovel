use std::str::FromStr;

use ratatui::{
    layout::{Constraint, Margin},
    style::{Color, Style},
    text::Span,
    widgets::ListItem,
};
use ratatui_kit::prelude::*;

use crate::components::{search_input::SearchInput, select::Select};

#[derive(Default, Props)]
pub struct SelectColorProps {
    pub color: Color,
    pub is_editing: bool,
    pub on_change: Handler<'static, Color>,
}

#[derive(Clone)]
struct ColorItem(Color);

impl From<Color> for ColorItem {
    fn from(color: Color) -> Self {
        Self(color)
    }
}

impl From<ColorItem> for ListItem<'static> {
    fn from(value: ColorItem) -> Self {
        ListItem::new(Span::styled(value.0.to_string(), value.0))
    }
}

#[component]
pub fn SelectColor(
    props: &mut SelectColorProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let mut on_change = props.on_change.take();
    let is_editing = props.is_editing;

    let tx = hooks.use_memo(
        move || {
            if is_editing {
                let (tx, mut rx) = tokio::sync::mpsc::channel::<Color>(1);
                tokio::spawn(async move {
                    while let Some(color) = rx.recv().await {
                        on_change(color);
                    }
                });
                Some(tx)
            } else {
                None
            }
        },
        props.is_editing,
    );

    let list = vec![
        Color::Reset,
        Color::Black,
        Color::White,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
    ]
    .into_iter()
    .map(|color| color.into())
    .collect::<Vec<ColorItem>>();

    let default = list.iter().position(|i| i.0 == props.color);

    element!(Modal(
        width:Constraint::Percentage(60),
        height:Constraint::Percentage(70),
        open: props.is_editing,
        // 非阻塞浮层:取色弹窗内含 SearchInput/Select(自管各自层),取消键 Esc 由父级 ThemeSetting
        // 的 root 层 handler 处理(current.set(None))。若用默认 blocks_lower=true 会截断 root 层 →
        // Esc 永远关不掉弹窗(只能靠选中颜色退出)。背景主题色列表已用 `current.is_none()` 门控,
        // 故非阻塞不会引入背景误响应。
        blocks_lower: false,
        style:Style::new().dim(),
    ){
        View(
            margin: Margin::new(1,1),
        ){
            SearchInput(
                is_editing: props.is_editing,
                value: if default.is_some() {
                    "".to_string()
                } else {
                    props.color.to_string()
                },
                validate: move |value:String| {
                    let color = Color::from_str(&value);
                    (color.is_ok(), color.err().map(|e| e.to_string()).unwrap_or_default())
                },
                clear_on_escape: true,
                placeholder: "输入颜色名称如: Red 或 #FF0000".to_string(),
                on_submit: {
                    let tx=tx.clone();
                    move |value:String| {
                        let color = Color::from_str(&value);
                        if let Ok(color) = color {
                            // color_value.set(color);
                            if let Some(tx) = tx.as_ref() {
                                let _ = tx.try_send(color);
                            }
                            true
                        } else {
                            false
                        }
                    }
                },
            )
            Select<ColorItem>(
                items: list,
                is_editing: props.is_editing,
                default_value: default,
                on_select: move |item: ColorItem| {
                    if let Some(tx) = tx.as_ref() {
                        let _ = tx.try_send(item.0);
                    }
                },
                highlight_symbol: "➤ ",
            )
        }
    })
}
