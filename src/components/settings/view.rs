use super::{Control, Focus, Row};
use crate::theme::AppChromeTheme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Widget},
};
use std::sync::{Arc, Mutex};
pub type FrameArea = Arc<Mutex<Rect>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Row(usize),
    Decrease(usize),
    Increase(usize),
    Activate(usize),
}
impl Hit {
    pub fn row(self) -> usize {
        match self {
            Self::Row(i) | Self::Decrease(i) | Self::Increase(i) | Self::Activate(i) => i,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Entry {
    Group(&'static str),
    Space,
    Field(usize),
}

pub fn entries<K>(rows: &[Row<K>]) -> Vec<Entry> {
    let mut result = Vec::new();
    let mut previous = "";
    for (index, row) in rows.iter().enumerate() {
        if previous != row.group {
            if !result.is_empty() {
                result.push(Entry::Space);
            }
            result.push(Entry::Group(row.group));
            previous = row.group;
        }
        result.push(Entry::Field(index));
    }
    result
}

pub fn reveal(entries: &[Entry], cursor: usize, offset: usize, height: usize) -> usize {
    if cursor == 0 && height > 1 {
        return 0;
    }
    let line = entries
        .iter()
        .position(|e| matches!(e,Entry::Field(i) if *i==cursor))
        .unwrap_or(0);
    let height = height.max(1);
    let next = if line < offset {
        line
    } else if line >= offset + height {
        line + 1 - height
    } else {
        offset
    };
    next.min(entries.len().saturating_sub(height))
}

pub fn tab_areas(area: Rect, count: usize) -> Vec<Rect> {
    Layout::horizontal(vec![Constraint::Length(8); count])
        .split(area)
        .to_vec()
}
pub fn tab_at(area: Rect, count: usize, position: Position) -> Option<usize> {
    tab_areas(area, count)
        .iter()
        .position(|r| r.contains(position))
}

pub fn control_area(area: Rect, control: Control) -> Rect {
    let available = area.width.saturating_sub(4);
    let desired = match control {
        Control::Toggle(_) => 8,
        _ => 30,
    };
    let width = desired.min(available / 2);
    Rect::new(
        area.right().saturating_sub(width + 2),
        area.y,
        width,
        area.height.min(1),
    )
}
fn stepper_areas(area: Rect) -> [Rect; 3] {
    Layout::horizontal([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(area)
}

pub fn hit<K>(
    area: Rect,
    list: &[Entry],
    offset: usize,
    rows: &[Row<K>],
    position: Position,
) -> Option<Hit> {
    // 最后一列留给框架滚动条,不参与控件命中。
    let content = Rect {
        width: area.width.saturating_sub(1),
        ..area
    };
    if !content.contains(position) {
        return None;
    }
    let Entry::Field(index) = *list.get(offset + usize::from(position.y - area.y))? else {
        return None;
    };
    let row = &rows[index];
    let controls = control_area(Rect::new(area.x, position.y, area.width, 1), row.control);
    if controls.contains(position) {
        match row.control {
            Control::Number { decrease, increase } => {
                let [left, _, right] = stepper_areas(controls);
                if decrease && left.contains(position) {
                    return Some(Hit::Decrease(index));
                }
                if increase && right.contains(position) {
                    return Some(Hit::Increase(index));
                }
            }
            Control::Toggle(_) | Control::Choice | Control::Action => {
                return Some(Hit::Activate(index));
            }
            Control::Link => {
                let text_width =
                    Line::from(shorten_middle(&row.value, controls.width)).width() as u16;
                if position.x < controls.x + text_width {
                    return Some(Hit::Activate(index));
                }
            }
            _ => {}
        }
    }
    Some(Hit::Row(index))
}

#[derive(Clone)]
pub struct TabsWidget {
    pub labels: Arc<Vec<String>>,
    pub area: FrameArea,
    pub section: usize,
    pub focus: Focus,
    pub hover: Option<usize>,
    pub theme: AppChromeTheme,
}
impl Widget for TabsWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        *self.area.lock().unwrap_or_else(|e| e.into_inner()) = area;
        for (index, rect) in tab_areas(area, self.labels.len()).into_iter().enumerate() {
            let active = index == self.section;
            let style = if self.hover == Some(index) {
                self.theme.selected
            } else if active {
                self.theme.highlight
            } else {
                self.theme.muted
            };
            let label = if active && self.focus == Focus::Tabs {
                format!("› {}", self.labels[index])
            } else {
                self.labels[index].to_owned()
            };
            Line::styled(label, style)
                .alignment(Alignment::Center)
                .render(
                    Rect {
                        height: rect.height.min(1),
                        ..rect
                    },
                    buf,
                );
            if active && rect.height > 1 {
                Line::styled("━".repeat(usize::from(rect.width)), self.theme.highlight)
                    .render(Rect::new(rect.x, rect.y + 1, rect.width, 1), buf);
            }
        }
    }
}

#[derive(Clone)]
pub struct FormWidget<K> {
    pub area: FrameArea,
    pub rows: Arc<Vec<Row<K>>>,
    pub list: Arc<Vec<Entry>>,
    pub cursor: usize,
    pub focus: Focus,
    pub offset: usize,
    pub hover: Option<Hit>,
    pub theme: AppChromeTheme,
}
impl<K: Clone> Widget for FormWidget<K> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        *self.area.lock().unwrap_or_else(|e| e.into_inner()) = area;
        for (y, entry) in self
            .list
            .iter()
            .skip(self.offset)
            .take(usize::from(area.height))
            .enumerate()
        {
            let rect = Rect::new(area.x, area.y + y as u16, area.width, 1);
            match *entry {
                Entry::Space => {}
                Entry::Group(label) => {
                    Line::styled(label, self.theme.muted.add_modifier(Modifier::BOLD))
                        .render(rect, buf)
                }
                Entry::Field(index) => {
                    let row = &self.rows[index];
                    let focused = self.focus == Focus::Form && self.cursor == index;
                    let hovered = self.hover.is_some_and(|h| h.row() == index);
                    let style = if hovered {
                        self.theme.highlight
                    } else {
                        self.theme.text
                    };
                    if focused {
                        Line::styled("›", self.theme.highlight).render(
                            Rect {
                                width: rect.width.min(1),
                                ..rect
                            },
                            buf,
                        );
                    }
                    let controls = control_area(rect, row.control);
                    let label = Rect::new(
                        rect.x.saturating_add(2),
                        rect.y,
                        controls.x.saturating_sub(rect.x + 3),
                        1,
                    )
                    .intersection(rect);
                    let mut spans = vec![ratatui::text::Span::styled(&row.label, style)];
                    if let Some(color) = row.swatch {
                        spans.push(ratatui::text::Span::styled(" ██", Style::new().fg(color)));
                    }
                    Line::from(spans).render(label, buf);
                    let base = if focused {
                        self.theme.highlight
                    } else {
                        self.theme.text
                    };
                    match row.control {
                        Control::Number { decrease, increase } => {
                            let [left, value, right] = stepper_areas(controls);
                            draw_control(
                                "[-]",
                                left,
                                decrease,
                                self.hover == Some(Hit::Decrease(index)),
                                base,
                                self.theme,
                                buf,
                            );
                            Line::styled(&row.value, base)
                                .alignment(Alignment::Center)
                                .render(value, buf);
                            draw_control(
                                "[+]",
                                right,
                                increase,
                                self.hover == Some(Hit::Increase(index)),
                                base,
                                self.theme,
                                buf,
                            );
                        }
                        Control::Toggle(value) => draw_control(
                            if value { "[开]" } else { "[关]" },
                            controls,
                            true,
                            self.hover == Some(Hit::Activate(index)),
                            base,
                            self.theme,
                            buf,
                        ),
                        Control::Choice | Control::Action => {
                            let text = format!(
                                "{} {}",
                                row.value,
                                if row.control == Control::Choice {
                                    "▾"
                                } else {
                                    "›"
                                }
                            );
                            draw_control(
                                &text,
                                controls,
                                true,
                                self.hover == Some(Hit::Activate(index)),
                                base,
                                self.theme,
                                buf,
                            );
                        }
                        Control::Link => {
                            let style = if self.hover == Some(Hit::Activate(index)) {
                                self.theme.selected
                            } else {
                                self.theme.title
                            }
                            .add_modifier(Modifier::UNDERLINED);
                            Line::styled(shorten_middle(&row.value, controls.width), style)
                                .render(controls, buf);
                        }
                        Control::ReadOnly => {
                            Line::styled(&row.value, self.theme.text).render(controls, buf)
                        }
                    }
                }
            }
        }
    }
}
fn shorten_middle(text: &str, width: u16) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if text.width() <= usize::from(width) {
        return text.to_owned();
    }
    let budget = usize::from(width).saturating_sub(1);
    let suffix_budget = budget * 2 / 3;
    let mut suffix = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > suffix_budget {
            break;
        }
        suffix.push(c);
        used += w;
    }
    let mut prefix = String::new();
    let mut used_prefix = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used_prefix + w > budget - used {
            break;
        }
        prefix.push(c);
        used_prefix += w;
    }
    format!("{prefix}…{}", suffix.into_iter().rev().collect::<String>())
}

