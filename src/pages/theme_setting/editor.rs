//! Custom theme draft, inheritance and editing interaction.
use crate::{
    AppearanceConfig,
    components::settings,
    theme::presets::{
        CUSTOM_COLORS, CustomTheme, PALETTE_FIELDS, ThemeChoice, ThemeEntry, color_to_hex,
        nearest_custom_color, palette_field, parse_hex_color,
    },
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{layout::Constraint, style::Color};
use ratatui_kit::prelude::*;
use std::{collections::BTreeMap, sync::Arc};
use tui_widget_list::ListState;

/// 编辑器里 Tab 轮换的三个区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorFocus {
    Name,
    Base,
    Colors,
}

#[derive(Debug, Clone)]
pub(super) struct CustomDraft {
    /// 编辑已有方案时保留原 slug; 新建时为 None。
    pub editing_slug: Option<String>,
    name: String,
    focus: EditorFocus,
    base: usize,
    /// 当前选中的调色字段下标, 与 PALETTE_FIELDS 同序。
    field: usize,
    /// 每个字段当前选中的颜色, 与 PALETTE_FIELDS 同序。
    colors: Vec<Color>,
    overridden: Vec<bool>,
    /// 手输的 #rrggbb 草稿; 空表示仍用 16 色色格。
    hex_input: String,
}

impl Default for CustomDraft {
    fn default() -> Self {
        Self {
            editing_slug: None,
            name: String::new(),
            focus: EditorFocus::Name,
            base: 0,
            field: 0,
            colors: vec![Color::Reset; PALETTE_FIELDS.len()],
            overridden: vec![false; PALETTE_FIELDS.len()],
            hex_input: String::new(),
        }
    }
}

impl CustomDraft {
    fn from_config(config: &AppearanceConfig, edit_slug: Option<&str>) -> Self {
        let editing = edit_slug
            .and_then(|slug| config.custom_theme(slug))
            .cloned();
        let source_theme = editing
            .as_ref()
            .or_else(|| config.custom_theme(&config.theme_slug));
        let overridden = PALETTE_FIELDS
            .iter()
            .map(|field| {
                source_theme.is_some_and(|theme| {
                    theme
                        .colors
                        .get(field.key)
                        .is_some_and(|value| parse_hex_color(value).is_some())
                        || match field.key {
                            "fg" => parse_hex_color(&theme.fg).is_some(),
                            "bg" => parse_hex_color(&theme.bg).is_some(),
                            "accent" => parse_hex_color(&theme.accent).is_some(),
                            _ => false,
                        }
                })
            })
            .collect();
        let (name, base_slug, palette, editing_slug) = match editing {
            Some(theme) => (
                theme.name.clone(),
                theme.base.clone(),
                theme.palette(),
                Some(theme.slug),
            ),
            None => {
                let base_slug = config
                    .custom_theme(&config.theme_slug)
                    .map(|theme| theme.base.clone())
                    .unwrap_or_else(|| config.theme_slug.clone());
                let slug = CustomTheme::next_slug(&config.custom_themes);
                let suffix = slug.trim_start_matches("custom-");
                (
                    format!("自定义 {suffix}"),
                    base_slug,
                    effective_palette(config),
                    None,
                )
            }
        };
        let base = ThemeChoice::all()
            .iter()
            .position(|choice| choice.slug() == base_slug)
            .unwrap_or(0);
        let colors = PALETTE_FIELDS
            .iter()
            .map(|field| palette_field(&palette, field.key).unwrap_or(Color::Reset))
            .collect();

        Self {
            editing_slug,
            name,
            focus: EditorFocus::Name,
            base,
            field: 0,
            colors,
            overridden,
            hex_input: String::new(),
        }
    }
    fn change_base(&mut self, base: usize) {
        if base >= ThemeChoice::all().len() {
            return;
        }
        self.base = base;
        self.refresh_inherited();
    }
    fn refresh_inherited(&mut self) {
        let palette = self.theme().palette();
        for (index, field) in PALETTE_FIELDS.iter().enumerate() {
            if !self.overridden[index] {
                self.colors[index] = palette_field(&palette, field.key).unwrap_or(Color::Reset);
            }
        }
    }
    fn set_color(&mut self, field: usize, color: Color) {
        self.colors[field] = color;
        self.overridden[field] = true;
        self.refresh_inherited();
    }
    fn theme(&self) -> CustomTheme {
        let choices = ThemeChoice::all();
        let base = choices.get(self.base).copied().unwrap_or(choices[0]);
        let mut colors = BTreeMap::new();
        for (index, (field, color)) in PALETTE_FIELDS.iter().zip(self.colors.iter()).enumerate() {
            if !self.overridden[index] {
                continue;
            }
            if let Some(hex) = color_to_hex(*color) {
                colors.insert(field.key.to_string(), hex);
            }
        }

        CustomTheme {
            name: self.name.trim().to_owned(),
            slug: String::new(),
            base: base.slug().to_string(),
            bg: String::new(),
            fg: String::new(),
            accent: String::new(),
            colors,
        }
    }
}

