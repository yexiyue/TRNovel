use crate::app::navigation::UseAppNavigate;
#[cfg(feature = "network")]
use crate::components::BrowserPromptModal;
#[cfg(feature = "network")]
use crate::pages::network_novel::book_detail::BookDetailState;
use crate::{AppearanceConfig, Commands, History, HistoryItem, TRNovel};

/// 浏览器授权弹窗只在网络构建里存在;无网络构建渲染零高度的占位元素,
/// 让两种构建共用同一棵元素树(元素树里只能写组件名,不能写条件表达式)。
#[cfg(feature = "network")]
#[component]
fn BrowserPromptSlot(_hooks: Hooks) -> impl Into<AnyElement<'static>> {
    element!(BrowserPromptModal)
}

#[cfg(not(feature = "network"))]
#[component]
fn BrowserPromptSlot(_hooks: Hooks) -> impl Into<AnyElement<'static>> {
    element!(View(height: ratatui::layout::Constraint::Length(0)))
}
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui_kit::prelude::*;
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
    // 确认弹窗点「退出」时真正执行的句柄;use_exit 不实现 Copy,所以只能再造一个。
    let mut exit_confirmed = hooks.use_exit();
    let mut confirm_exit_open = hooks.use_state(|| false);
    let mut appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let show_header = appearance.read().show_header;
    let (terminal_width, _) = hooks.use_terminal_size();
    let chrome_theme = hooks.use_component_theme::<crate::theme::AppChromeTheme>();
    let display = *hooks.use_atom(&crate::state::READER_DISPLAY).read();
    // 阅读页自己画提示条(它还要按开关扣正文行数),这里只负责其余页面,避免同屏两条。
    let reading_page = {
        let trail = trail.read();
        matches!(trail.current_path(), "/local-novel" | "/network-novel")
    };
    let footer_rows = u16::from(display.show_hint_bar && !reading_page);
    let footer_hint = if footer_rows > 0 {
        crate::keymap::other_hint_bar(terminal_width.saturating_sub(2) as usize)
    } else {
        String::new()
    };
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
                    #[cfg(feature = "network")]
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
                                #[cfg(feature = "network")]
                                HistoryItem::Network(_) => {
                                    navigate.push_with_state(
                                        "/book-detail",
                                        BookDetailState::from_cache(path.clone()),
                                    );
                                }
                                // 无网络构建:最近一条是网络小说时没有可续读的页面,保持停在首页。
                                #[cfg(not(feature = "network"))]
                                HistoryItem::Network(_) => {}
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
            // Esc/Backspace 统一兜底为「返回上一页」,与顶栏返回、b/B 共用 navigate.go(-1)。
            // 本 handler 在 root 层且优先级 Low:页面 Normal handler 先处理;搜索框、Modal、
            // 主题编辑器等 blocks_lower 输入层会直接把本层截断,故不会抢走既有 Esc 语义。
            KeyCode::Esc | KeyCode::Backspace => {
                // 先释放 trail 读锁再 go:go 会写同一个 Atom,读锁未放会死锁。
                let can_back = {
                    let trail = trail.read();
                    trail.can_go_back() && trail.current_path() != "/home"
                };
                if can_back {
                    navigate.go(-1);
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                request_exit(&appearance.read(), confirm_exit_open, &mut exit);
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
            crate::components::ActionButton(label: "全局设置", on_press: move |_| navigate.push("/settings"))
            crate::components::ActionButton(label: if show_header { "隐藏顶部栏" } else { "显示顶部栏" }, shortcut: "F10 呼出", on_press: move |_| {
                crate::components::action_bar::toggle_header(appearance);
            })
            crate::components::ActionButton(
                label: forward.map(|name| format!("前进至{name} ›")).unwrap_or_else(|| "前进 ›".into()), shortcut: "G",
                disabled: forward.is_none(), on_press: move |_| navigate.go(1),
            )
            crate::components::ActionButton(label: "退出", shortcut: "Q", on_press: move |_| {
                request_exit(&appearance.read(), confirm_exit_open, &mut exit_button);
            })
        }
        Outlet
        // 常显快捷键提示条固定在整屏最后一行;阅读页自带,故此时高度为 0。
        View(height: ratatui::layout::Constraint::Length(footer_rows), margin: ratatui::layout::Margin::new(1, 0)) {
            widget(ratatui::text::Line::from(footer_hint).style(chrome_theme.meta_label).centered())
        }
        crate::components::search_input::HeaderSearch(
            chrome_layer: Some(chrome_layer),
            back_shortcut: back.map(|name| format!("B · 返回{name}")).unwrap_or_else(|| "B".into()),
            back_disabled: back.is_none(), on_back: move |_| navigate.go(-1),
        )
        crate::components::action_menu::AppMenuPopup
        crate::components::ActionHint
        BrowserPromptSlot
        // 放在元素树最后:遮罩和面板盖在所有页面内容之上,鼠标命中顺序也一致。
        crate::components::ConfirmExitModal(
            open: confirm_exit_open.get(),
            on_confirm: move |no_remind: bool| {
                if no_remind {
                    let mut next = appearance.read().clone();
                    next.confirm_exit = false;
                    let _ = next.save();
                    appearance.set(next);
                }
                confirm_exit_open.set(false);
                exit_confirmed();
            },
            on_cancel: move |_| confirm_exit_open.set(false),
        )
    })
}

/// 退出是否要先弹确认框。抽成纯函数,便于不启动 TUI 也能覆盖两种偏好。
fn should_confirm_exit(appearance: &AppearanceConfig) -> bool {
    appearance.confirm_exit
}

/// Q 键与「退出」按钮共用的入口:需要确认时只把弹窗打开,真正退出交给确认回调。
/// 这样两条路径的判定不会各写一份、日后跑偏。
fn request_exit(appearance: &AppearanceConfig, mut open: State<bool>, exit: &mut impl FnMut()) {
    if should_confirm_exit(appearance) {
        open.set(true);
    } else {
        exit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_exit_follows_persisted_preference() {
        let defaults = AppearanceConfig::default();
        assert!(should_confirm_exit(&defaults), "默认必须先确认再退出");

        let mut disabled = defaults.clone();
        disabled.confirm_exit = false;
        assert!(!should_confirm_exit(&disabled), "关掉开关后应直接退出");
    }
}
