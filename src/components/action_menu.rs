//! Page actions register near their state owner and render in the shared header.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use rat_menu::{
    menuitem::MenuItem,
    popup_menu::{PopupConstraint, PopupMenu, PopupMenuState},
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Offset, Rect},
    widgets::{Block, StatefulWidget, Widget},
};
use ratatui_kit::prelude::*;

use crate::theme::AppChromeTheme;

pub type MenuCallback = Arc<Mutex<Handler<'static, ()>>>;
#[derive(Clone)]
pub struct MenuEntry {
    pub group: String,
    pub label: String,
    pub shortcut: String,
    pub disabled: bool,
    pub callback: MenuCallback,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuRequest {
    First,
    More,
}
pub static HEADER_MENU_REQUEST: Atom<Option<MenuRequest>> = Atom::new(|| None);

pub static ENTRIES: Atom<BTreeMap<u64, MenuEntry>> = Atom::new(BTreeMap::new);
#[derive(Clone)]
pub struct MenuGroup(pub String);

#[derive(Default, Props)]
pub struct AppMenusProps {
    pub start_x: u16,
}

#[component]
pub fn AppMenus(props: &mut AppMenusProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (terminal_width, _) = hooks.use_terminal_size();
    let entries = hooks.use_atom(&ENTRIES);
    let mut headers = hooks.use_atom(&HEADERS);
    let mut groups = Vec::new();
    for entry in entries.read().values() {
        if !groups.contains(&entry.group) {
            groups.push(entry.group.clone());
        }
    }
    // Stable order even when async content mounts later.
    groups.sort_by_key(|name| match name.as_str() {
        "阅读" => 0,
        "听书" => 1,
        "更多" => 3,
        _ => 2,
    });
    let mut x = props.start_x;
    let mut positioned = Vec::new();
    for name in groups {
        let width = (ratatui::text::Line::from(name.clone()).width() + 4) as u16;
        if name == "更多" {
            x = x.max(terminal_width.saturating_sub(width));
        }
        positioned.push((x, name));
        x += width;
    }
    let mut request = hooks.use_atom(&HEADER_MENU_REQUEST);
    let mut active = hooks.use_atom(&ACTIVE_MENU);
    let requested = request.get();
    let target = positioned
        .iter()
        .find(|(_, name)| requested != Some(MenuRequest::More) || name == "更多")
        .map(|(x, name)| ActiveMenu {
            name: name.clone(),
            x: *x,
        });
    hooks.use_effect(
        move || {
            if requested.is_some() {
                active.set(target.clone());
                request.set(None);
            }
        },
        requested,
    );
    let hit_areas: Vec<_> = positioned
        .iter()
        .map(|(x, name)| Header {
            name: name.clone(),
            area: Rect::new(
                *x,
                0,
                (ratatui::text::Line::from(name.clone()).width() + 4) as u16,
                1,
            ),
        })
        .collect();
    hooks.use_effect(
        {
            let hit_areas = hit_areas.clone();
            move || {
                headers.set(hit_areas.clone());
            }
        },
        hit_areas,
    );
    element!(View(height: Constraint::Length(1), flex_direction: ratatui::layout::Direction::Horizontal) {
        for (index, (x, name)) in positioned.into_iter().enumerate() {
            if name == "更多" { View(width: Constraint::Fill(1)) }
            ActionMenu(key: name.clone(), label: name.clone(), x: x, first: index == 0)
        }
    })
}

#[derive(Default, Props)]
pub struct ActionMenuProps {
    pub label: String,
    pub x: u16,
    pub first: bool,
}

#[derive(Clone)]
struct ActiveMenu {
    name: String,
    x: u16,
}
#[derive(Clone, PartialEq)]
struct Header {
    name: String,
    area: Rect,
}
static HEADERS: Atom<Vec<Header>> = Atom::new(Vec::new);
static ACTIVE_MENU: Atom<Option<ActiveMenu>> = Atom::new(|| None);

#[component]
fn ActionMenu(props: &mut ActionMenuProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut active = hooks.use_atom(&ACTIVE_MENU);
    let name = props.label.clone();
    let x = props.x;
    let first = props.first;
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let name = name.clone(); move |event| {
            if first && matches!(event, Event::Key(key) if key.kind == KeyEventKind::Press && key.code == KeyCode::F(10)) {
                active.set(Some(ActiveMenu { name: name.clone(), x })); EventResult::Consumed
            } else { EventResult::Ignored }
        }
    });
    element!(super::ActionButton(label: format!("{name} ▾"), shortcut: "F10 打开菜单 · ↑/↓ 选择 · Enter 执行 · Esc 关闭", on_press: move |_| {
        active.set(Some(ActiveMenu { name: name.clone(), x }));
    }))
}

