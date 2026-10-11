use crate::{
    components::{
        Loading,
        list_view::{ListView, RenderItem},
    },
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    widgets::Block,
};
use ratatui_kit::prelude::*;
use std::{cell::RefCell, rc::Rc};
use tui_widget_list::{ListState, hit_test::Hit};

/// 两种业务列表共用命中判断,加载/空态不使用上一次列表渲染的命中缓存。
pub(super) fn pointer_item_index(
    state: &ListState,
    event: PointerEvent,
    item_count: usize,
    interactive: bool,
) -> Option<usize> {
    if !interactive || !matches!(event.kind, PointerKind::Click { .. }) {
        return None;
    }
    match state.hit_test(event.position.x, event.position.y) {
        Some(Hit::Item(index)) if index < item_count => Some(index),
        _ => None,
    }
}

#[derive(Props)]
pub struct ListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    pub items: Vec<T>,
    pub on_select: ratatui_kit::Handler<'static, T>,
    pub default_value: Option<usize>,
    pub top_title: Option<ratatui::text::Line<'static>>,
    pub bottom_title: Option<ratatui::text::Line<'static>>,
    pub is_editing: bool,
    pub render_item: RenderItem<'static>,
    pub state: Option<ratatui_kit::State<ListState>>,
    pub empty_message: String,
    pub loading: bool,
    pub loading_tip: String,
}

impl<T> Default for ListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    fn default() -> Self {
        Self {
            items: vec![],
            on_select: Handler::default(),
            default_value: None,
            top_title: None,
            bottom_title: None,
            is_editing: false,
            render_item: RenderItem::default(),
            state: None,
            empty_message: String::default(),
            loading: false,
            loading_tip: String::from("加载中..."),
        }
    }
}

#[component]
pub fn ListSelect<T>(
    props: &mut ListSelectProps<T>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>>
where
    T: Unpin + Sync + Clone + Send + 'static,
{
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let state = hooks.use_state(|| {
        let mut state = ListState::default();
        state.select(props.default_value);
        state
    });
    let is_empty = props.items.is_empty();
    let interactive = props.is_editing && !props.loading && !is_empty;

    let state = props.state.unwrap_or(state);

    // 键盘与鼠标两个闭包共享同一个回调(Handler 不 Clone,经 Rc<RefCell> 转发)。
    let on_select = Rc::new(RefCell::new(props.on_select.take()));

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let on_select = on_select.clone();
        let data = props.items.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press || !interactive {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    state.write().next();
                    EventResult::Consumed
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    state.write().previous();
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    // 用 data.get(path) 而非裸 data[path]:列表可能为空但 state.selected 仍是
                    // 强制的 Some(0)(如空书源列表按 Tab 进只选模式、或书源删到空),
                    // 裸索引会 index out of bounds panic 直接崩溃退出。
                    let res = state.read().selected;
                    if let Some(path) = res
                        && let Some(item) = data.get(path)
                    {
                        (on_select.borrow_mut())(item.clone());
                    }
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });

    // 鼠标:滚轮移动选中(j/k 同义);点击条目 = 选中并确认(Enter 同义)。
    // 行→索引交给 tui-widget-list 的 hit_test —— 条目高度可变(历史项 5 行、书源项 3 行),
    // 它按上一帧记录的可见条目尺寸精确映射,截断的首条目也能算对。
    hooks.use_pointer(PointerOptions::default(), {
        let on_select = on_select.clone();
        let data = props.items.clone();
        move |ev| {
            if !interactive {
                return EventResult::Ignored;
            }
            match ev.kind {
                PointerKind::ScrollUp => {
                    state.write().previous();
                    EventResult::Consumed
                }
                PointerKind::ScrollDown => {
                    state.write().next();
                    EventResult::Consumed
                }
                PointerKind::Click { .. } => {
                    // 释放命中读取后再修改响应式状态,避免读写 guard 交叠。
                    let hit = pointer_item_index(&state.read(), ev, data.len(), interactive);
                    if let Some(index) = hit {
                        state.write().select(Some(index));
                        (on_select.borrow_mut())(data[index].clone());
                        EventResult::Consumed
                    } else {
                        EventResult::Ignored
                    }
                }
                _ => EventResult::Ignored,
            }
        }
    });

    let mut border = Block::bordered().border_style(theme.border);

    if let Some(title) = props.top_title.clone() {
        border = border.title_top(title);
    }
    if let Some(title) = props.bottom_title.clone() {
        border = border.title_bottom(title);
    }

    if props.loading {
        return element!(
            Border(
                top_title: props.top_title.clone(),
                bottom_title: props.bottom_title.clone(),
                border_style: theme.border,
            ){
                Loading(tip: props.loading_tip.clone())
            }
        )
        .into_any();
    }

    if is_empty {
        return element!(
            Border(
                top_title: props.top_title.clone(),
                bottom_title: props.bottom_title.clone(),
                border_style: theme.border,
            ){
                Center(
                    height:Constraint::Length(5),
                    width:Constraint::Percentage(50)
                ){
                    Text(
                        text: props.empty_message.clone(),
                        alignment: Alignment::Center,
                        style: theme.empty,
                        wrap: true,
                    )
                }
            }
        )
        .into_any();
    }

    element!(ListView(
        state: state,
        block: border,
        render_item: std::mem::take(&mut props.render_item),
        item_count: props.items.len(),
    ))
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton};
    use ratatui::{
        buffer::Buffer,
        layout::{Position, Rect},
        text::Line,
        widgets::StatefulWidget,
    };
    use tui_widget_list::{ListBuilder, ListView as WidgetListView};

    #[test]
    fn loading_and_empty_views_do_not_hit_cached_list_items() {
        let area = Rect::new(0, 0, 20, 8);
        let mut state = ListState::default();
        let builder = ListBuilder::new(|context| (Line::from(format!("row{}", context.index)), 2));
        WidgetListView::new(builder, 3)
            .block(Block::bordered())
            .render(area, &mut Buffer::empty(area), &mut state);
        let event = PointerEvent {
            kind: PointerKind::Click {
                button: MouseButton::Left,
                count: 1,
            },
            position: Position::new(1, 3),
            screen_position: Position::new(1, 3),
            modifiers: KeyModifiers::NONE,
            area,
        };
        assert_eq!(pointer_item_index(&state, event, 3, true), Some(1));
        // 加载画面保留旧 ListState,但其旧命中不能确认/勾选隐藏条目。
        assert_eq!(pointer_item_index(&state, event, 3, false), None);
        assert_eq!(pointer_item_index(&state, event, 0, false), None);
        // 列表缩短时,即使命中缓存尚未刷新,也不能返回越界条目。
        assert_eq!(pointer_item_index(&state, event, 1, true), None);
        assert_eq!(
            pointer_item_index(
                &state,
                PointerEvent {
                    position: Position::new(0, 3),
                    ..event
                },
                3,
                true
            ),
            None
        );
    }
}