fn effective_palette(config: &AppearanceConfig) -> ratatui_kit::Palette {
    config
        .custom_theme(&config.theme_slug)
        .map(CustomTheme::palette)
        .or_else(|| ThemeChoice::from_slug(&config.theme_slug).map(ThemeChoice::palette))
        .unwrap_or_else(|| ThemeChoice::Builtin(config.theme_name()).palette())
}

/// 打开编辑器: edit_slug 有值时预填该自定义方案, 否则从当前配色新建。
pub(super) fn open_custom_editor(
    appearance: AtomState<AppearanceConfig>,
    mut draft: ratatui_kit::State<CustomDraft>,
    mut editor_open: ratatui_kit::State<bool>,
    edit_slug: Option<String>,
) {
    draft.set(CustomDraft::from_config(
        &appearance.read(),
        edit_slug.as_deref(),
    ));
    editor_open.set(true);
}

/// 把编辑器里的逐字段颜色写回配置并立即应用。
///
/// 保存显式覆盖，未改字段继承最终色板的派生规则；编辑已有方案时覆盖
/// 同一个 slug 和列表位置, 新建时追加到「自定义」分组。
pub(super) fn save_custom_theme(
    mut appearance: AtomState<AppearanceConfig>,
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

    let theme = draft.theme();
    let mut next = appearance.read().clone();
    let slug = match draft.editing_slug.as_deref() {
        Some(slug) if next.custom_theme(slug).is_some() => {
            if let Some(existing) = next.custom_themes.iter_mut().find(|item| item.slug == slug) {
                *existing = CustomTheme {
                    slug: slug.to_string(),
                    ..theme
                };
            }
            slug.to_string()
        }
        _ => {
            let slug = CustomTheme::next_slug(&next.custom_themes);
            next.custom_themes.push(CustomTheme {
                slug: slug.clone(),
                ..theme
            });
            slug
        }
    };
    next.theme_slug = slug.clone();
    let _ = next.save();
    appearance.set(next.clone());

    let entries = ThemeEntry::all(&next.custom_themes);
    if let Some(index) = entries.iter().position(|entry| entry.slug() == slug) {
        list_state.write().select(Some(index));
    }
    editor_open.set(false);
}

#[derive(Props)]
pub(super) struct CustomEditorProps {
    pub draft: State<CustomDraft>,
    pub on_save: Handler<'static, ()>,
    pub on_cancel: Handler<'static, ()>,
}

