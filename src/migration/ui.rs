//! 迁移交互:进程级会话 Atom + 首启门控 + 三态视图(询问/进度/汇总)。
//!
//! [`MigrationModal`] 挂在应用根上(启动门控期间覆盖在加载屏上),
//! [`MigrationRoot`] 是 `trn migrate` 的独立全屏根,两者复用 [`MigrationView`]。
//! 引擎在阻塞线程运行,进度快照写 [`MIGRATION`] Atom,Esc 置共享取消标志。

use super::{Event, Outcome, Status};
use crate::{paths::AppPaths, theme::AppChromeTheme};
use crossterm::event::{Event as TermEvent, KeyCode, KeyEventKind, MouseButton};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Flex, Margin},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Gauge,
};
use ratatui_kit::{
    AnyElement, Atom, EventPriority, EventResult, EventScope, Handler, Hooks, PointerKind,
    PointerOptions, Props, UseAtom, UseEventHandler, UseExit, UseFuture, UseHover, UseInputLayer,
    UsePointer, UseTheme, component, element,
    prelude::{Border, ButtonVariant, Center, Modal, PaletteProvider, Text, View},
};
use std::io::{self, Write};
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

/// 启动门控的「用户已决定」信号:迁移结束(含取消/失败)或跳过时通知。
static DECIDED: LazyLock<Notify> = LazyLock::new(Notify::new);

/// 进程级迁移会话;`Idle` 之外的状态都会以独占弹窗/全屏呈现。
pub static MIGRATION: Atom<Session> = Atom::new(Session::default);

/// 迁移进度快照(引擎事件折叠而来;kept/skipped 只计数,不逐文件输出)。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub items: usize,
    pub items_done: usize,
    pub current_item: String,
    pub files_total: u64,
    pub files_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub copied: u64,
    pub kept: u64,
    pub skipped: u64,
}

/// 迁移会话状态机。
#[derive(Clone, Debug, Default)]
pub enum Session {
    #[default]
    Idle,
    /// 检测到待迁移数据,等待用户决定(规模来自预扫描)。
    Ask {
        items: usize,
        files: u64,
        bytes: u64,
        start_selected: bool,
    },
    /// 迁移进行中;`cancel` 供 Esc 协作式取消。
    Running {
        progress: Progress,
        cancel: Arc<AtomicBool>,
    },
    /// 迁移结束(完成/取消/失败),等待用户关闭。
    Finished {
        outcome: Outcome,
        progress: Progress,
    },
}

/// 启动门控:Pending 时预扫描规模并置 Ask,等用户决定(迁移结束或跳过)后返回,
/// 保证「迁移后才加载缓存」;其余状态直接返回,任何检查失败都不阻断启动。
pub async fn gate_and_wait() {
    let pending = tokio::task::spawn_blocking(|| -> std::io::Result<Option<(usize, u64, u64)>> {
        let paths = AppPaths::user_default()?;
        if super::status_at(&paths) == Status::Pending {
            let (items, files, bytes) = super::pending_survey(&paths);
            Ok(Some((items, files, bytes)))
        } else {
            Ok(None)
        }
    })
    .await;
    match pending {
        Ok(Ok(Some((items, files, bytes)))) => {
            MIGRATION.set(Session::Ask {
                items,
                files,
                bytes,
                start_selected: true,
            });
            DECIDED.notified().await;
        }
        Ok(Ok(None)) => {}
        Ok(Err(error)) => eprintln!("旧目录迁移检查失败：{error:#}；继续启动"),
        Err(error) => eprintln!("旧目录迁移检查未完成：{error}；继续启动"),
    }
}