/// Render the floating surface at the app root, outside the header's one-row clip.
#[component]
pub fn AppMenuPopup(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut active = hooks.use_atom(&ACTIVE_MENU);
    let entries = hooks.use_atom(&ENTRIES);
    let current = active.read().clone();
    let items: Vec<_> = entries
        .read()
        .values()
        .filter(|entry| {
            current
                .as_ref()
                .is_some_and(|menu| entry.group == menu.name)
        })
        .cloned()
        .collect();
    let is_open = current.is_some() && !items.is_empty();
    let mut selected = hooks.use_state(|| None::<usize>);
    let mut pressed = hooks.use_state(|| None::<usize>);
    let geometry = hooks
        .use_state(|| Arc::new(Mutex::new(Vec::<Rect>::new())))
        .read()
        .clone();
    let layer = hooks.use_input_layer(is_open, true);
    let (width, _) = hooks.use_terminal_size();
    let first_enabled = items.iter().position(|entry| !entry.disabled);
    hooks.use_effect(
        move || {
            selected.set(first_enabled);
            pressed.set(None);
        },
        current.as_ref().map(|menu| menu.name.clone()),
    );
    let event_items = items.clone();
    let event_headers = hooks.use_atom(&HEADERS).read().clone();
    let current_name = current.as_ref().map(|menu| menu.name.clone());
    let event_geometry = geometry.clone();
    hooks.use_event_handler_with_options(EventScope::Layer(layer), EventPriority::Normal,
        EventOptions { hit_test: false }, move |event| {
        if !is_open { return EventResult::Ignored; }
        if matches!(event, Event::Key(key) if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc) {
            active.set(None);
            return EventResult::Consumed;
        }
        let activate = match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Down | KeyCode::Char('j') => { selected.set(next_enabled(&event_items, selected.get(), false)); None }
                KeyCode::Up | KeyCode::Char('k') => { selected.set(next_enabled(&event_items, selected.get(), true)); None }
                KeyCode::Left | KeyCode::Right => {
                    if let Some(index) = event_headers.iter().position(|header| Some(&header.name) == current_name.as_ref()) {
                        let next = if key.code == KeyCode::Left { (index + event_headers.len() - 1) % event_headers.len() } else { (index + 1) % event_headers.len() };
                        let header = &event_headers[next];
                        active.set(Some(ActiveMenu { name: header.name.clone(), x: header.area.x }));
                    }
                    None
                }
                KeyCode::Enter => selected.get(),
                _ => None,
            },
            Event::Mouse(mouse) => {
                if let Some(header) = event_headers.iter().find(|header| header.area.contains((mouse.column, mouse.row).into()))
                    && matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Down(MouseButton::Left)) {
                        if Some(&header.name) != current_name.as_ref() {
                            active.set(Some(ActiveMenu { name: header.name.clone(), x: header.area.x }));
                        } else if mouse.kind == MouseEventKind::Down(MouseButton::Left) { active.set(None); }
                        return EventResult::Consumed;
                }
                let index = event_geometry.lock().unwrap_or_else(|e| e.into_inner()).iter()
                    .position(|area| area.contains((mouse.column, mouse.row).into()));
                match mouse.kind {
                    MouseEventKind::Moved => {
                        if let Some(index) = index.filter(|index| event_items.get(*index).is_some_and(|item| !item.disabled)) { selected.set(Some(index)); }
                        None
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        pressed.set(index);
                        if index.is_none() { active.set(None); }
                        None
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        let activated = index.filter(|index| pressed.get() == Some(*index));
                        pressed.set(None);
                        activated
                    }
                    MouseEventKind::ScrollDown => { selected.set(next_enabled(&event_items, selected.get(), false)); None }
                    MouseEventKind::ScrollUp => { selected.set(next_enabled(&event_items, selected.get(), true)); None }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(item) = activate.and_then(|index| event_items.get(index)).filter(|item| !item.disabled) {
            active.set(None);
            item.callback.lock().unwrap_or_else(|e| e.into_inner())(());
        }
        // The exclusive layer must swallow keys/clicks not understood by the menu.
        EventResult::Consumed
    });
    let mut widget = PopupMenu::new();
    for entry in &items {
        widget = widget.item(MenuItem {
            item: entry.label.clone().into(),
            right: entry.shortcut.clone().into(),
            disabled: entry.disabled,
            ..Default::default()
        });
    }
    let widget = widget
        .block(Block::bordered().border_style(theme.title))
        .style(theme.text)
        .highlight_style(theme.highlight.reversed())
        .focus_style(theme.highlight.reversed())
        .disabled_style(theme.meta_label.dim());
    let popup_width = widget.width().min(width);
    let popup_x = current
        .as_ref()
        .map(|menu| menu.x)
        .unwrap_or(0)
        .min(width.saturating_sub(popup_width));
    element!(Modal(open: is_open, layer: layer, placement: Placement::TopLeft,
        offset: Offset::new(popup_x as i32, 1), width: Constraint::Length(popup_width), height: Constraint::Length(widget.height())) {
        View(width: Constraint::Fill(1), height: Constraint::Fill(1)) {
            widget(MenuWidget { menu: widget, selected: selected.get(), geometry })
        }
    })
}

