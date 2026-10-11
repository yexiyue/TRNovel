use crate::app::navigation::UseAppNavigate;
use crate::{
    Commands, History, HistoryItem, TRNovel, components::BrowserPromptModal,
    pages::network_novel::book_detail::BookDetailState,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui_kit::{
    AnyElement, EventPriority, EventResult, EventScope, Hooks, State, UseAtom, UseContext,
    UseEffect, UseEventHandler, UseExit, UseInputLayer, UseRouter, UseState, component, element,
    prelude::{Outlet, View},
};
use std::path::PathBuf;

#[component]
pub fn Layout(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut navigate = hooks.use_app_navigate();
    let trail = crate::app::navigation::use_trail(&mut hooks);
    let back = trail.read().back_label();
    let forward = trail.read().forward_label();
    let mut exit = hooks.use_exit();
    let mut exit_button = hooks.use_exit();
    let appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let show_header = appearance.read().show_header;
    let chrome_layer = hooks.use_input_layer(true, false);
    let params = hooks.try_use_route_state::<TRNovel>();
    let history = *hooks.use_context::<State<Option<History>>>();

    // CLI 子命令导航**只做一次**。deps 是 `params`,而 `/home` 的 route state 就是 `TRNovel`
    // 本身,故按 `b` 从目标页退回 `/home` 会让 params 再次变化、effect 重跑并把用户**弹回原页**,
    // 表现为「-n/-H/-l 启动时后退键无效」。用一次性标志挡住重复导航。
    let mut did_initial_nav = hooks.use_state(|| false);

    hooks.use_effect(
        || {
            if did_initial_nav.get() {
                return;
            }
            if let Some(params) = params.clone() {
                did_initial_nav.set(true);
                match params.subcommand.clone() {
                    Some(Commands::Network) => {
                        navigate.push("/book-source");
                    }
                    Some(Commands::History) => {
                        navigate.push("/select-history");
                    }
                    Some(Commands::Local { path }) => {
                        if let Some(path) = path {
                            navigate.push_with_state("/select-file", path);
                        } else {
                            navigate.push("/select-file");
                        }
                    }
                    Some(Commands::Quick) => {
                        if let Some(history) = history.read().as_ref()
                            && let Some((path, item)) = history.histories.first()
                        {
                            match item {
                                HistoryItem::Local(_) => {
                                    navigate.push_with_state("/local-novel", PathBuf::from(path));
                                }
                                HistoryItem::Network(_) => {
                                    navigate.push_with_state(
                                        "/book-detail",
                                        BookDetailState::from_cache(path.clone()),
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                };
            }
        },
        params.clone(),
    );

    // 背景 shell 键:注册在 root 层(Current)。当页面的搜索框/模态开启 blocks_lower 输入层时,
    // 本 handler 随之被框架自动截断 —— 这正是旧 `is_inputting` 手动门控的事,现由输入层零竞态完成。
    // 不设 Global:否则会在文本输入时劫持 q/g/b。非自身键返回 Ignored,让 Outlet 子页面继续处理。
    // 优先级用 Low:同层内按 priority 降序分发(High→Normal→Low)且 Consumed 早停,故页面级
    // Normal handler 先跑,shell 键只做兜底。**代价**:页面一旦占用同一个键并 Consumed,该 shell 键
    // 在那个页面就彻底失效(书源页的 `b` 曾这样吃掉后退键,现已改用 `w`)。
    // 因此新增页面快捷键时务必避开 q/g/b。
    hooks.use_event_handler(EventScope::Current, EventPriority::Low, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        // shell 键只认**裸键**:`key.code` 不带修饰信息,只 match 它会让 Ctrl+B 被当成 b
        // (退回上一页)、Ctrl+Q 被当成 q(退出程序)——阅读页把 Ctrl+B/Ctrl+F 绑成翻页后
        // 这条会天天踩到。SHIFT 必须放行:大写字母(Q/G/B)本身就带它。
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                exit();
                EventResult::Consumed
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                navigate.go(1);
                EventResult::Consumed
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                navigate.go(-1);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });
    element!(View {
        View(height: ratatui::layout::Constraint::Length(u16::from(show_header)))
        crate::components::ActionBar(menu: "更多") {
            crate::components::ActionButton(label: "全局设置", on_press: move |_| navigate.push("/theme-setting"))
            crate::components::ActionButton(label: if show_header { "隐藏顶部栏" } else { "显示顶部栏" }, shortcut: "F10 呼出", on_press: move |_| {
                crate::components::action_bar::toggle_header(appearance);
            })
            crate::components::ActionButton(
                label: forward.map(|name| format!("前进至{name} ›")).unwrap_or_else(|| "前进 ›".into()), shortcut: "G",
                disabled: forward.is_none(), on_press: move |_| navigate.go(1),
            )
            crate::components::ActionButton(label: "退出", shortcut: "Q", on_press: move |_| exit_button())
        }
        Outlet
        crate::components::search_input::HeaderSearch(
            chrome_layer: Some(chrome_layer),
            back_shortcut: back.map(|name| format!("B · 返回{name}")).unwrap_or_else(|| "B".into()),
            back_disabled: back.is_none(), on_back: move |_| navigate.go(-1),
        )
        crate::components::action_menu::AppMenuPopup
        crate::components::ActionHint
        BrowserPromptModal
    })
}