/// 后台执行迁移:进度写 [`MIGRATION`],结束置 Finished;`notify` 时唤醒启动门控。
pub fn start(notify: bool) {
    let cancel = Arc::new(AtomicBool::new(false));
    MIGRATION.set(Session::Running {
        progress: Progress::default(),
        cancel: cancel.clone(),
    });
    tokio::task::spawn_blocking(move || {
        let shared = Arc::new(Mutex::new(Progress::default()));
        let snapshot = shared.clone();
        let cancel_for_view = cancel.clone();
        let mut emit = move |event: Event| {
            let mut progress = snapshot.lock().unwrap();
            match event {
                Event::Scan {
                    items,
                    files,
                    bytes,
                } => {
                    progress.items = items;
                    progress.files_total = files;
                    progress.bytes_total = bytes;
                }
                Event::Item { name } => {
                    progress.items_done += 1;
                    progress.current_item = name.to_string();
                }
                Event::Copied { bytes } => {
                    progress.files_done += 1;
                    progress.bytes_done += bytes;
                    progress.copied += 1;
                }
                Event::Kept => {
                    progress.files_done += 1;
                    progress.kept += 1;
                }
                Event::Skipped => progress.skipped += 1,
            }
            MIGRATION.set(Session::Running {
                progress: progress.clone(),
                cancel: cancel_for_view.clone(),
            });
        };
        let outcome = match AppPaths::user_default() {
            Ok(paths) => super::run(&paths, cancel.as_ref(), &mut emit),
            Err(error) => Outcome::Failed {
                fatal: Some(error.to_string()),
                errors: Vec::new(),
            },
        };
        let progress = shared.lock().unwrap().clone();
        MIGRATION.set(Session::Finished { outcome, progress });
        if notify {
            DECIDED.notify_one();
        }
    });
}

/// 记住「跳过」决定:置 Idle、唤醒门控,并异步落盘拒绝标记(失败仅提示,下次重新询问)。
pub fn decline() {
    MIGRATION.set(Session::Idle);
    DECIDED.notify_one();
    tokio::task::spawn_blocking(|| {
        if let Ok(paths) = AppPaths::user_default()
            && let Err(error) = super::publish_declined(&paths)
        {
            eprintln!("记录迁移跳过决定失败：{error:#}");
        }
    });
}

/// 无头迁移(`trn migrate --yes`):直接执行并打印摘要,返回进程退出码。
pub fn run_headless() -> i32 {
    let Ok(paths) = AppPaths::user_default() else {
        eprintln!("无法获取用户主目录");
        return 1;
    };
    match super::status_at(&paths) {
        Status::Done | Status::NotNeeded => {
            println!("无需迁移：没有待迁移的旧数据");
            0
        }
        // 显式命令无视拒绝标记:Declined 只是「不再自动询问」,数据仍在。
        Status::Declined | Status::Pending => {
            let cancel = AtomicBool::new(false);
            let mut copied = 0;
            let mut kept = 0;
            let mut skipped = 0;
            let mut files_done = 0;
            let mut files_total = 0;
            let mut bytes_done = 0;
            let mut bytes_total = 0;
            let mut current_item = String::new();
            let mut emit = |event: Event| {
                match event {
                    Event::Scan {
                        items,
                        files,
                        bytes,
                    } => {
                        files_total = files;
                        bytes_total = bytes;
                        println!(
                            "待迁移：{items} 个条目 · {files} 个文件 · {}",
                            human_bytes(bytes)
                        );
                    }
                    Event::Item { name } => {
                        current_item = name.to_string();
                        println!("→ {name}");
                    }
                    Event::Copied { bytes } => {
                        copied += 1;
                        files_done += 1;
                        bytes_done += bytes;
                    }
                    Event::Kept => {
                        kept += 1;
                        files_done += 1;
                    }
                    Event::Skipped => skipped += 1,
                }
                print!(
                    "\r  {current_item}: {files_done}/{files_total} 个文件 · {} / {} · 拷贝 {copied} · 保留 {kept} · 跳过 {skipped}   ",
                    human_bytes(bytes_done),
                    human_bytes(bytes_total),
                );
                let _ = io::stdout().flush();
            };
            match super::run(&paths, &cancel, &mut emit) {
                Outcome::Done => {
                    println!();
                    println!("迁移完成：拷贝 {copied} · 保留已有 {kept} · 跳过 {skipped}");
                    println!("源目录 ~/.novel 保留未动；完成标记已写入。");
                    0
                }
                Outcome::Cancelled => {
                    println!();
                    println!("迁移取消：已拷贝 {copied} 个文件保留，下次可继续");
                    1
                }
                Outcome::Failed { fatal, errors } => {
                    println!();
                    if let Some(fatal) = fatal {
                        eprintln!("迁移失败：{fatal}");
                    }
                    for error in &errors {
                        eprintln!("条目失败：{error}");
                    }
                    eprintln!("源文件保留，下次启动或重试时继续。");
                    1
                }
            }
        }
    }
}

