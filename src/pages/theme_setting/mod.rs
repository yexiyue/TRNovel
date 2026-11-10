use crate::{
    AppearanceConfig, BackgroundMode,
    components::{ConfirmModal, KeyShortcutInfo, ShortcutInfoModal, list_select::ListSelect},
    theme::{
        AppChromeTheme,
        presets::{
            CUSTOM_COLORS, CustomTheme, ThemeChoice, ThemeEntry, color_to_hex, nearest_custom_color,
        },
    },
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Widget, WidgetRef},
};
use ratatui_kit::prelude::*;
use tui_widget_list::ListState;

/// 弹窗里的 5 个可编辑字段, 顺序即 Tab 的移动顺序。
const EDITOR_FIELDS: usize = 5;
const FIELD_NAME: usize = 0;
const FIELD_BASE: usize = 1;
const FIELD_BG: usize = 2;
const FIELD_FG: usize = 3;
const FIELD_ACCENT: usize = 4;

#[derive(Debug, Clone)]
struct CustomDraft {
    name: String,
    active: usize,
    base: usize,
    bg: usize,
    fg: usize,
    accent: usize,
}

impl Default for CustomDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            active: FIELD_NAME,
            base: 0,
            bg: 0,
            fg: 1,
            accent: 10,
        }
    }
}

impl CustomDraft {
    fn color_index(&self, field: usize) -> Option<usize> {
        match field {
            FIELD_BG => Some(self.bg),
            FIELD_FG => Some(self.fg),
            FIELD_ACCENT => Some(self.accent),
            _ => None,
        }
    }

    fn color_index_mut(&mut self, field: usize) -> Option<&mut usize> {
        match field {
            FIELD_BG => Some(&mut self.bg),
            FIELD_FG => Some(&mut self.fg),
            FIELD_ACCENT => Some(&mut self.accent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct ThemeListItem {
    entry: ThemeEntry,
    selected: bool,
    hovered: bool,
    current: bool,
    /// 自定义分组的第一项需要额外一行分组标题, 因此比普通条目高 1 行。
    group_header: bool,
    theme: AppChromeTheme,
}

impl Widget for ThemeListItem {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        self.render_ref(area, buf);
    }
}

impl WidgetRef for ThemeListItem {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        if !self.group_header {
            self.render_card(area, buf);
            return;
        }
        let [header, card] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        // 不单独占一个列表项, 而是并入第一项: 这样键盘上下移动不会在标题上停住,
        // 鼠标命中也不会把标题当成可应用的主题。
        Paragraph::new(Line::from("── 自定义 ──").style(self.theme.meta_label)).render(header, buf);
        self.render_card(card, buf);
    }
}

impl ThemeListItem {
    fn render_card(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let style = if self.selected {
            self.theme.selected
        } else if self.hovered {
            self.theme.text.add_modifier(Modifier::BOLD)
        } else if self.current {
            self.theme.highlight.add_modifier(Modifier::BOLD)
        } else {
            self.theme.text
        };

        let block = Block::bordered()
            .padding(Padding::horizontal(1))
            .border_style(if self.current {
                self.theme.highlight
            } else {
                self.theme.border
            })
            .style(if self.selected {
                self.theme.selected
            } else {
                Style::new()
            });

        let inner = block.inner(area);
        block.render(area, buf);

        let palette = self.entry.palette();
        let marker = if self.current { "当前" } else { "    " };
        // 中文名在前、slug 在后: 用户按名字挑, 排查配置时才看 slug。
        let title = Line::from(vec![
            Span::from(marker).style(self.theme.meta_label.patch(style)),
            Span::from("  "),
            Span::from(self.entry.display_name()).style(style),
            Span::from("  "),
            Span::from(self.entry.slug()).style(self.theme.meta_label.patch(style)),
        ]);

        // 直接铺一块底色, 让用户不选中也能看出背景/正文色的观感, 只比色块更接近实读效果。
        let preview_style = Style::new().fg(palette.fg).bg(palette.bg);
        let swatches = Line::from(vec![
            Span::from("正文").style(preview_style),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.accent)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.selection)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.success)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.warning)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.error)),
            Span::from(" "),
            Span::from("  ").style(Style::new().bg(palette.info)),
        ])
        .style(style);

        let [top, bottom] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);
        Paragraph::new(title).render(top, buf);
        Paragraph::new(swatches).render(bottom, buf);
    }
}