fn draw_control(
    text: &str,
    area: Rect,
    enabled: bool,
    hovered: bool,
    base: Style,
    theme: AppChromeTheme,
    buf: &mut Buffer,
) {
    let style = if !enabled {
        theme.muted.add_modifier(Modifier::DIM)
    } else if hovered {
        theme.selected
    } else {
        base
    };
    buf.set_style(area, style);
    Line::styled(text, style)
        .alignment(Alignment::Center)
        .render(area, buf);
}

#[derive(Clone)]
pub struct PickerWidget {
    pub colors: Option<Arc<Vec<ratatui::style::Color>>>,
    pub offset: usize,
    pub area: FrameArea,
    pub title: String,
    pub options: Vec<String>,
    pub selected: usize,
    pub hover: Option<usize>,
    pub theme: AppChromeTheme,
}
impl Widget for PickerWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        *self.area.lock().unwrap_or_else(|e| e.into_inner()) = area;
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title.as_str())
            .border_style(self.theme.border);
        let inner = block.inner(area);
        block.render(area, buf);
        if self.options.is_empty() {
            Line::styled("没有匹配结果，请调整关键词", self.theme.meta_label).render(inner, buf);
        }
        for (visual, (i, option)) in self
            .options
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(usize::from(inner.height))
            .enumerate()
        {
            let style = if i == self.selected {
                self.theme.selected
            } else if self.hover == Some(i) {
                self.theme.highlight
            } else {
                self.theme.text
            };
            let line = format!("{} {}", if i == self.selected { "›" } else { " " }, option);
            let mut spans = vec![ratatui::text::Span::styled(line, style)];
            if let Some(color) = self.colors.as_ref().and_then(|colors| colors.get(i)) {
                spans.push(ratatui::text::Span::styled(
                    " ██",
                    ratatui::style::Style::new().fg(*color),
                ));
            }
            Line::from(spans).render(
                Rect::new(inner.x, inner.y + visual as u16, inner.width, 1),
                buf,
            );
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    fn rows() -> Vec<Row<()>> {
        (0..8)
            .map(|i| Row {
                swatch: None,
                key: (),
                group: if i < 4 { "排版" } else { "界面" },
                label: format!("条目{i}"),
                value: "0".into(),
                detail: String::new(),
                control: Control::Number {
                    decrease: false,
                    increase: true,
                },
            })
            .collect()
    }
    #[test]
    fn pointer_hits_only_visible_controls_and_not_spacing_or_scrollbar() {
        let rows = rows();
        let list = entries(&rows);
        let area = Rect::new(3, 5, 88, 4);
        assert_eq!(hit(area, &list, 0, &rows, Position::new(10, 5)), None);
        assert_eq!(
            hit(area, &list, 0, &rows, Position::new(6, 6)),
            Some(Hit::Row(0))
        );
        let controls = control_area(Rect::new(3, 6, 88, 1), rows[0].control);
        let [left, _, right] = stepper_areas(controls);
        assert_eq!(
            hit(area, &list, 0, &rows, Position::new(right.x, 6)),
            Some(Hit::Increase(0))
        );
        assert_eq!(
            hit(area, &list, 0, &rows, Position::new(left.x, 6)),
            Some(Hit::Row(0))
        );
        assert_eq!(hit(area, &list, 0, &rows, Position::new(90, 6)), None);
        assert_eq!(hit(area, &list, 0, &rows, Position::new(right.x, 9)), None);
    }
    #[test]
    fn focus_reveal_handles_last_item_and_resizing() {
        let rows = rows();
        let list = entries(&rows);
        let offset = reveal(&list, rows.len() - 1, 0, 5);
        assert_eq!(offset, list.len() - 5);
        assert_eq!(reveal(&list, 0, offset, 5), 0);
        assert_eq!(reveal(&list, 0, offset, 100), 0);
        assert_eq!(reveal(&list, 0, offset, 1), 1);
    }

    #[test]
    fn hover_feedback_does_not_replace_the_keyboard_focus_or_active_tab() {
        let area = Rect::new(0, 0, 88, 8);
        let theme = AppChromeTheme::default();
        let mut buffer = Buffer::empty(area);
        let geometry = Arc::default();
        TabsWidget {
            labels: Arc::new(vec![
                "阅读".into(),
                "外观".into(),
                "键位".into(),
                "数据".into(),
                "关于".into(),
            ]),
            area: Arc::clone(&geometry),
            section: 0,
            focus: Focus::Tabs,
            hover: Some(1),
            theme,
        }
        .render(area, &mut buffer);
        assert_eq!(*geometry.lock().unwrap(), area);
        assert_eq!(buffer[(0, 1)].symbol(), "━");
        assert_eq!(buffer[(10, 0)].style().bg, theme.selected.bg);
        assert_ne!(buffer[(0, 0)].style().bg, theme.selected.bg);

        let rows = Arc::new(rows());
        let list = Arc::new(entries(&rows));
        let field_area = Rect::new(0, 1, area.width, 1);
        let [_, _, plus] = stepper_areas(control_area(field_area, rows[0].control));
        FormWidget {
            area: Arc::default(),
            rows,
            list,
            cursor: 1,
            focus: Focus::Form,
            offset: 0,
            hover: Some(Hit::Increase(0)),
            theme,
        }
        .render(area, &mut buffer);
        assert_eq!(buffer[(0, 2)].symbol(), "›");
        assert_eq!(buffer[(plus.x, 1)].style().bg, theme.selected.bg);
    }
    #[test]
    fn rendering_and_hit_testing_stay_in_bounds_at_small_sizes() {
        let rows = Arc::new(rows());
        let list = Arc::new(entries(&rows));
        for width in [0, 1, 12, 40, 80, 120] {
            for height in [0, 1, 3, 18, 30] {
                let area = Rect::new(0, 0, width, height);
                let mut buffer = Buffer::empty(area);
                (FormWidget {
                    area: Arc::default(),
                    rows: rows.clone(),
                    list: list.clone(),
                    cursor: 0,
                    focus: Focus::Form,
                    offset: 0,
                    hover: None,
                    theme: Default::default(),
                })
                .render(area, &mut buffer);
                (TabsWidget {
                    labels: Arc::new(vec!["阅读".into(), "外观".into()]),
                    area: Arc::default(),
                    section: 0,
                    focus: Focus::Tabs,
                    hover: None,
                    theme: Default::default(),
                })
                .render(area, &mut buffer);
            }
        }
    }
}
