use crate::app::navigation::UseAppNavigate;
use crate::{
    History,
    components::{KeyShortcutInfo, ShortcutInfoModal},
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::Constraint,
    text::Line,
    widgets::{List, ListState, Paragraph, Wrap},
};
use ratatui_kit::prelude::*;
use tui_big_text::{BigText, PixelSize};

/// 首页菜单项数,悬停/点击的行→索引映射与列表内容都以此为准。
const MENU_LEN: usize = 4;

/// 菜单导航:Enter 键与鼠标点击共用同一份目标路由。
fn navigate_menu(
    mut navigate: crate::app::navigation::AppNavigate,
    local_path: Option<std::path::PathBuf>,
    index: usize,
) {
    match index {
        0 => {
            if let Some(path) = &local_path {
                navigate.push_with_state("/select-file", path.clone());
            } else {
                navigate.push("/select-file");
            }
        }
        1 => navigate.push("/book-source"),
        2 => navigate.push("/select-history"),
        3 => navigate.push("/theme-setting"),
        _ => {}
    }
}

#[derive(Default, Props)]
struct HomeMenuProps {
    state: Option<State<ListState>>,
    /// 帮助浮层等独占态时为 false:鼠标事件放行(与键盘 j/k/Enter 的门控一致)。
    is_editing: bool,
    on_navigate: Handler<'static, usize>,
}

/// 首页菜单:鼠标悬停移动高亮、滚轮移动选中、点击直接导航。
/// 悬停只动高亮不导航 —— 路由跳转只由点击/Enter 触发,避免鼠标扫过菜单就误入子页。
#[component]
fn HomeMenu(props: &mut HomeMenuProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let local = hooks.use_state(|| ListState::default().with_selected(Some(0)));
    let state = props.state.unwrap_or(local);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let props_is_editing = props.is_editing;
    let mut on_navigate = props.on_navigate.take();

    let list = List::new(vec![
        Line::from("本地小说").centered(),
        Line::from("网络小说").centered(),
        Line::from("历史记录").centered(),
        Line::from("全局设置").centered(),
    ])
    .style(theme.text)
    .highlight_style(theme.selected);

    hooks.use_pointer(
        PointerOptions {
            track_move: true,
            ..Default::default()
        },
        move |ev| {
            if !props_is_editing {
                return EventResult::Ignored;
            }
            let row = ev.position.y.saturating_sub(ev.area.y) as usize;
            match ev.kind {
                PointerKind::Move | PointerKind::Enter if row < MENU_LEN => {
                    state.write().select(Some(row));
                    EventResult::Consumed
                }
                PointerKind::ScrollUp => {
                    state.write().select_previous();
                    EventResult::Consumed
                }
                PointerKind::ScrollDown => {
                    state.write().select_next();
                    EventResult::Consumed
                }
                PointerKind::Click { .. } if row < MENU_LEN => {
                    on_navigate(row);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        },
    );

    element!(View(height: Constraint::Length(MENU_LEN as u16)) {
        stateful(list, state)
    })
}

#[component]
pub fn Home(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let state = hooks.use_state(|| ListState::default().with_selected(Some(0)));
    let mut info_modal_open = hooks.use_state(|| false);
    let history = hooks.use_context::<State<Option<History>>>();
    let local_path = history.read().as_ref().and_then(|h| h.local_path.clone());

    let theme = hooks.use_component_theme::<AppChromeTheme>();

    let navigate = hooks.use_app_navigate();
    let navigate_for_keys = navigate;
    let local_path_for_keys = local_path.clone();

    hooks.use_terminal_size();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        if info_modal_open.get() {
            match key.code {
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(false);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        } else {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    state.write().select_next();
                    EventResult::Consumed
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    state.write().select_previous();
                    EventResult::Consumed
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(true);
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    if let Some(index) = state.read().selected() {
                        navigate_menu(navigate_for_keys, local_path_for_keys.clone(), index);
                    }
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });

    let big_txt = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .lines(vec!["TRNovel".into()])
        .centered()
        .style(theme.logo)
        .build();

    let info_txt = Paragraph::new(vec!["终端小说阅读器 (Terminal reader for novel)".into()])
        .wrap(Wrap { trim: true })
        .style(theme.text)
        .centered();

    element!(
        Center{
            View(
                height:Constraint::Length(5)
            ){
                widget(big_txt)
            }
            View(
                height:Constraint::Length(2)
            ){
                widget(info_txt)
            }
            crate::components::ActionBar(menu: "更多") {
                crate::components::ActionButton(label: "帮助", shortcut: "I", on_press: move |_| info_modal_open.set(true))
            }
            HomeMenu(
                state: state,
                is_editing: !info_modal_open.get(),
                on_navigate: {
                    let local_path = local_path.clone();
                    move |index: usize| navigate_menu(navigate, local_path.clone(), index)
                },
            )
            ShortcutInfoModal(
                key_shortcut_info:KeyShortcutInfo::new(vec![
                    ("选择下一个", "J / ▼"),
                    ("选择上一个", "K / ▲"),
                    ("确认选择", "Enter / 鼠标点击"),
                ]),
                open:info_modal_open.get(),
                on_close:move |_: ()| info_modal_open.set(false),
            )
        }
    )
}