/// 打开编辑器时用当前生效配色预填。
///
/// 「从现有配色出发」不是再让用户从头挑 16 色: 名称、base 与三个颜色都取当前主题,
/// 用户只需要微调想改的那一格。
fn open_custom_editor<N: ratatui_kit::Notifier>(
    appearance: ratatui_kit::ReactiveHandle<AppearanceConfig, N>,
    mut draft: ratatui_kit::State<CustomDraft>,
    mut editor_open: ratatui_kit::State<bool>,
) {
    let config = appearance.read();
    let base_slug = config
        .custom_theme(&config.theme_slug)
        .map(|theme| theme.base.clone())
        .unwrap_or_else(|| config.theme_slug.clone());
    // 直接取主题色板而不是 config.palette(): 后者在「终端背景」模式下会把 bg 换成 Reset,
    // 用 Reset 找最近色格只会永远落在第一个墨黑, 预填就失去意义。
    let palette = config
        .custom_theme(&config.theme_slug)
        .map(CustomTheme::palette)
        .or_else(|| ThemeChoice::from_slug(&config.theme_slug).map(ThemeChoice::palette))
        .unwrap_or_else(|| ThemeChoice::Builtin(config.theme_name()).palette());
    let base = ThemeChoice::all()
        .iter()
        .position(|choice| choice.slug() == base_slug)
        .unwrap_or(0);
    let suffix = CustomTheme::next_slug(&config.custom_themes);
    let suffix = suffix.trim_start_matches("custom-");

    draft.set(CustomDraft {
        name: format!("自定义 {suffix}"),
        active: FIELD_NAME,
        base,
        bg: nearest_custom_color(palette.bg),
        fg: nearest_custom_color(palette.fg),
        accent: nearest_custom_color(palette.accent),
    });
    drop(config);
    editor_open.set(true);
}

/// 把编辑器里的色格写回配置并立即应用。
///
/// 保存后把列表选中项移到新条目, 否则用户会看到配色变了、光标却还停在原来那一行。
fn save_custom_theme<N: ratatui_kit::Notifier>(
    mut appearance: ratatui_kit::ReactiveHandle<AppearanceConfig, N>,
    draft: ratatui_kit::State<CustomDraft>,
    mut editor_open: ratatui_kit::State<bool>,
    list_state: ratatui_kit::State<ListState>,
) {
    let draft = draft.read().clone();
    let name = draft.name.trim();
    if name.is_empty() {
        // 名称是列表里唯一可读的标识, 空名会让用户分不清自己建了哪套方案。
        return;
    }

    let choices = ThemeChoice::all();
    let base = choices.get(draft.base).copied().unwrap_or(choices[0]);
    let color_hex = |index: usize| {
        color_to_hex(CUSTOM_COLORS[index % CUSTOM_COLORS.len()].color).unwrap_or_default()
    };

    let mut next = appearance.read().clone();
    let slug = CustomTheme::next_slug(&next.custom_themes);
    next.custom_themes.push(CustomTheme {
        name: name.to_string(),
        slug: slug.clone(),
        base: base.slug().to_string(),
        bg: color_hex(draft.bg),
        fg: color_hex(draft.fg),
        accent: color_hex(draft.accent),
    });
    next.theme_slug = slug.clone();
    let _ = next.save();
    appearance.set(next.clone());

    let entries = ThemeEntry::all(&next.custom_themes);
    if let Some(index) = entries.iter().position(|entry| entry.slug() == slug) {
        list_state.write().select(Some(index));
    }
    editor_open.set(false);
}