/// The same form and exclusive pickers as reader and global settings. Text/hex
/// editing remains owned by the editor's keyboard layer.
#[component]
pub(super) fn CustomEditor(
    props: &mut CustomEditorProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let mut draft = props.draft;
    let save = settings::use_callback(&mut hooks, &mut props.on_save);
    let cancel = settings::use_callback(&mut hooks, &mut props.on_cancel);
    let mut keyboard_revision = hooks.use_state(|| 0u64);
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let save = save.clone();
        let cancel = cancel.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            keyboard_revision.set(keyboard_revision.get().wrapping_add(1));
            match key.code {
                KeyCode::Esc => {
                    cancel.call(());
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    save.call(());
                    EventResult::Consumed
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    let mut next = draft.read().clone();
                    next.focus = match (key.code, next.focus) {
                        (KeyCode::BackTab, EditorFocus::Name) => EditorFocus::Colors,
                        (KeyCode::BackTab, EditorFocus::Base) => EditorFocus::Name,
                        (KeyCode::BackTab, EditorFocus::Colors) => EditorFocus::Base,
                        (_, EditorFocus::Name) => EditorFocus::Base,
                        (_, EditorFocus::Base) => EditorFocus::Colors,
                        (_, EditorFocus::Colors) => EditorFocus::Name,
                    };
                    next.hex_input.clear();
                    draft.set(next);
                    EventResult::Consumed
                }
                KeyCode::Up | KeyCode::Down => {
                    let mut next = draft.read().clone();
                    next.focus = EditorFocus::Colors;
                    let len = PALETTE_FIELDS.len();
                    next.field = if key.code == KeyCode::Down {
                        (next.field + 1) % len
                    } else {
                        (next.field + len - 1) % len
                    };
                    next.hex_input.clear();
                    draft.set(next);
                    EventResult::Consumed
                }
                KeyCode::Left | KeyCode::Right => {
                    let mut next = draft.read().clone();
                    let delta = if key.code == KeyCode::Right { 1 } else { -1 };
                    match next.focus {
                        EditorFocus::Base => {
                            let len = ThemeChoice::all().len();
                            let base =
                                (next.base as isize + delta).rem_euclid(len as isize) as usize;
                            next.change_base(base);
                        }
                        EditorFocus::Colors => {
                            let len = CUSTOM_COLORS.len();
                            let current = nearest_custom_color(next.colors[next.field]);
                            let index =
                                (current as isize + delta).rem_euclid(len as isize) as usize;
                            next.set_color(next.field, CUSTOM_COLORS[index].color);
                            next.hex_input.clear();
                        }
                        EditorFocus::Name => {}
                    }
                    draft.set(next);
                    EventResult::Consumed
                }
                KeyCode::Backspace => {
                    let mut next = draft.read().clone();
                    match next.focus {
                        EditorFocus::Name => {
                            next.name.pop();
                        }
                        EditorFocus::Colors => {
                            next.hex_input.pop();
                            if let Some(color) = parse_hex_color(&next.hex_input) {
                                next.set_color(next.field, color);
                            }
                        }
                        EditorFocus::Base => {}
                    }
                    draft.set(next);
                    EventResult::Consumed
                }
                KeyCode::Char(ch) => {
                    let mut next = draft.read().clone();
                    match next.focus {
                        EditorFocus::Name if !ch.is_control() => next.name.push(ch),
                        EditorFocus::Colors if ch == '#' || ch.is_ascii_hexdigit() => {
                            // 输满 7 位后重新开一段, 让用户可以直接覆盖上一份 hex。
                            if next.hex_input.len() >= 7 {
                                next.hex_input.clear();
                            }
                            next.hex_input.push(ch);
                            if let Some(color) = parse_hex_color(&next.hex_input) {
                                next.set_color(next.field, color);
                            }
                        }
                        _ => {}
                    }
                    draft.set(next);
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            }
        }
    });

    let mut picker = hooks.use_state(|| None::<usize>);
    let snapshot = draft.read().clone();
    let bases = ThemeChoice::all();
    let mut rows = vec![
        settings::Row {
            swatch: None,
            group: "方案",
            key: 0,
            label: "名称".into(),
            value: snapshot.name.clone(),
            detail: "点击后直接输入名称".into(),
            control: settings::Control::Action,
        },
        settings::Row {
            swatch: None,
            group: "方案",
            key: 1,
            label: "基于".into(),
            value: bases[snapshot.base].display_name().into(),
            detail: "未改字段沿用基础主题".into(),
            control: settings::Control::Choice,
        },
    ];
    rows.extend(
        PALETTE_FIELDS
            .iter()
            .enumerate()
            .map(|(index, field)| settings::Row {
                swatch: Some(snapshot.colors[index]),
                group: "颜色",
                key: index + 2,
                label: field.label.into(),
                value: color_to_hex(snapshot.colors[index]).unwrap_or_else(|| "默认".into()),
                detail: if index == snapshot.field && !snapshot.hex_input.is_empty() {
                    format!("输入 {} · {}", snapshot.hex_input, field.usage)
                } else {
                    field.usage.into()
                },
                control: settings::Control::Choice,
            }),
    );
    let selected = match snapshot.focus {
        EditorFocus::Name => 0,
        EditorFocus::Base => 1,
        EditorFocus::Colors => snapshot.field + 2,
    };
    let editor = settings::callback(move |edit| {
        let index = match edit {
            settings::Edit::Focus(i)
            | settings::Edit::Activate(i)
            | settings::Edit::Adjust(i, _) => i,
        };
        let mut next = draft.read().clone();
        let previous = (next.focus, next.field);
        next.focus = match index {
            0 => EditorFocus::Name,
            1 => EditorFocus::Base,
            _ => EditorFocus::Colors,
        };
        if index >= 2 {
            next.field = index - 2;
        }
        if previous != (next.focus, next.field) {
            next.hex_input.clear();
        }
        if matches!(edit, settings::Edit::Activate(_)) && index > 0 {
            picker.set(Some(index));
        }
        draft.set(next);
    });
    let choice = picker.get().map(|index| {
        let (options, initial) = if index == 1 {
            (
                bases.iter().map(|b| b.display_name().to_string()).collect(),
                snapshot.base,
            )
        } else {
            (
                CUSTOM_COLORS
                    .iter()
                    .map(|c| format!("{}  {}", c.name, color_to_hex(c.color).unwrap_or_default()))
                    .collect(),
                nearest_custom_color(snapshot.colors[index - 2]),
            )
        };
        (index, Arc::new(options), initial)
    });
    let detail = rows[selected].detail.clone();
    element!(View(flex_direction:ratatui::layout::Direction::Vertical,margin:ratatui::layout::Margin::new(1,0)) {
        settings::SettingsForm<usize>(..settings::FormProps {rows:Arc::new(rows),selected,focused:true,active:picker.get().is_none(),identity:0,keyboard_revision:keyboard_revision.get(),editor})
        if !snapshot.hex_input.is_empty() {
            View(height:Constraint::Length(1)) {Text(text:detail)}
        }
        View(height:Constraint::Length(1),flex_direction:ratatui::layout::Direction::Horizontal,gap:2) {
            settings::SettingsButton(..settings::ButtonProps {label:"保存 · Enter".into(),on_press:save})
            settings::SettingsButton(..settings::ButtonProps {label:"取消 · Esc".into(),on_press:cancel})
        }
        if let Some((index,options,initial)) = choice {
            settings::SettingsPicker(..settings::PickerProps {
                colors:if index==1 {None} else {Some(Arc::new(CUSTOM_COLORS.iter().map(|c|c.color).collect()))},
                title:if index==1 {"选择基础主题".into()} else {PALETTE_FIELDS[index-2].label.into()},options,initial,
                on_select:settings::callback(move |choice:usize| {
                    let mut next=draft.read().clone();
                    if index==1 {next.change_base(choice);} else {next.set_color(index-2,CUSTOM_COLORS[choice].color);next.hex_input.clear();}
                    draft.set(next);picker.set(None);
                }),on_cancel:settings::callback(move |_|picker.set(None)),
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_base_preserves_only_explicit_edits_after_round_trip() {
        let mut draft = CustomDraft::default();
        draft.change_base(0);
        let edited = Color::Rgb(18, 52, 86);
        draft.set_color(0, edited);
        draft.change_base(1);
        let theme = draft.theme();
        assert_eq!(theme.colors.len(), 1);
        let reloaded: CustomTheme = toml::from_str(&toml::to_string(&theme).unwrap()).unwrap();
        let palette = reloaded.palette();
        assert_eq!(palette_field(&palette, "fg"), Some(edited));
        assert_eq!(
            palette_field(&palette, "fg_dim"),
            palette_field(&ThemeChoice::all()[1].palette(), "fg_dim")
        );
    }

    fn field(key: &str) -> usize {
        PALETTE_FIELDS.iter().position(|f| f.key == key).unwrap()
    }
    #[test]
    fn inherited_preview_uses_the_same_derived_colors_as_saved_theme() {
        let mut draft = CustomDraft::default();
        draft.change_base(0);
        draft.set_color(field("accent"), Color::Rgb(18, 52, 86));
        draft.change_base(1);
        let palette = draft.theme().palette();
        for key in ["on_accent", "border_active"] {
            assert_eq!(Some(draft.colors[field(key)]), palette_field(&palette, key));
        }
    }
    #[test]
    fn explicit_derived_color_equal_to_base_survives_save_and_unchanged_edit() {
        let mut draft = CustomDraft::default();
        draft.change_base(0);
        let original_border = draft.colors[field("border_active")];
        draft.set_color(field("accent"), Color::Rgb(18, 52, 86));
        draft.set_color(field("border_active"), original_border);
        let mut theme = draft.theme();
        theme.slug = "custom-1".into();
        let expected = theme.colors.clone();
        assert!(expected.contains_key("border_active"));
        let config = AppearanceConfig {
            theme_slug: theme.slug.clone(),
            custom_themes: vec![theme],
            ..Default::default()
        };
        let reopened = CustomDraft::from_config(&config, Some("custom-1"));
        assert_eq!(reopened.theme().colors, expected);
        assert_eq!(reopened.theme().palette().border_active, original_border);
    }
}
