use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Margin, Size},
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Row, StatefulWidget, Table, Widget},
};
use std::ops::{Deref, DerefMut};
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};

#[derive(Debug, Clone, Default)]
pub struct ShortcutInfoState {
    pub show: bool,
    pub state: ScrollViewState,
}

impl ShortcutInfoState {
    pub fn new() -> Self {
        Self::default()
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

#[derive(Debug, Clone, Default)]
pub struct KeyShortcutInfo(pub Vec<(String, String)>);

impl Deref for KeyShortcutInfo {
    type Target = Vec<(String, String)>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for KeyShortcutInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<KeyShortcutInfo> for Vec<(String, String)> {
    fn from(value: KeyShortcutInfo) -> Self {
        value.0
    }
}

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
        let global_shortcut_info = KeyShortcutInfo::new(vec![
            ("退出程序", "Q / Ctrl + C"),
            ("后退", "B"),
            ("后退(不保存记录)", "Backspace"),
            ("前进", "G"),
            ("显示/隐藏快捷键信息", "I"),
        ]);
        Self {
            key_shortcut_info,
            global_shortcut_info,
        }
    }

    fn height(&self) -> u16 {
        let global_height = self.global_shortcut_info.0.len() as u16;
        let key_height = self.key_shortcut_info.0.len() as u16;
        global_height + key_height + 3 * 2
    }

    fn render_widgets_into_scrollview(&self, buf: &mut Buffer) {
        let area = buf.area.inner(Margin::new(1, 0));

        let [top, bottom] = Layout::vertical([
            Constraint::Length((self.key_shortcut_info.rows().len() + 3) as u16),
            Constraint::Length((self.global_shortcut_info.rows().len() + 3) as u16),
        ])
        .areas(area);

        let widths = [Constraint::Fill(1), Constraint::Fill(1)];
        let header = Row::new(vec!["描述", "快捷键"]).light_cyan().bold();

        let table = Table::new(self.key_shortcut_info.rows(), widths)
            .header(header.clone())
            .block(
                Block::bordered()
                    .not_dim()
                    .title(Line::from("当前页").centered().dim()),
            );
        Widget::render(table, top, buf);

        let table = Table::new(self.global_shortcut_info.rows(), widths)
            .header(header)
            .block(
                Block::bordered()
                    .not_dim()
                    .title(Line::from("全局").centered().dim()),
            );
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
        let [horizontal] = Layout::horizontal([Constraint::Length(50)])
            .flex(Flex::Center)
            .areas(vertical);

        Clear.render(horizontal, buf);
        let horizontal = horizontal.inner(Margin::new(2, 1));

        let mut scroll_view = ScrollView::new(Size::new(horizontal.width, self.height()))
            .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

        self.render_widgets_into_scrollview(scroll_view.buf_mut());

        scroll_view.render(horizontal, buf, &mut state.state);
    }
}
