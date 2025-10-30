use crate::{
    History,
    components::{
        Loading, WarningModal, file_select::FileSelect,
        modal::shortcut_info_modal::ShortcutInfoModal, search_input::SearchInput,
    },
    file_list::NovelFiles,
    hooks::UseInitState,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::text::Line;
use ratatui_kit::prelude::*;
use std::{env::current_dir, path::PathBuf};

#[component]
pub fn SelectFile(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let dir_path = hooks.try_use_route_state::<PathBuf>();
    let mut navigate = hooks.use_navigate();
    let is_inputting = *hooks.use_context::<State<bool>>();
    let mut path = hooks.use_state(|| dir_path.map(|p| (*p).clone()));
    let mut info_modal_open = hooks.use_state(|| false);
    let history = *hooks.use_context::<State<Option<History>>>();

    let dir_path = path.read().clone().unwrap_or(current_dir().unwrap());

    let (data, loading, error) = hooks.use_effect_state(
        {
            let path = dir_path.clone();
            async move { tokio::spawn(async move { NovelFiles::from_path(path) }).await? }
        },
        dir_path.clone(),
    );

    let tree_items = data
        .read()
        .clone()
        .map(|i| i.into_tree_item())
        .unwrap_or_default();

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && !is_inputting.get()
        {
            match key.code {
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(!info_modal_open.get());
                }
                _ => {}
            }
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
            FileSelect(
                is_editing: !is_inputting.get() && !info_modal_open.get(),
                top_title: Line::from("本地小说".to_string()).centered(),
                items: tree_items,
                on_select: move |item:PathBuf| {
                    navigate.push_with_state("/local-novel", item);
                },
                empty_message: "未搜索到小说文件，请确认路径是否正确，或按s 开始输入路径",
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
                    ("开始输入路径", "S")
                ],
                open: info_modal_open.get(),
            )
        }
    })
    .into_any()
}