// rat-menu's state contains Rc/Cell focus flags and cannot live in Send + Sync hooks.
// Keep it local to drawing; only selection and exact rendered hit regions cross frames.
#[derive(Clone)]
struct MenuWidget {
    menu: PopupMenu<'static>,
    selected: Option<usize>,
    geometry: Arc<Mutex<Vec<Rect>>>,
}
impl Widget for &MenuWidget {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        // A CJK glyph immediately left of the popup can extend into its border.
        // Clear that glyph's head so terminal diffing does not skip the border cell.
        for y in area.y..area.bottom() {
            if area.x > buffer.area.x
                && let Some(cell) = buffer.cell_mut((area.x - 1, y))
                && ratatui::text::Line::from(cell.symbol()).width() > 1
            {
                cell.reset();
            }
        }
        let mut state = PopupMenuState::default();
        state.set_active(true);
        state.selected = self.selected;
        self.menu
            .clone()
            .constraint(PopupConstraint::Position(area.x, area.y))
            .boundary(area)
            .render(area, buffer, &mut state);
        *self.geometry.lock().unwrap_or_else(|e| e.into_inner()) = state.item_areas;
    }
}
fn next_enabled(items: &[MenuEntry], current: Option<usize>, backwards: bool) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let start = current.unwrap_or(if backwards { 0 } else { items.len() - 1 });
    (1..=items.len())
        .map(|step| {
            if backwards {
                (start + items.len() - step) % items.len()
            } else {
                (start + step) % items.len()
            }
        })
        .find(|index| !items[*index].disabled)
}

impl Widget for MenuWidget {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        (&self).render(area, buffer);
    }
}

pub fn menu_is_open(hooks: &mut Hooks) -> bool {
    hooks.use_atom(&ACTIVE_MENU).read().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(disabled: bool) -> MenuEntry {
        MenuEntry {
            group: "阅读".into(),
            label: "搜索正文".into(),
            shortcut: "Ctrl+F".into(),
            disabled,
            callback: Arc::new(Mutex::new(Handler::default())),
        }
    }
    #[test]
    fn keyboard_skips_disabled_items_and_wraps() {
        let items = [entry(true), entry(false), entry(true), entry(false)];
        assert_eq!(next_enabled(&items, None, false), Some(1));
        assert_eq!(next_enabled(&items, Some(1), false), Some(3));
        assert_eq!(next_enabled(&items, Some(3), false), Some(1));
        assert_eq!(next_enabled(&items, Some(1), true), Some(3));
        assert_eq!(next_enabled(&[entry(true)], None, false), None);
        assert_eq!(next_enabled(&[], None, false), None);
    }
    #[test]
    fn rendered_hit_regions_match_popup_and_stay_in_bounds() {
        let area = Rect::new(3, 1, 24, 5);
        let geometry = Arc::new(Mutex::new(Vec::new()));
        let menu = PopupMenu::new()
            .item(MenuItem {
                item: "搜索正文".into(),
                right: "Ctrl+F".into(),
                ..Default::default()
            })
            .block(Block::bordered());
        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 10));
        buffer.set_string(
            area.x - 1,
            area.y + 1,
            "小说正文",
            ratatui::style::Style::default(),
        );
        MenuWidget {
            menu,
            selected: Some(0),
            geometry: geometry.clone(),
        }
        .render(area, &mut buffer);
        let regions = geometry.lock().unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(buffer[(area.x - 1, area.y + 1)].symbol(), " ");
        assert_eq!(buffer[(area.x, area.y + 1)].symbol(), "│");
        assert!(area.contains(regions[0].as_position()));
        assert!(regions[0].right() <= area.right());
        let line: String = (area.x..area.right())
            .map(|x| buffer[(x, regions[0].y)].symbol())
            .collect();
        assert!(line.replace(' ', "").contains("搜索正文"));
        assert!(line.contains("Ctrl+F"));
    }
}
