use crate::{
    components::{
        Loading,
        list_select::pointer_item_index,
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
use std::{cell::RefCell, collections::HashSet, rc::Rc};
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
    let item_count = props.items.len();
    let interactive = props.is_editing && !props.loading && !is_empty;

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
                KeyCode::Char('\n') | KeyCode::Char(' ') => {
                    let cursor = state.read().selected;
                    if let Some(item) = cursor.filter(|index| *index < item_count) {
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
                    (on_select.borrow_mut())(selected_items);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });

    // 鼠标:滚轮移动光标(j/k 同义);点击条目 = 移动光标并切换勾选(空格同义)。
    // 确认仍是 Enter —— 鼠标多选的惯例是勾选,最后确认交给键盘或双击留给将来。
    hooks.use_pointer(PointerOptions::default(), {
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
                    let hit = pointer_item_index(&state.read(), ev, item_count, interactive);
                    if let Some(index) = hit {
                        state.write().select(Some(index));
                        let checked = selected.read().contains(&index);
                        if checked {
                            selected.write().remove(&index);
                        } else {
                            selected.write().insert(index);
                        }
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
