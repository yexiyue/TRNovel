//! 书源登录页(9.1/9.2/12.1):loginUi 表单 → 触发脚本/浏览器登录 → 产物落盘(经 build_engine 注入)。
//!
//! - `loginUrl` 为 `@js:`/`<js>` 脚本 → 脚本登录(`source.getLoginInfo()` 读本页收集的凭据);
//! - `loginUrl` 为普通 URL → headful 浏览器登录(用户在真实页面登录,Enter 完成 / Esc 取消)。
//!
//! 表单单字段编辑(Up/Down/Tab 切换,password 掩码),Enter 提交。登录态由 [`crate::login`] 落盘。

use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::{BookSource, LoginSignal, source::RowUiType};
use ratatui::{
    layout::{Constraint, Margin},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_kit::prelude::*;
use std::sync::atomic::Ordering;
use tui_input::{Input, backend::crossterm::EventHandler};

use crate::{
    components::{Loading, WarningModal},
    errors::Errors,
    hooks::UseInitState,
    login::{browser_login, is_script_login, login_info_json, script_login},
    theme::AppChromeTheme,
};

#[component]
pub fn BookSourceLogin(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let source = (*hooks.use_route_state::<BookSource>()).clone();
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut navigate = hooks.use_navigate();
    // 登录页是独占全屏表单:开一个 blocks_lower 输入层,本页独占输入并自动截断 Layout 的全局键
    // (q/g/b),取代旧的全局 `is_inputting` 标志;离页卸载后该层下一帧自动消失。
    let layer = hooks.use_input_layer(true, true);

    let ui = source.login_ui.clone();
    let n = ui.len();
    let script = is_script_login(&source);
    // 书源配置错误:仅配置 loginUi 而无登录脚本/loginUrl——表单收集的凭据无处可去,
    // 浏览器登录也会因 loginUrl 为空必然失败。拦截 Enter 提交并明确提示,而非引导用户填一个
    // 注定被丢弃的表单。
    let misconfigured = !script && source.login_url.trim().is_empty() && n > 0;

    let fields = hooks.use_state(|| ui.iter().map(|_| Input::default()).collect::<Vec<Input>>());
    let mut active = hooks.use_state(|| 0usize);
    let mut submit = hooks.use_state(|| 0u32);
    let signal = hooks.use_state(LoginSignal::default);
    // 浏览器登录进行中:显示「Enter 完成 / Esc 取消」提示并接收按键翻转 signal。
    let mut browsering = hooks.use_state(|| false);
    // 提交互斥(同步置位):loading 经 use_effect_state 有 200ms 防抖窗口,窗口内二次 Enter
    // 会替换在飞登录 future(双开浏览器抢同一 profile / 丢弃脚本登录结果),故按键互斥
    // 以 in_flight 为准,loading 仅驱动 spinner 渲染。
    let mut in_flight = hooks.use_state(|| false);

    // 触发登录:keyed on submit 计数(mount 时 submit=0 → no-op,不自动登录)。
    let (_done, loading, error) = hooks.use_effect_state(
        {
            let source = source.clone();
            async move {
                if submit.get() == 0 {
                    return Ok::<(), Errors>(());
                }
                // 包住结果统一复位 in_flight(覆盖 `?` 失败路径),否则一次失败后页面永久锁死。
                let result = async {
                    if script {
                        let info = if n > 0 {
                            let pairs: Vec<(String, String)> = source
                                .login_ui
                                .iter()
                                .zip(fields.read().iter())
                                .map(|(row, inp)| (row.name.clone(), inp.value().to_string()))
                                .collect();
                            Some(login_info_json(&pairs))
                        } else {
                            None
                        };
                        script_login(source.clone(), info).await?;
                    } else {
                        let sig = signal.read().clone();
                        browser_login(source.clone(), sig).await?;
                    }
                    Ok::<(), Errors>(())
                }
                .await;
                in_flight.set(false);
                result
            }
        },
        submit.get(),
    );

    // 登录成功(已触发、加载完、无错)→ 复位输入态并**直接进入选书页**(而非弹回书源列表),
    // 登录态已落盘、随 build_engine 注入,用户即可读全本;再回列表时该源显示「已登录」。
    hooks.use_effect(
        {
            let source = source.clone();
            move || {
                if submit.get() > 0 && !loading.get() && error.read().is_none() {
                    navigate.push_with_state("/select-books", source.clone());
                }
            }
        },
        loading.get(),
    );

    hooks.use_event_handler(
        EventScope::Layer(layer),
        EventPriority::Normal,
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            // 登录进行中(in_flight 同步互斥):浏览器登录可 Enter 完成 / Esc 取消;
            // 脚本登录 Esc 可放弃等待返回书源页;其余按键忽略(但本层独占,统一 Consumed)。
            if in_flight.get() {
                if browsering.get() {
                    match key.code {
                        // done/cancel 仍附加 loading(>200ms)条件:避免浏览器尚未真正启动时
                        // 抢先置 done,把空产物当成功提交。
                        KeyCode::Enter if loading.get() => {
                            signal.read().done.store(true, Ordering::Relaxed)
                        }
                        KeyCode::Esc if loading.get() => {
                            signal.read().cancel.store(true, Ordering::Relaxed)
                        }
                        _ => {}
                    }
                } else if key.code == KeyCode::Esc {
                    // 脚本登录无取消通道(spawn_blocking 里的 block_on 不可中断):允许用户
                    // 离开登录页;在飞任务由 30s 默认超时兜底自行了结,其慢成功结果随组件
                    // 卸载被丢弃不落盘(用户主动放弃语义,可接受)。
                    navigate.push("/book-source");
                }
                return EventResult::Consumed;
            }
            match key.code {
                KeyCode::Esc => {
                    navigate.push("/book-source");
                    EventResult::Consumed
                }
                KeyCode::Down | KeyCode::Tab if n > 0 => {
                    active.set((active.get() + 1) % n);
                    EventResult::Consumed
                }
                KeyCode::Up | KeyCode::BackTab if n > 0 => {
                    active.set((active.get() + n - 1) % n);
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    // loginUi-only 配置错误:凭据无处可去,拦截提交(页面已有提示)。
                    if misconfigured {
                        return EventResult::Consumed;
                    }
                    // 重试前清掉上次错误:返回导航以 error 为空为条件,不清的话即使重试成功
                    // 也永远卡在错误弹窗、不返回书源页。
                    error.write().take();
                    if !script {
                        browsering.set(true);
                    }
                    in_flight.set(true);
                    submit.set(submit.get() + 1);
                    EventResult::Consumed
                }
                _ if n > 0 => {
                    let i = active.get().min(n - 1);
                    fields.write()[i].handle_event(&event);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        },
    );

    // 表单文本(单 Paragraph 渲染:name: 值,password 掩码,active 行高亮 + 光标标记)。
    let active_idx = active.get();
    let mut lines: Vec<Line> = Vec::new();
    for (i, row) in ui.iter().enumerate() {
        let raw = fields.read()[i].value().to_string();
        let shown = if row.ui_type == RowUiType::Password {
            "•".repeat(raw.chars().count())
        } else {
            raw
        };
        let cursor = if i == active_idx { "▏" } else { "" };
        let label_style = if i == active_idx {
            theme.meta_label.bold()
        } else {
            theme.meta_label
        };
        lines.push(Line::from(vec![
            Span::from(format!("{}: ", row.name)).style(label_style),
            Span::from(format!("{shown}{cursor}")).style(theme.text),
        ]));
        lines.push(Line::from(""));
    }
    if n == 0 {
        let tip = if script {
            "此书源为脚本登录(无需填写表单)。按 Enter 开始登录。"
        } else {
            "此书源为浏览器登录。按 Enter 打开系统浏览器,在真实页面登录后回到此处按 Enter 完成。"
        };
        lines.push(Line::from(tip).style(theme.text));
    }
    if misconfigured {
        // loginUi-only 配置错误:明确告知,Enter 已被拦截(避免引导用户填一个会被丢弃的表单)。
        lines.push(
            Line::from("书源配置错误:仅配置 loginUi 而无登录脚本/loginUrl,无法执行登录。")
                .style(theme.text.bold()),
        );
    }

    let hint = if misconfigured {
        "该书源登录配置不完整(联系书源作者修正)· Esc 返回"
    } else if in_flight.get() && !browsering.get() {
        "脚本登录中... Esc 放弃等待并返回"
    } else if browsering.get() && loading.get() {
        "浏览器登录中:在弹出的浏览器里登录,完成后按 Enter,取消按 Esc"
    } else if script {
        "↑/↓/Tab 切换字段 · 输入凭据 · Enter 登录 · Esc 返回"
    } else {
        "Enter 打开浏览器登录 · Esc 返回"
    };

    let body = Paragraph::new(lines);

    element!(Border(
        top_title: Line::from(format!("书源登录 · {}", source.name)).centered().style(theme.title),
        border_style: theme.border,
    ){
        View(margin: Margin::new(2, 1), flex_direction: ratatui::layout::Direction::Vertical){
            View(height: Constraint::Fill(1)){
                Text(text: body, style: theme.text)
            }
            View(height: Constraint::Length(1)){
                Text(text: Paragraph::new(Line::from(hint).style(theme.meta_label)))
            }
        }
        {if loading.get() {
            element!(Loading(tip: if browsering.get() {"等待浏览器登录..."} else {"登录中..."})).into_any()
        } else {
            element!(Fragment).into_any()
        }}
        WarningModal(
            tip: format!("登录失败:{}", error.read().as_ref().map(|e| e.to_string()).unwrap_or_default()),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
            // 登录失败是可恢复错误:按 q 关闭弹窗清掉 error 即可重试,
            // 不落入 WarningModal 默认 handler 的「按 q 退出整个应用」分支。
            on_close: move |_| {
                error.write().take();
            },
        )
    })
}
