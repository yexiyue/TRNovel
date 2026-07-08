use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    text::Line,
    widgets::{Block, Scrollbar},
};
use ratatui_kit::prelude::*;
use tui_tree_widget::{TreeItem, TreeState};

use crate::{components::search_input::SearchInput, novel::VolumeMarker, theme::AppChromeTheme};

/// 章节项：`(标题, 扁平章节索引)`。扁平索引同时是它在章节列表中的位置。
#[derive(Default, Clone)]
pub struct ChapterName(pub String, pub usize);

impl From<(String, usize)> for ChapterName {
    fn from(value: (String, usize)) -> Self {
        Self(value.0, value.1)
    }
}

/// 折叠树节点标识：卷节点用卷索引，章节叶子用扁平章节索引。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TocId {
    Volume(usize),
    Chapter(usize),
}

#[derive(Default, Props)]
pub struct SelectChapterProps {
    pub is_editing: bool,
    pub chapters: Vec<ChapterName>,
    /// 分卷元数据，空表示无分卷（章节平铺在根层级）。
    pub volumes: Vec<VolumeMarker>,
    pub on_select: Handler<'static, usize>,
    pub default_value: Option<usize>,
}

/// 定位某个扁平章节索引所属的卷（最后一个 `first_chapter_index <= idx` 的卷）。
fn locate_volume(idx: usize, volumes: &[VolumeMarker]) -> Option<usize> {
    volumes
        .iter()
        .enumerate()
        .rev()
        .find(|(_, v)| v.first_chapter_index <= idx)
        .map(|(i, _)| i)
}

#[component]
pub fn SelectChapter(
    props: &mut SelectChapterProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let mut filter_text = hooks.use_state(String::default);
    let theme = hooks.use_component_theme::<AppChromeTheme>();

    // 树状态：首次渲染时定位到当前章节并展开其所属卷。
    let state = {
        let dv = props.default_value;
        let volumes = props.volumes.clone();
        hooks.use_state(move || {
            let mut st = TreeState::<TocId>::default();
            if let Some(idx) = dv {
                if let Some(vi) = locate_volume(idx, &volumes) {
                    st.open(vec![TocId::Volume(vi)]);
                    st.select(vec![TocId::Volume(vi), TocId::Chapter(idx)]);
                } else {
                    st.select(vec![TocId::Chapter(idx)]);
                }
            }
            st
        })
    };

    let is_editing = props.is_editing;
    let is_empty = props.chapters.is_empty();

    // 构建树节点：搜索态塌成扁平过滤列表；否则按卷分组（无卷则平铺）。
    let items = hooks.use_memo(
        || build_items(&props.chapters, &props.volumes, &filter_text.read(), &theme),
        (
            filter_text.read().clone(),
            props.chapters.len(),
            props.volumes.len(),
        ),
    );

    let mut on_select = props.on_select.take();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press || !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                state.write().key_left();
                EventResult::Consumed
            }
            KeyCode::Char('j') | KeyCode::Down => {
                state.write().key_down();
                EventResult::Consumed
            }
            KeyCode::Char('k') | KeyCode::Up => {
                state.write().key_up();
                EventResult::Consumed
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                let selected = state.read().selected().last().cloned();
                match selected {
                    // 选中章节叶子 → 沿用既有「按扁平索引设置当前章节」逻辑
                    Some(TocId::Chapter(idx)) => on_select(idx),
                    // 卷节点 → 展开/收起
                    Some(TocId::Volume(_)) => {
                        state.write().toggle_selected();
                    }
                    None => {}
                }
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    let border = Block::bordered()
        .border_style(theme.border)
        .title_top(Line::from("目录").style(theme.title).centered());

    element!(View {
        SearchInput(
            placeholder: "按s搜索章节,以$开头输入数字表示索引",
            on_submit: move |text| {
                filter_text.set(text);
                true
            },
            clear_on_escape: true,
            on_clear: move |_| {
                filter_text.set(String::default());
            },
            validate: |input: String| {
                if let Some(stripped) = input.strip_prefix('$') {
                    if stripped.parse::<usize>().is_ok() {
                        (true, "".to_owned())
                    } else {
                        (false, "请输入正确的数字".to_owned())
                    }
                } else {
                    (true, "".to_owned())
                }
            },
            is_editing: is_editing,
        )
        if is_empty {
            Border(
                top_title: Some(Line::from("目录").style(theme.title).centered()),
                border_style: theme.border,
            ){
                Center(height: Constraint::Length(5), width: Constraint::Percentage(50)){
                    Text(
                        text: if filter_text.read().is_empty() { "暂无章节".to_owned() } else { "无匹配章节".to_owned() },
                        alignment: Alignment::Center,
                        style: theme.empty,
                        wrap: true,
                    )
                }
            }
        } else {
            TreeSelect<TocId>(
                style: theme.text,
                highlight_style: theme.selected,
                state: state,
                items: items.clone(),
                scrollbar: Scrollbar::default(),
                block: border,
            )
        }
    })
}

/// 根据章节、卷与过滤条件构建树节点列表。
fn build_items(
    chapters: &[ChapterName],
    volumes: &[VolumeMarker],
    filter: &str,
    theme: &AppChromeTheme,
) -> Vec<TreeItem<'static, TocId>> {
    let leaf = |c: &ChapterName| TreeItem::new_leaf(TocId::Chapter(c.1), c.0.clone());

    // 搜索态：塌成扁平过滤列表。
    if !filter.is_empty() {
        return chapters
            .iter()
            .filter(|c| {
                if let Some(idx) = filter.strip_prefix('$') {
                    idx.parse::<usize>().map(|n| n == c.1).unwrap_or(false)
                } else {
                    c.0.contains(filter)
                }
            })
            .map(leaf)
            .collect();
    }

    // 无分卷：平铺。
    if volumes.is_empty() {
        return chapters.iter().map(leaf).collect();
    }

    // 分卷：卷前的孤立章节置于根层级，其余按卷分组。
    let len = chapters.len();
    let mut items = Vec::new();
    let first_start = volumes[0].first_chapter_index.min(len);
    items.extend(chapters[..first_start].iter().map(leaf));

    for (vi, v) in volumes.iter().enumerate() {
        let start = v.first_chapter_index.min(len);
        let end = volumes
            .get(vi + 1)
            .map(|nv| nv.first_chapter_index.min(len))
            .unwrap_or(len);
        let children: Vec<_> = chapters[start..end].iter().map(leaf).collect();
        let title = Line::from(v.title.clone()).style(theme.title);
        if let Ok(node) = TreeItem::new(TocId::Volume(vi), title, children) {
            items.push(node);
        }
    }

    items
}
