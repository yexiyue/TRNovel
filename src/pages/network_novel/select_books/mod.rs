use crate::components::modal::shortcut_info_modal::ShortcutInfoModal;
use crate::{
    components::{WarningModal, select::Select},
    errors::Errors,
    hooks::UseInitState,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::{BookSource, Engine, ExploreEntry};
use ratatui::{
    layout::{Constraint, Margin},
    style::Style,
    text::Line,
    widgets::{ListItem, ListState},
};
use ratatui_kit::prelude::*;
use std::hash::Hash;
mod find_book;
use find_book::*;

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct ExploreListItem(pub ExploreEntry);

impl From<ExploreListItem> for ListItem<'_> {
    fn from(value: ExploreListItem) -> Self {
        ListItem::new(value.0.title.clone())
    }
}

#[component]
pub fn SelectBooks(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut info_modal_open = hooks.use_state(|| false);
    let book_source = hooks.use_route_state::<BookSource>();
    let mut explores = hooks.use_state(std::vec::Vec::new);
    let theme = hooks.use_component_theme::<AppChromeTheme>();

    let list_state = hooks.use_state(ListState::default);
    let mut is_explore_open = hooks.use_state(|| false);

    let mut current_explore = hooks.use_state(|| None::<ExploreListItem>);

    let (book_source_engine, engine_loading, error) = hooks.use_init_state(async move {
        let engine = crate::browser_assist::build_engine((*book_source).clone())?;
        // 预热会话 cookie(若书源配置了 http.warmup)。
        engine.warmup().await;
        // 动态入口加载:静态固定入口 + 远端抓取入口(可能请求分类 API),故为 async。
        let book_source_explores = engine.explore_entries().await?;

        if let Some(explore) = book_source_explores.first() {
            current_explore.set(Some(ExploreListItem(explore.clone())));
            list_state.write().select_first();
        }

        explores.set(book_source_explores);
        Ok::<Engine, Errors>(engine)
    });

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Tab => {
                is_explore_open.set(!is_explore_open.get());
                EventResult::Consumed
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                info_modal_open.set(!info_modal_open.get());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    let explores_list = hooks.use_memo(
        move || {
            explores
                .read()
                .iter()
                .map(|item| ExploreListItem(item.clone()))
                .collect::<Vec<_>>()
        },
        explores.read().len(),
    );

    element!(View {
        FindBooks(..FindBooksProps {
                engine: book_source_engine,
                current_explore: current_explore.read().clone(),
                is_editing: !is_explore_open.get() && !info_modal_open.get(),
                // 透传引擎初始化 loading:build_engine + warmup + explore_entries 期间(render 源可达数秒)
                // 让列表区显示「加载中...」,而非误显空态「暂无书籍」。
                engine_loading: engine_loading.get(),
            }
        )
        WarningModal(
            // Display(非 Debug):让底层中文错误提示直达用户,而非 Some(Errors(..)) 调试串。
            tip: error.read().as_ref().map(|e| e.to_string()).unwrap_or_default(),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
        ShortcutInfoModal(
            key_shortcut_info: vec![
                ("切换分类弹窗", "Tab"),
                ("上下移动", "J / K / ↑ / ↓"),
                ("选择/进入", "Enter"),
                ("上一页", "H / ←"),
                ("下一页", "L / →"),
                ("搜索书籍", "S"),
            ],
            open: info_modal_open.get(),
        )
        Modal(
            width: Constraint::Length(30),
            height: Constraint::Percentage(70),
            style: Style::default().dim(),
            open: is_explore_open.get(),
            // 非阻塞浮层:关闭键 Tab 由本页 root handler 处理(toggle is_explore_open);弹窗内 Select
            // 在 Modal 子树层。背景 FindBooks 已用 `!is_explore_open` 门控,故非阻塞不引入冲突。
            // 默认 blocks_lower=true 会截断 root → Tab 关不掉抽屉。
            blocks_lower: false,
        ) {
            View(
                margin: Margin::new(1,1),
            ){
                Select<ExploreListItem>(
                    items: explores_list.clone(),
                    state: list_state,
                    on_select: move |item:ExploreListItem| {
                        current_explore.set(Some(item.clone()));
                        is_explore_open.set(false);
                    },
                    top_title: Line::from("选择分类").style(theme.title).centered(),
                    is_editing: is_explore_open.get(),
                    empty_message: "暂无分类".to_string(),
                )
            }
        }
    })
}