#[component]
pub fn ThemeSetting(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut info_modal_open = hooks.use_state(|| false);
    let mut reset_modal_open = hooks.use_state(|| false);
    let mut editor_open = hooks.use_state(|| false);
    let mut delete_modal_open = hooks.use_state(|| false);
    let mut draft = hooks.use_state(CustomDraft::default);
    let mut delete_target = hooks.use_state(|| None::<(String, String)>);
    let mut appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let entries = ThemeEntry::all(&appearance.read().custom_themes);
    // 未知 slug(手改配置/未来版本回退)回落到应用当前实际生效的主题,
    // 保证「当前」标记永远指向屏幕上真正的那套配色。
    let current_slug = {
        let config = appearance.read();
        if entries
            .iter()
            .any(|entry| entry.slug() == config.theme_slug)
        {
            config.theme_slug.clone()
        } else {
            config.theme_name().slug().to_string()
        }
    };
    let selected_index = entries
        .iter()
        .position(|entry| entry.slug() == current_slug)
        .unwrap_or(0);
    let first_custom = entries.iter().position(ThemeEntry::is_custom);
    let list_state = hooks.use_state(move || {
        let mut state = ListState::default();
        state.selected = Some(selected_index);
        state
    });
    let editor_layer = hooks.use_input_layer(editor_open.get(), true);

    let mut toggle_background = move |_: ()| {
        let mut next = appearance.read().clone();
        next.background = match next.background {
            BackgroundMode::Theme => BackgroundMode::Terminal,
            BackgroundMode::Terminal => BackgroundMode::Theme,
        };
        let _ = next.save();
        appearance.set(next);
    };
    // 退出确认被关掉后,这里是唯一能重新打开它的入口。
    let mut toggle_confirm_exit = move |_: ()| {
        let mut next = appearance.read().clone();
        next.confirm_exit = !next.confirm_exit;
        let _ = next.save();
        appearance.set(next);
    };

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let entries = entries.clone();
        let open_editor = move || open_custom_editor(appearance, draft, editor_open);
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            // 弹窗由各自的输入层独占, 这里只处理列表页的快捷键, 避免 Esc/Enter 穿透。
            if editor_open.get() || delete_modal_open.get() || reset_modal_open.get() {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(!info_modal_open.get());
                    EventResult::Consumed
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    reset_modal_open.set(true);
                    EventResult::Consumed
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    open_editor();
                    EventResult::Consumed
                }
                KeyCode::Char('x') | KeyCode::Char('X') => {
                    // 只允许删除自定义项: 预设与内置主题是代码资产, 不该被配置删掉。
                    let target = list_state
                        .read()
                        .selected
                        .and_then(|index| entries.get(index))
                        .and_then(|entry| match entry {
                            ThemeEntry::Custom(custom) => {
                                Some((custom.slug.clone(), custom.name.clone()))
                            }
                            _ => None,
                        });
                    if let Some(target) = target {
                        delete_target.set(Some(target));
                        delete_modal_open.set(true);
                        EventResult::Consumed
                    } else {
                        EventResult::Ignored
                    }
                }
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    toggle_background(());
                    EventResult::Consumed
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    toggle_confirm_exit(());
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });

    // 编辑器独占一层: Modal 复用这层, 页面里注册的 handler 才能收到按键。
    hooks.use_event_handler(
        EventScope::Layer(editor_layer),
        EventPriority::Normal,
        move |event| {
            if !editor_open.get() {
                return EventResult::Ignored;
            }
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Esc => {
                    editor_open.set(false);
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    save_custom_theme(appearance, draft, editor_open, list_state);
                    EventResult::Consumed
                }
                KeyCode::Tab | KeyCode::Down | KeyCode::Up | KeyCode::BackTab => {
                    let mut next = draft.read().clone();
                    let forward = !matches!(key.code, KeyCode::Up | KeyCode::BackTab);
                    next.active = if forward {
                        (next.active + 1) % EDITOR_FIELDS
                    } else {
                        (next.active + EDITOR_FIELDS - 1) % EDITOR_FIELDS
                    };
                    draft.set(next);
                    EventResult::Consumed
                }
                KeyCode::Left | KeyCode::Right => {
                    let mut next = draft.read().clone();
                    let delta = if key.code == KeyCode::Right { 1 } else { -1 };
                    if next.active == FIELD_BASE {
                        let len = ThemeChoice::all().len();
                        next.base = (next.base as isize + delta).rem_euclid(len as isize) as usize;
                    } else if let Some(index) = next.color_index_mut(next.active) {
                        let len = CUSTOM_COLORS.len();
                        *index = (*index as isize + delta).rem_euclid(len as isize) as usize;
                    }
                    draft.set(next);
                    EventResult::Consumed
                }
                KeyCode::Backspace if draft.read().active == FIELD_NAME => {
                    let mut next = draft.read().clone();
                    next.name.pop();
                    draft.set(next);
                    EventResult::Consumed
                }
                KeyCode::Char(ch) if draft.read().active == FIELD_NAME && !ch.is_control() => {
                    let mut next = draft.read().clone();
                    next.name.push(ch);
                    draft.set(next);
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            }
        },
    );

    let base_choices = ThemeChoice::all();
    let draft_snapshot = draft.read().clone();
    let editor_lines = custom_editor_lines(&draft_snapshot, &base_choices, theme);

    element!(View{
        crate::components::ActionBar(menu: "设置") {
            crate::components::ActionButton(label: if appearance.read().show_header { "顶部栏：显示" } else { "顶部栏：隐藏" }, on_press: move |_| {
                crate::components::action_bar::toggle_header(appearance);
            })
            crate::components::ActionButton(label: "切换背景模式", shortcut: "T", on_press: toggle_background)
            crate::components::ActionButton(label: if appearance.read().confirm_exit { "退出确认：开启" } else { "退出确认：关闭" }, shortcut: "C", on_press: toggle_confirm_exit)
            crate::components::ActionButton(label: "重置外观", shortcut: "D", on_press: move |_| reset_modal_open.set(true))
            crate::components::ActionButton(label: "帮助", shortcut: "I", on_press: move |_| info_modal_open.set(true))
            crate::components::ActionButton(label: "新建配色", shortcut: "N", on_press: move |_| open_custom_editor(appearance, draft, editor_open))
        }
        ListSelect<ThemeEntry>(
            items: entries.clone(),
            state: list_state,
            top_title: Line::from("全局设置").style(theme.title).centered(),
            bottom_title: Line::from(format!(
                "当前: {} · 背景: {}",
                entries
                    .iter()
                    .find(|entry| entry.slug() == current_slug)
                    .map(ThemeEntry::display_name)
                    .unwrap_or("默认主题"),
                background_label(appearance.read().background),
            ))
            .style(theme.meta_label)
            .centered(),
            render_item:{
                let entries = entries.clone();
                move |ctx:&VirtualItemContext|{
                    let entry = entries[ctx.index].clone();
                    let group_header = first_custom == Some(ctx.index);
                    (ThemeListItem {
                        current: entry.slug() == current_slug,
                        entry,
                        selected: ctx.is_selected,
                        hovered: ctx.is_hovered,
                        group_header,
                        theme,
                    }.into(), if group_header { 5 } else { 4 })
                }
            },
            is_editing: !info_modal_open.get()
                && !reset_modal_open.get()
                && !editor_open.get()
                && !delete_modal_open.get(),
            empty_message: "暂无可用主题",
            on_select: move |entry:ThemeEntry| {
                let mut next = appearance.read().clone();
                next.theme_slug = entry.slug().to_string();
                let _ = next.save();
                appearance.set(next);
            },
        )
        ShortcutInfoModal(
            key_shortcut_info: KeyShortcutInfo::new(vec![
                ("选择下一个", "J / ▼"),
                ("选择上一个", "K / ▲"),
                ("应用主题", "Enter"),
                ("新建自定义方案", "N"),
                ("删除当前自定义方案", "X"),
                ("切换终端背景", "T"),
                ("退出前确认", "C"),
                ("重置外观", "D"),
            ]),
            open: info_modal_open.get(),
            on_close:{let mut open = info_modal_open; move |_: ()| open.set(false)},
        )
        ConfirmModal(
            title: "删除自定义方案",
            content: match delete_target.read().as_ref() {
                Some((_, name)) => format!("是否删除自定义方案「{name}」？删除后若正在使用会回到默认主题。"),
                None => "是否删除当前自定义方案？".to_string(),
            },
            open: delete_modal_open.get(),
            on_confirm: {
                move |_| {
                    // 先克隆目标再改状态: 直接在 `read()` 的 guard 上 set 会同时借不可变与可变。
                    let target = delete_target.read().clone();
                    if let Some((slug, _)) = target {
                        let mut next = appearance.read().clone();
                        next.remove_custom_theme(&slug);
                        let _ = next.save();
                        appearance.set(next.clone());
                        // remove_custom_theme 已在删除当前主题时回落默认主题; 这里按新列表
                        // 重新定位光标, 否则列表重建后选中项会指向被删掉的下一项。
                        let updated = ThemeEntry::all(&next.custom_themes);
                        if let Some(index) =
                            updated.iter().position(|entry| entry.slug() == next.theme_slug)
                        {
                            list_state.write().select(Some(index));
                        }
                        delete_target.set(None);
                    }
                    delete_modal_open.set(false);
                }
            },
            on_cancel: {
                let entries = entries.clone();
                move |_| {
                    // 取消时也覆盖一次选中项, 防止列表在弹窗期间因外部状态变化而越界。
                    if let Some(index) = list_state
                        .read()
                        .selected
                        .filter(|index| *index < entries.len())
                    {
                        list_state.write().select(Some(index));
                    }
                    delete_target.set(None);
                    delete_modal_open.set(false);
                }
            }
        )
        ConfirmModal(
            title: "重置外观",
            content: "是否恢复默认主题、终端背景并显示顶部栏？自定义方案也会一并清空。",
            open: reset_modal_open.get(),
            on_confirm: {
                move |_| {
                    let next = AppearanceConfig::default();
                    let _ = next.save();
                    appearance.set(next.clone());
                    let entries = ThemeEntry::all(&next.custom_themes);
                    let target = entries
                        .iter()
                        .position(|entry| entry.slug() == next.theme_slug)
                        .unwrap_or(0);
                    list_state.write().select(Some(target));
                    reset_modal_open.set(false);
                }
            },
            on_cancel: move |_| {
                reset_modal_open.set(false);
            }
        )
        Modal(
            open: editor_open.get(),
            width: Constraint::Percentage(60),
            height: Constraint::Length(14),
            layer: editor_layer,
            style: Style::new().dim(),
        ){
            Border(
                top_title: Some(Line::from("新建自定义方案").style(theme.title)),
                border_style: theme.border,
            ){
                View {
                    widget(Paragraph::new(editor_lines))
                }
            }
        }
    })
}

