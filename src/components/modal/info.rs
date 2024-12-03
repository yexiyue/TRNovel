use std::ops::{Deref, DerefMut};

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Margin, Size},
    style::Stylize,
    widgets::{Block, Clear, Row, StatefulWidget, Table, Widget},
};
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};

#[derive(Debug, Clone)]
pub struct ShortcutInfoState {
    pub show: bool,
    pub state: ScrollViewState,
}

impl ShortcutInfoState {
    pub fn new() -> Self {
        Self {
            show: false,
            state: ScrollViewState::default(),
        }
    }
}

impl ShortcutInfoState {
    pub fn toggle(&mut self) {
        self.show = !self.show;
    }
}

impl Deref for ShortcutInfoState {
    type Target = ScrollViewState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for ShortcutInfoState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

pub struct KeyShortcutInfo(pub Vec<(String, String)>);

impl KeyShortcutInfo {
    pub fn new(data: Vec<(&str, &str)>) -> Self {
        Self(
            data.into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    pub fn rows(&self) -> Vec<Row<'_>> {
        self.0
            .iter()
            .map(|(key, info)| Row::new(vec![key.as_str(), info.as_str()]))
            .collect::<Vec<_>>()
    }
}

impl From<Vec<(&str, &str)>> for KeyShortcutInfo {
    fn from(data: Vec<(&str, &str)>) -> Self {
        Self::new(data)
    }
}

pub struct ShortcutInfo {
    pub key_shortcut_info: KeyShortcutInfo,
    pub global_shortcut_info: KeyShortcutInfo,
}

impl ShortcutInfo {
    pub fn new(key_shortcut_info: KeyShortcutInfo) -> Self {
        let global_shortcut_info =
            KeyShortcutInfo::new(vec![("q", "退出程序"), ("g", "前进"), ("b", "后退")]);
        Self {
            key_shortcut_info,
            global_shortcut_info,
        }
    }

    fn height(&self) -> u16 {
        let global_height = self.global_shortcut_info.rows().len() as u16;
        let key_height = self.key_shortcut_info.rows().len() as u16;
        global_height + key_height + 3 * 2
    }

    fn render_widgets_into_scrollview(&self, buf: &mut Buffer) {
        let area = buf.area.inner(Margin::new(1, 0));

        let [top, bottom] = Layout::vertical([
            Constraint::Length((self.global_shortcut_info.rows().len() + 3) as u16),
            Constraint::Length((self.key_shortcut_info.rows().len() + 3) as u16),
        ])
        .areas(area);

        let widths = [Constraint::Min(20), Constraint::Fill(1)];
        let header = Row::new(vec!["快捷键", "描述"])
            .yellow()
            .bold()
            .bottom_margin(1);

        let table = Table::new(self.global_shortcut_info.rows(), widths)
            .header(header.clone())
            .column_spacing(1)
            .block(Block::bordered().not_dim().title("全局快捷键"));
        Widget::render(table, top, buf);

        let table = Table::new(self.key_shortcut_info.rows(), widths)
            .header(header)
            .block(Block::bordered().not_dim().title("当前页面快捷键"));
        Widget::render(table, bottom, buf);
    }
}

impl StatefulWidget for ShortcutInfo {
    type State = ShortcutInfoState;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        if !state.show {
            return;
        }

        Block::new().dim().render(area, buf);

        let [vertical] = Layout::vertical([Constraint::Percentage(80)])
            .flex(Flex::Center)
            .areas(area);
        let [horizontal] = Layout::horizontal([Constraint::Percentage(80)])
            .flex(Flex::Center)
            .areas(vertical);

        Clear.render(horizontal, buf);

        let mut scroll_view = ScrollView::new(Size::new(horizontal.width, self.height()))
            .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);
        self.render_widgets_into_scrollview(scroll_view.buf_mut());

        scroll_view.render(horizontal, buf, &mut state.state);
    }
}
