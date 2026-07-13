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
use std::collections::HashSet;
use tui_widget_list::ListState;

#[derive(Props)]
pub struct MultiListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    pub items: Vec<T>,
    pub on_select: ratatui_kit::Handler<'static, Vec<T>>,
    pub top_title: Option<ratatui::text::Line<'static>>,
    pub bottom_title: Option<ratatui::text::Line<'static>>,
    pub is_editing: bool,
    pub render_item: RenderItem<'static>,
    pub empty_message: String,
    pub state: Option<State<HashSet<usize>>>,
    pub loading: bool,
    pub loading_tip: String,
}

impl<T> Default for MultiListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    fn default() -> Self {
        Self {
            items: vec![],
            on_select: Handler::default(),
            top_title: None,
            bottom_title: None,
            is_editing: false,
            render_item: RenderItem::default(),
            empty_message: String::default(),
            state: None,
            loading: false,
            loading_tip: String::from("加载中..."),
        }
    }
}

#[component]
pub fn MultiListSelect<T>(
    props: &mut MultiListSelectProps<T>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>>
where
    T: Unpin + Sync + Clone + Send + 'static,
{
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let state = hooks.use_state(ListState::default);
    let selected = hooks.use_state(HashSet::<usize>::default);
    let selected = props.state.unwrap_or(selected);

    let is_empty = props.items.is_empty();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let is_editing = props.is_editing;
        let mut on_select = props.on_select.take();
        let data = props.items.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press || !is_editing {
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
                KeyCode::Char('\n') | KeyCode::Char(' ') => {
                    if let Some(item) = state.read().selected {
                        let is_included = selected.read().contains(&item);
                        if is_included {
                            selected.write().remove(&item);
                        } else {
                            selected.write().insert(item);
                        }
                    }
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    // 先把选中项收进 Vec 并**释放 selected 的读 guard**,再调 on_select。
                    // 否则 `selected.read()` 作为实参临时量,其读 guard 会存活到整条语句结束——
                    // 即在 on_select 执行期间仍持读借用;而 on_select 内若写同一个 selected
                    //(如导入页的 `selected.write().clear()`)→ 同线程「读未释放又写」→
                    // parking_lot RwLock 死锁(generational-box 的 try_write 名为 try 实为阻塞,
                    // 不返回 Err 而是永久 block)→ 渲染主循环 dispatch 卡死,整个 TUI 冻结。
                    // 与框架内置 MultiSelect 的两语句写法一致。
                    let selected_items: Vec<T> = {
                        let selected = selected.read();
                        // data.get(i) 而非裸 data[i]:选中后若 items 缩减(重新解析返回更少项),
                        // 残留的选中下标会越界 panic;过滤掉失效下标更稳。
                        selected
                            .iter()
                            .filter_map(|&i| data.get(i).cloned())
                            .collect()
                    };
                    on_select(selected_items);
                    EventResult::Consumed
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
        render_item: props.render_item.take(),
        item_count: props.items.len(),
    ))
    .into_any()
}
