use crate::{
    History,
    components::{KeyShortcutInfo, ShortcutInfoModal},
    hooks::UseThemeConfig,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::Constraint,
    text::Line,
    widgets::{List, ListState, Paragraph, Wrap},
};
use ratatui_kit::prelude::*;
use tui_big_text::{BigText, PixelSize};

#[component]
pub fn Home(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let state = hooks.use_state(ListState::default);
    let mut info_modal_open = hooks.use_state(|| false);
    let history = hooks.use_context::<State<Option<History>>>();
    let local_path = history.read().as_ref().and_then(|h| h.local_path.clone());

    let theme = hooks.use_theme_config();

    let mut navigate = hooks.use_navigate();

    hooks.use_terminal_size();

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            if info_modal_open.get() {
                match key.code {
                    KeyCode::Char('i') | KeyCode::Char('I') => {
                        info_modal_open.set(false);
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        state.write().select_next();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        state.write().select_previous();
                    }
                    KeyCode::Char('i') | KeyCode::Char('I') => {
                        info_modal_open.set(true);
                    }
                    KeyCode::Enter => {
                        if let Some(index) = state.read().selected() {
                            match index {
                                0 => {
                                    if let Some(path) = &local_path {
                                        navigate.push_with_state("/select-file", path.clone());
                                    } else {
                                        navigate.push("/select-file");
                                    }
                                }
                                1 => {
                                    // navigate.push(network_novel_first_page().unwrap());
                                }
                                2 => {
                                    // navigate.push(SelectHistory::to_page_route());
                                    navigate.push("/select-history");
                                }
                                3 => {
                                    // navigate.push(ThemeSettingPage::to_page_route());
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
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
        .style(theme.basic.text)
        .centered();

    let list = List::new(vec![
        Line::from("本地小说").centered(),
        Line::from("网络小说").centered(),
        Line::from("历史记录").centered(),
        Line::from("主题设置").centered(),
    ])
    .style(theme.basic.text)
    .highlight_style(theme.selected);

    element!(
        Center{
            View(
                height:Constraint::Length(5)
            ){
                $big_txt
            }
            View(
                height:Constraint::Length(2)
            ){
                $info_txt
            }
            View(height:Constraint::Length(4)){
                $(list,state)
            }
            ShortcutInfoModal(
                key_shortcut_info:KeyShortcutInfo::new(vec![
                    ("选择下一个", "J / ▼"),
                    ("选择上一个", "K / ▲"),
                    ("确认选择", "Enter"),
                ]),
                open:info_modal_open.get(),
            )
        }
    )
}