// ───────────────────────── 视图(询问 / 进度 / 汇总) ─────────────────────────

/// 会话对应的内容高度(弹窗与独立根共用,保证布局不裁切;按钮行 3 行已计入,
/// 内容列不用 gap,按钮前用 1 行显式间隔——间距可精确计入高度)。
fn session_height(session: &Session) -> u16 {
    match session {
        Session::Idle => 5,
        Session::Ask { .. } => 12,
        Session::Running { .. } => 12,
        Session::Finished { outcome, .. } => match outcome {
            Outcome::Done => 11,
            Outcome::Cancelled => 10,
            Outcome::Failed { fatal, errors } => {
                let problems = errors.len() + usize::from(fatal.is_some());
                11 + problems.min(4) as u16 + u16::from(errors.len() > 4)
            }
        },
    }
}

#[derive(Props, Default)]
struct MigrationButtonProps {
    label: String,
    variant: ButtonVariant,
    selected: bool,
    on_press: Handler<'static, ()>,
}

/// Migration actions use the same outlined, hover-selected treatment as ConfirmModal.
#[component]
fn MigrationButton(
    props: &mut MigrationButtonProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let hovered = hooks.use_hover();
    let mut on_press = props.on_press.take();
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if matches!(event.kind, PointerKind::Down(MouseButton::Left)) {
            on_press(());
            EventResult::Consumed
        } else {
            EventResult::Ignored
        }
    });

    let base_style = if props.variant == ButtonVariant::Danger {
        theme.error
    } else {
        theme.text
    };
    let selected = props.selected || hovered;
    let style = if selected {
        theme.highlight
    } else {
        base_style
    };
    let label = props.label.clone();
    let width = Line::from(label.as_str()).width() as u16 + 6;
    element!(Border(
        width: Constraint::Length(width),
        height: Constraint::Length(3),
        border_style: style,
    ) {
        Text(
            text: if selected { format!(" {label} ") } else { label },
            style: if selected { style.add_modifier(Modifier::BOLD) } else { style },
            alignment: Alignment::Center,
        )
    })
}

fn human_bytes(bytes: u64) -> String {
    for (threshold, suffix) in [(1u64 << 30, "GB"), (1 << 20, "MB"), (1 << 10, "KB")] {
        if bytes >= threshold {
            return format!("{:.1} {suffix}", bytes as f64 / threshold as f64);
        }
    }
    format!("{bytes} B")
}

/// 供 [Gauge] 的 label 用的百分比;0/0 时保持 0。
fn percent_of(done: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        ((done as f64 / total as f64) * 100.0).round() as u64
    }
}

#[derive(Props, Default)]
pub struct MigrationViewProps {
    pub session: Session,
    /// 结束态按钮文案(应用内「继续」/独立根「退出」)。
    pub dismiss_label: String,
    /// Ask:开始迁移(Enter/Y 或点击主按钮)。
    pub on_confirm: Handler<'static, ()>,
    /// Ask:跳过(N/Esc 或点击次按钮)。
    pub on_decline: Handler<'static, ()>,
    /// Running:取消(Esc 或点击危险按钮)。
    pub on_cancel: Handler<'static, ()>,
    /// Finished:关闭汇总(Enter/Esc 或点击按钮)。
    pub on_dismiss: Handler<'static, ()>,
}

