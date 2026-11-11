mod browser;

use crate::app::navigation::UseAppNavigate;
use crate::components::{ActionBar, ActionButton, Loading};
use crate::{
    History,
    components::{
        modal::shortcut_info_modal::ShortcutInfoModal, search_input::SearchInput, select::Select,
    },
    hooks::UseInitState,
    theme::AppChromeTheme,
};
use browser::{
    BrowserRow, DirListing, RowKind, build_rows, initial_dir, is_supported_file, list_directory,
    parent_dir, quick_dirs, truncate_path_left,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::Constraint,
    text::{Line, Span},
    widgets::ListState,
};
use ratatui_kit::prelude::*;
use std::path::PathBuf;

/// 每个目录独立记住光标,避免 A 目录的选中位置串到 B 目录。
fn cursor_key(dir: &std::path::Path) -> String {
    format!("{}:{}", crate::app::ui_memory::LOCAL_NOVEL, dir.display())
}

#[component]
pub fn SelectFile(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let route_path = hooks
        .try_use_route_state::<PathBuf>()
        .map(|path| (*path).clone());
    let mut navigate = hooks.use_app_navigate();
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let history = *hooks.use_context::<State<Option<History>>>();

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // 路由带的是文件时直接定位到父目录并把该文件选中；目录则从它本身开始。
    let initial_file = route_path
        .as_ref()
        .filter(|path| path.is_file())
        .map(|path| std::path::absolute(path).unwrap_or_else(|_| path.clone()));
    let quick_cwd = cwd.clone();
    // 本次运行里上次停留的目录优先于进程 cwd;路由明确带路径时仍以路由为准。
    let remembered_dir =
        crate::app::ui_memory::remembered_dir(crate::app::ui_memory::LOCAL_NOVEL_DIR);
    let mut current_dir = hooks.use_state({
        let route_path = route_path.clone();
        move || initial_dir(route_path.as_deref(), remembered_dir.unwrap_or(cwd))
    });
    let quick = hooks.use_state(move || quick_dirs(&quick_cwd));
    let mut filter_text = hooks.use_state(String::default);
    let mut info_modal_open = hooks.use_state(|| false);
    let list_state = hooks.use_state(ListState::default);
    let mut pending_file = hooks.use_state(move || initial_file);

    let dir_path = current_dir.read().clone();
    let filter = filter_text.read().clone();
    let quick_snapshot = quick.read().clone();

    // 只读一层目录；大目录的 read_dir 也是同步阻塞 IO，走 spawn_blocking 避免拖住
    // 同 runtime 的 TTS/网络任务。
    let (listing, _loading, error) = hooks.use_effect_state(
        {
            let dir = dir_path.clone();
            async move {
                tokio::task::spawn_blocking(move || {
                    let entries = list_directory(&dir)?;
                    Ok::<_, anyhow::Error>(DirListing {
                        dir: dir.clone(),
                        entries,
                    })
                })
                .await?
            }
        },
        dir_path.clone(),
    );

    let (rows, ready, read_error) = {
        let listing = listing.read();
        match listing.as_ref().filter(|listing| listing.dir == dir_path) {
            Some(listing) => (
                build_rows(&dir_path, &listing.entries, &filter, &quick_snapshot),
                true,
                None,
            ),
            None => (
                Vec::new(),
                false,
                error
                    .read()
                    .as_ref()
                    .map(|error| format!("读取目录失败：{error:#}")),
            ),
        }
    };
    let is_loading = !ready && read_error.is_none();

    // 记住当前目录，返回本地小说页时先回到这里。
    hooks.use_effect(
        {
            let dir_path = dir_path.clone();
            move || {
                crate::app::ui_memory::remember_dir(
                    crate::app::ui_memory::LOCAL_NOVEL_DIR,
                    &dir_path,
                );
            }
        },
        dir_path.clone(),
    );

    // 目录或筛选词变化后重建列表：目录内恢复该目录记住的选中项，筛选时回到首项。
    // 列表为空则清掉 selection，避免重新有内容后 Enter 没反应；越界由 restored_index 钳制。
    hooks.use_effect(
        {
            let rows = rows.clone();
            let dir_path = dir_path.clone();
            let filter_empty = filter.is_empty();
            move || {
                if !ready {
                    return;
                }
                let len = rows.len();
                let index = if filter_empty {
                    crate::app::ui_memory::restored_index(&cursor_key(&dir_path), len, 0)
                } else {
                    0
                };
                list_state.write().select(if len == 0 {
                    None
                } else {
                    Some(index.min(len - 1))
                });
            }
        },
        (dir_path.clone(), filter.clone(), ready),
    );

    // 从首页历史/命令行带文件进来时，等目录读完再定位到该文件。
    hooks.use_effect(
        {
            let rows = rows.clone();
            move || {
                let Some(target) = pending_file.read().clone() else {
                    return;
                };
                if let Some(index) = rows
                    .iter()
                    .position(|row| row.kind == RowKind::File && row.path == target)
                {
                    list_state.write().select(Some(index));
                    pending_file.set(None);
                }
            }
        },
        ready,
    );

    // 光标变化时写回当前目录；筛选态不覆盖目录里记住的真实选择。
    let selected = list_state.read().selected();
    let filter_empty = filter.is_empty();
    hooks.use_effect(
        {
            let dir_path = dir_path.clone();
            move || {
                if filter_empty && let Some(index) = selected {
                    crate::app::ui_memory::save_index(cursor_key(&dir_path), index);
                }
            }
        },
        // 只跟随选中项：目录变化时列表还没恢复，若把 dir_path 也列为依赖，
        // 会把上一目录的索引写到新目录的键上。
        selected,
    );

    // 每个目录只保留本次读取的错误；否则进入可访问目录后状态栏还挂着上一次的
    // 权限错误。
    hooks.use_effect(
        move || {
            error.write().take();
        },
        dir_path.clone(),
    );

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        if !info_modal_open.get() {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Backspace | KeyCode::Left => {
                    // 先把父路径算出来并释放 ReactiveRef，再 set；否则临时借用会活到
                    // if let 块末尾，与 set 的可变借用冲突。
                    let parent = parent_dir(&current_dir.read());
                    if let Some(parent) = parent {
                        current_dir.set(parent);
                        filter_text.set(String::default());
                    }
                    return EventResult::Consumed;
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(!info_modal_open.get());
                    return EventResult::Consumed;
                }
                _ => {}
            }
        }
        EventResult::Ignored
    });

    let (terminal_width, _) = hooks.use_terminal_size();
    let path_width = usize::from(terminal_width.saturating_sub(5)).max(1);
    let dir_label = dir_path.to_string_lossy();
    let path_line = Line::from(vec![
        Span::styled("路径 ", theme.meta_label),
        Span::styled(truncate_path_left(&dir_label, path_width), theme.text),
    ]);

    let status = if let Some(error) = &read_error {
        error.clone()
    } else if is_loading {
        "读取目录中…".to_owned()
    } else {
        format!(
            "{} 项 · ↑/↓ 移动 · Enter 进入/打开 · Backspace/← 上级 · S 手输路径 · / 筛选 · I 帮助",
            rows.len()
        )
    };
    let status_style = if read_error.is_some() {
        theme.error
    } else {
        theme.meta_label
    };

    let empty_message = if read_error.is_some() {
        "无法读取当前目录，请按 Backspace 返回上级".to_owned()
    } else if !filter.is_empty() {
        "没有匹配项，按 Esc 清空筛选".to_owned()
    } else {
        "此目录没有子目录或 .txt 文件，可按 S 手动输入路径".to_owned()
    };

    element!(Fragment {
        View{
            ActionBar(menu: "本地") {
                ActionButton(label: "清除筛选", shortcut: "Esc（输入中）", disabled: filter.is_empty(), on_press: move |_| filter_text.set(String::new()))
                ActionButton(label: "帮助", shortcut: "I", on_press: move |_| info_modal_open.set(true))
            }
            View(height: Constraint::Length(1)) {
                Text(text: path_line, wrap: false)
            }
            SearchInput(
                label: "路径",
                value: dir_path.to_string_lossy().to_string(),
                placeholder: "小说文件或文件夹路径（点击编辑，Enter 打开）",
                is_editing: !info_modal_open.get(),
                validate: |input: String| {
                    let path = PathBuf::from(&input);
                    if !path.exists() {
                        (false, "路径不存在".to_owned())
                    } else if path.is_dir() || is_supported_file(&path) {
                        (true, String::new())
                    } else {
                        (false, "请选择 .txt 文件或文件夹".to_owned())
                    }
                },
                on_submit: move |input: String| {
                    let new_path = PathBuf::from(input);
                    if !new_path.exists() {
                        return false;
                    }
                    let absolute = std::path::absolute(&new_path).unwrap_or_else(|_| new_path.clone());
                    if new_path.is_file() {
                        if let Some(history) = history.write().as_mut() {
                            history.local_path = Some(absolute.clone());
                        }
                        navigate.push_with_state("/local-novel", absolute);
                    } else {
                        current_dir.set(absolute);
                        filter_text.set(String::default());
                    }
                    true
                },
            )
            SearchInput(
                label: "筛选",
                value: filter.clone(),
                placeholder: "筛选当前目录（点击输入，Enter 应用）",
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
            if is_loading {
                Loading(tip: "读取目录中...")
            } else {
                Select::<BrowserRow>(
                    is_editing: !info_modal_open.get(),
                    state: Some(list_state),
                    top_title: Some(Line::from("目录内容").style(theme.title).centered()),
                    items: rows.clone(),
                    on_select: move |row: BrowserRow| {
                        if row.kind == RowKind::File {
                            if let Some(history) = history.write().as_mut() {
                                history.local_path = Some(row.path.clone());
                            }
                            navigate.push_with_state("/local-novel", row.path);
                        } else {
                            current_dir.set(row.path);
                            filter_text.set(String::default());
                        }
                    },
                    empty_message,
                )
            }
            View(height: Constraint::Length(1)) {
                Text(text: status, style: status_style, wrap: false)
            }
            ShortcutInfoModal(
                key_shortcut_info: vec![
                    ("选择下一个", "J / ▼ / ↓"),
                    ("选择上一个", "K / ▲ / ↑"),
                    ("进入目录/打开小说", "Enter"),
                    ("返回上级目录", "Backspace / ←"),
                    ("跳到首项", "Home"),
                    ("跳到末项", "End"),
                    ("手动输入路径", "S"),
                    ("筛选当前目录", "/"),
                    ("关闭帮助", "Esc"),
                ],
                open: info_modal_open.get(),
                on_close:{let mut open = info_modal_open; move |_: ()| open.set(false)},
            )
        }
    })
    .into_any()
}
