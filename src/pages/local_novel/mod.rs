use crate::{
    History,
    components::{
        Loading, WarningModal, file_select::FileSelect,
        modal::shortcut_info_modal::ShortcutInfoModal, search_input::SearchInput,
    },
    file_list::NovelFiles,
    hooks::UseInitState,
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::text::Line;
use ratatui_kit::prelude::*;
use std::{env::current_dir, path::PathBuf};

#[component]
pub fn SelectFile(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let dir_path = hooks.try_use_route_state::<PathBuf>();
    let mut navigate = hooks.use_navigate();
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut path = hooks.use_state(|| dir_path.map(|p| (*p).clone()));
    let mut filter_text = hooks.use_state(String::default);
    let mut info_modal_open = hooks.use_state(|| false);
    let history = *hooks.use_context::<State<Option<History>>>();

    let dir_path = path.read().clone().unwrap_or(current_dir().unwrap());
    let filter = filter_text.read().clone();

    let (data, loading, error) = hooks.use_effect_state(
        {
            let path = dir_path.clone();
            let filter = filter.clone();
            // walkdir 递归是同步阻塞 IO,必须走 spawn_blocking:用 tokio::spawn
            // 会把它压在 async worker 上,大目录扫描期间同 runtime 的网络书源
            // 请求和 TTS 下载都会被一起拖慢。
            async move {
                tokio::task::spawn_blocking(move || {
                    NovelFiles::from_path_with_filter(path, Some(filter))
                })
                .await?
            }
        },
        (dir_path.clone(), filter.clone()),
    );

    let tree_items = data
        .read()
        .clone()
        .map(|i| i.into_tree_item())
        .unwrap_or_default();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('i') | KeyCode::Char('I') => {
                info_modal_open.set(!info_modal_open.get());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    if loading.get() {
        return element!(Loading(tip:"搜索小说中...")).into_any();
    }

    element!(Fragment {
        View{
            SearchInput(
                value: dir_path.to_string_lossy().to_string(),
                placeholder: "按s 开始输入小说文件夹路径",
                is_editing: !info_modal_open.get(),
                validate: |input: String| {
                    let path = PathBuf::from(input);
                    if path.exists() {
                        if path.is_file() && path.extension().unwrap_or_default() != "txt" {
                            (false, "文件格式不正确".to_owned())
                        } else {
                            (true, "".to_owned())
                        }
                    } else {
                        (false, "路径不存在".to_owned())
                    }
                },
                on_submit: move |input: String| {
                    let new_path = PathBuf::from(input);
                    if new_path.exists() {
                        path.set(Some(new_path.clone()));
                        // 更新历史记录中的本地路径
                        if let Some(h) = history.write().as_mut() { h.local_path=new_path.canonicalize().ok(); }
                        true
                    } else {
                        false
                    }
                },
            )
            SearchInput(
                value: filter.clone(),
                placeholder: "按/搜索小说名称",
                activate_key: Some(KeyCode::Char('/')),
                is_editing: !info_modal_open.get(),
                clear_on_escape: true,
                on_submit: move |input: String| {
                    filter_text.set(input);
                    true
                },
                on_clear: move |_| {
                    filter_text.set(String::default());
                },
            )
            FileSelect(
                is_editing: !info_modal_open.get(),
                // 筛选态展开全部目录:命中的文件多半在子目录里,折叠着等于没筛。
                expand_all: !filter.is_empty(),
                top_title: Line::from("本地小说".to_string()).style(theme.title).centered(),
                items: tree_items,
                on_select: move |item:PathBuf| {
                    navigate.push_with_state("/local-novel", item);
                },
                empty_message: if filter.is_empty() {
                    "未搜索到小说文件，请确认路径是否正确，或按s 开始输入路径".to_owned()
                } else {
                    "没有匹配的小说，请按/重新搜索，或按Esc清除搜索".to_owned()
                },
            )
            WarningModal(
                tip: format!("加载失败:{:?}", error.read().as_ref()),
                is_error: error.read().is_some(),
                open: error.read().is_some(),
            )
            ShortcutInfoModal(
                key_shortcut_info: vec![
                    ("展开文件夹", "L / ► / Enter"),
                    ("折叠文件夹", "H / ◄"),
                    ("选择下一个", "J / ▼"),
                    ("选择上一个", "K / ▲"),
                    ("选择小说文件", "Enter"),
                    ("开始输入路径", "S"),
                    ("搜索小说名称", "/")
                ],
                open: info_modal_open.get(),
            )
        }
    })
    .into_any()
}