/// 编辑器正文: 每行一个字段, 当前行高亮, 颜色字段带真正铺色的色块。
fn custom_editor_lines(
    draft: &CustomDraft,
    base_choices: &[ThemeChoice],
    theme: AppChromeTheme,
) -> Vec<Line<'static>> {
    let active_label = |field: usize| {
        if draft.active == field {
            theme.highlight.add_modifier(Modifier::BOLD)
        } else {
            theme.meta_label
        }
    };
    let cursor = if draft.active == FIELD_NAME {
        "▏"
    } else {
        ""
    };
    let mut lines = vec![Line::from(vec![
        Span::from("名称  ").style(active_label(FIELD_NAME)),
        Span::from(format!("{}{cursor}", draft.name)).style(theme.text),
    ])];

    let base = base_choices
        .get(draft.base)
        .copied()
        .unwrap_or(base_choices[0]);
    lines.push(Line::from(vec![
        Span::from("基于  ").style(active_label(FIELD_BASE)),
        Span::from("◀ ").style(theme.meta_label),
        Span::from(base.display_name()).style(theme.text),
        Span::from(" ▶").style(theme.meta_label),
    ]));

    for (field, label) in [
        (FIELD_BG, "底色  "),
        (FIELD_FG, "正文色"),
        (FIELD_ACCENT, "强调色"),
    ] {
        let index = draft.color_index(field).unwrap_or(0);
        let color = CUSTOM_COLORS[index % CUSTOM_COLORS.len()];
        let hex = color_to_hex(color.color).unwrap_or_default();
        lines.push(Line::from(vec![
            Span::from(label).style(active_label(field)),
            Span::from("  ◀ ").style(theme.meta_label),
            Span::from("    ").style(Style::new().bg(color.color)),
            Span::from(" "),
            Span::from(format!("{}  {hex}", color.name)).style(theme.text),
            Span::from(" ▶").style(theme.meta_label),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(
        Line::from("Tab / ↑↓ 切换字段 · ←→ 选择 · Enter 保存 · Esc 取消").style(theme.meta_label),
    );
    lines.push(
        Line::from("颜色在固定 16 色里循环, 配置文件中保存为 #rrggbb").style(theme.meta_label),
    );
    lines.push(Line::from("保存后立即应用, 并追加到上方「自定义」分组").style(theme.meta_label));
    lines
}

fn background_label(mode: BackgroundMode) -> &'static str {
    match mode {
        BackgroundMode::Theme => "主题背景",
        BackgroundMode::Terminal => "终端背景",
    }
}