/// 三态迁移视图;键盘由宿主接线,按钮点击由本视图处理。
#[component]
pub fn MigrationView(
    props: &mut MigrationViewProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let muted = theme.muted;
    // 先克隆出 owned 快照:element! 要求 'static,不能让 props 的借用逃逸。
    let session = props.session.clone();

    // 进度条 widget 与按钮行按状态装配;行文本保持 owned。
    /// 状态装配产物:标题、正文行、进度条与按钮行。
    type Layout = (
        String,
        Vec<Line<'static>>,
        Option<Gauge<'static>>,
        Vec<(String, ButtonVariant, bool, Handler<'static, ()>)>,
    );
    let (title, lines, gauge, buttons): Layout = match &session {
        Session::Idle => (
            "数据迁移".to_string(),
            vec![Line::styled("检查迁移状态…", muted)],
            None,
            Vec::new(),
        ),
        Session::Ask {
            items,
            files,
            bytes,
            start_selected,
        } => (
            "旧数据迁移".to_string(),
            vec![
                Line::styled(
                    "检测到旧版阅读数据 ~/.novel，可迁移到 ~/.trnovel。",
                    theme.text,
                ),
                Line::from(vec![
                    Span::styled("规模：", muted),
                    Span::styled(
                        format!(
                            "{items} 个条目 · {files} 个文件 · 约 {}",
                            human_bytes(*bytes)
                        ),
                        theme.highlight,
                    ),
                ]),
                Line::styled("源目录保留不动，可随时重试；已有新文件一律优先。", muted),
                Line::styled("跳过后仍可运行 trn migrate 手动迁移。", muted),
            ],
            None,
            vec![
                (
                    "开始迁移".to_string(),
                    ButtonVariant::Primary,
                    *start_selected,
                    props.on_confirm.take(),
                ),
                (
                    "跳过".to_string(),
                    ButtonVariant::Secondary,
                    !start_selected,
                    props.on_decline.take(),
                ),
            ],
        ),
        Session::Running { progress, .. } => {
            let ratio = if progress.bytes_total == 0 {
                0.0
            } else {
                progress.bytes_done as f64 / progress.bytes_total as f64
            };
            let current = if progress.current_item.is_empty() {
                "扫描中…".to_string()
            } else {
                progress.current_item.clone()
            };
            (
                "迁移中".to_string(),
                vec![
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "条目 {}/{}　",
                                progress.items_done.max(1),
                                progress.items.max(1)
                            ),
                            muted,
                        ),
                        Span::styled(current, theme.highlight),
                    ]),
                    Line::styled(
                        format!(
                            "{} / {} 个文件 · {} / {}",
                            progress.files_done,
                            progress.files_total,
                            human_bytes(progress.bytes_done),
                            human_bytes(progress.bytes_total),
                        ),
                        theme.text,
                    ),
                    Line::styled(
                        format!(
                            "已拷贝 {} · 保留已有 {} · 跳过 {}",
                            progress.copied, progress.kept, progress.skipped
                        ),
                        muted,
                    ),
                ],
                Some(
                    Gauge::default()
                        .ratio(ratio)
                        .gauge_style(theme.loading)
                        .label(format!(
                            " {}% ",
                            percent_of(progress.bytes_done, progress.bytes_total)
                        )),
                ),
                vec![(
                    "取消".to_string(),
                    ButtonVariant::Danger,
                    true,
                    props.on_cancel.take(),
                )],
            )
        }
        Session::Finished { outcome, progress } => {
            let headline = match outcome {
                Outcome::Done => ("迁移完成", theme.success),
                Outcome::Cancelled => ("迁移已取消", theme.loading),
                Outcome::Failed { .. } => ("迁移未完成", theme.error),
            };
            let dismiss_label = if props.dismiss_label.is_empty() {
                "继续".to_string()
            } else {
                props.dismiss_label.clone()
            };
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(headline.0, headline.1),
                    Span::styled(
                        format!(
                            "　拷贝 {} · 保留已有 {} · 跳过 {}",
                            progress.copied, progress.kept, progress.skipped
                        ),
                        theme.text,
                    ),
                ]),
                Line::styled("源目录 ~/.novel 保留未动；已有新文件一律优先。", muted),
            ];
            if let Outcome::Failed { fatal, errors } = outcome {
                let mut problems = errors.clone();
                if let Some(fatal) = fatal.clone() {
                    problems.push(fatal);
                }
                lines.push(Line::styled(
                    format!(
                        "{} 个问题待重试（下次启动或 trn migrate）：",
                        problems.len()
                    ),
                    theme.error,
                ));
                for problem in problems.iter().take(4) {
                    lines.push(Line::styled(format!("· {problem}"), theme.error));
                }
                if problems.len() > 4 {
                    lines.push(Line::styled(format!("…等共 {} 条", problems.len()), muted));
                }
            }
            if matches!(outcome, Outcome::Done) {
                lines.push(Line::styled("完成标记已写入，后续启动不再询问。", muted));
            }
            (
                headline.0.to_string(),
                lines,
                None,
                vec![(
                    dismiss_label,
                    ButtonVariant::Primary,
                    true,
                    props.on_dismiss.take(),
                )],
            )
        }
    };

    element!(
        Border(
            flex_direction: Direction::Vertical,
            justify_content: Flex::Center,
            border_style: theme.title,
            top_title: Some(Line::from(title).style(theme.title).centered()),
        ){
            View(margin: Margin::new(2, 1), flex_direction: Direction::Vertical){
                for (index, line) in lines.iter().enumerate() {
                    View(key: index, height: Constraint::Length(1)){
                        Text(
                            text: line.clone(),
                            style: theme.text,
                            alignment: Alignment::Left,
                        )
                    }
                }
                if let Some(gauge) = gauge {
                    View(height: Constraint::Length(1)){
                        widget(gauge)
                    }
                }
                if !buttons.is_empty() {
                    View(height: Constraint::Length(1)){}
                    View(
                        flex_direction: Direction::Horizontal,
                        gap: 3,
                        height: Constraint::Length(3),
                        justify_content: Flex::Center,
                    ){
                        for (index, (label, variant, selected, on_press)) in
                            buttons.into_iter().enumerate()
                        {
                            MigrationButton(
                                key: index,
                                label: label,
                                variant: variant,
                                selected: selected,
                                on_press: on_press,
                            )
                        }
                    }
                }
            }
        }
    )
}

// ───────────────────────── 应用内弹窗(首启门控宿主) ─────────────────────────

#[component]
pub fn MigrationModal(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut state = hooks.use_atom(&MIGRATION);
    let session = state.read().clone();
    let open = !matches!(session, Session::Idle);
    // handler 与 Modal 同处独占输入层,否则 Modal 开层后 root 层收不到键。
    let layer = hooks.use_input_layer(open, true);
    // 取消按钮与 Esc 共用同一个共享取消标志。
    let cancel = match &session {
        Session::Running { cancel, .. } => Some(cancel.clone()),
        _ => None,
    };

    hooks.use_event_handler(
        EventScope::Layer(layer),
        EventPriority::Normal,
        move |event| {
            let TermEvent::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            // 先收成值再 match:读守卫不跨写,避免同 State 读写死锁。
            let session = state.read().clone();
            match session {
                Session::Ask {
                    items,
                    files,
                    bytes,
                    start_selected,
                } => match key.code {
                    KeyCode::Left | KeyCode::Right => {
                        state.set(Session::Ask {
                            items,
                            files,
                            bytes,
                            start_selected: !start_selected,
                        });
                        EventResult::Consumed
                    }
                    KeyCode::Enter => {
                        if start_selected {
                            start(true);
                        } else {
                            decline();
                        }
                        EventResult::Consumed
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        start(true);
                        EventResult::Consumed
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        decline();
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                },
                Session::Running { cancel, .. } => match key.code {
                    KeyCode::Esc => {
                        cancel.store(true, Ordering::Relaxed);
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                },
                Session::Finished { .. } => match key.code {
                    KeyCode::Enter | KeyCode::Esc => {
                        state.set(Session::Idle);
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                },
                Session::Idle => EventResult::Ignored,
            }
        },
    );

    element!(Modal(
        open: open,
        layer: Some(layer),
        width: Constraint::Percentage(60),
        height: Constraint::Length(session_height(&session)),
        style: Style::default().dim()
    ){
        MigrationView(
            session: session,
            dismiss_label: "继续",
            on_confirm: move |_: ()| start(true),
            on_decline: move |_: ()| decline(),
            on_cancel: move |_: ()| {
                if let Some(cancel) = cancel.as_ref() {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            on_dismiss: move |_: ()| state.set(Session::Idle),
        )
    })
}

// ───────────────────────── 独立全屏根(trn migrate) ─────────────────────────

#[component]
pub fn MigrationRoot(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut state = hooks.use_atom(&MIGRATION);
    // use_exit 句柄不可 Copy,键盘/跳过按钮/退出按钮各持一个(layout 同款做法)。
    let mut exit = hooks.use_exit();
    let mut exit_decline = hooks.use_exit();
    let mut exit_dismiss = hooks.use_exit();

    // 挂载即检查:已完成/无遗留直接给终态;待迁移(含曾被拒绝,显式命令无视该标记)
    // 先展示规模、确认后开始。
    hooks.use_future(async move {
        let checked =
            tokio::task::spawn_blocking(|| -> std::io::Result<(bool, usize, u64, u64)> {
                let paths = AppPaths::user_default()?;
                if matches!(super::status_at(&paths), Status::Pending | Status::Declined) {
                    let (items, files, bytes) = super::pending_survey(&paths);
                    Ok((true, items, files, bytes))
                } else {
                    Ok((false, 0, 0, 0))
                }
            })
            .await;
        match checked {
            Ok(Ok((true, items, files, bytes))) => {
                MIGRATION.set(Session::Ask {
                    items,
                    files,
                    bytes,
                    start_selected: true,
                });
            }
            Ok(Ok((false, ..))) => {
                MIGRATION.set(Session::Finished {
                    outcome: Outcome::Done,
                    progress: Progress::default(),
                });
            }
            Ok(Err(_)) | Err(_) => {
                MIGRATION.set(Session::Finished {
                    outcome: Outcome::Failed {
                        fatal: Some("无法获取用户主目录".to_string()),
                        errors: Vec::new(),
                    },
                    progress: Progress::default(),
                });
            }
        }
    });

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let TermEvent::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        let session = state.read().clone();
        match session {
            Session::Ask {
                items,
                files,
                bytes,
                start_selected,
            } => match key.code {
                KeyCode::Left | KeyCode::Right => {
                    state.set(Session::Ask {
                        items,
                        files,
                        bytes,
                        start_selected: !start_selected,
                    });
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    if start_selected {
                        start(false);
                    } else {
                        exit();
                    }
                    EventResult::Consumed
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    start(false);
                    EventResult::Consumed
                }
                KeyCode::Char('n')
                | KeyCode::Char('N')
                | KeyCode::Char('q')
                | KeyCode::Char('Q')
                | KeyCode::Esc => {
                    exit();
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            Session::Running { cancel, .. } => match key.code {
                KeyCode::Esc => {
                    cancel.store(true, Ordering::Relaxed);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            Session::Finished { .. } => match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                    exit();
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            Session::Idle => EventResult::Ignored,
        }
    });

    let session = state.read().clone();
    let cancel = match &session {
        Session::Running { cancel, .. } => Some(cancel.clone()),
        _ => None,
    };
    // 独立根不在应用树里,自己提供调色板(与应用根同源),否则按钮等组件回退默认色。
    let appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let palette = appearance.read().palette();
    element!(
        PaletteProvider(palette: palette){
            Center(width: Constraint::Percentage(70), height: Constraint::Length(session_height(&session))){
                MigrationView(
                    session: session,
                    dismiss_label: "退出",
                    on_confirm: move |_: ()| start(false),
                    on_decline: move |_: ()| exit_decline(),
                    on_cancel: move |_: ()| {
                        if let Some(cancel) = cancel.as_ref() {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    },
                    on_dismiss: move |_: ()| exit_dismiss(),
                )
            }
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_buckets() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2 << 10), "2.0 KB");
        assert_eq!(human_bytes(559 * (1 << 20) + 300_000), "559.3 MB");
    }

    #[test]
    fn percent_of_handles_empty_totals() {
        assert_eq!(percent_of(0, 0), 0);
        assert_eq!(percent_of(1, 4), 25);
        assert_eq!(percent_of(3, 4), 75);
    }
}
