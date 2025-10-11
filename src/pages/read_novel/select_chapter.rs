use ratatui::{text::Line, widgets::ListItem};
use ratatui_kit::prelude::*;

use crate::components::{search_input::SearchInput, select::Select};

#[derive(Default, Clone)]
pub struct ChapterName(pub String, pub usize);

impl From<(String, usize)> for ChapterName {
    fn from(value: (String, usize)) -> Self {
        Self(value.0, value.1)
    }
}

impl From<ChapterName> for ListItem<'_> {
    fn from(value: ChapterName) -> Self {
        ListItem::new(value.0)
    }
}

#[derive(Default, Props)]
pub struct SelectChapterProps {
    pub chapters: Vec<ChapterName>,
    pub on_select: Handler<'static, usize>,
    pub default_value: Option<usize>,
}

#[component]
pub fn SelectChapter(
    props: &mut SelectChapterProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let mut filter_text = hooks.use_state(String::default);
    let is_inputting = hooks.use_state(|| false);
    let state = hooks.use_state(|| {
        let mut state = ratatui::widgets::ListState::default();
        state.select(props.default_value);
        state
    });
    let items = hooks.use_memo(
        || {
            if filter_text.read().is_empty() {
                props.chapters.clone()
            } else {
                props
                    .chapters
                    .iter()
                    .filter(|&chapter_name| {
                        if filter_text.read().starts_with('$') {
                            let index_str = &filter_text.read()[1..];
                            if let Ok(index) = index_str.parse::<usize>() {
                                index == chapter_name.1
                            } else {
                                false
                            }
                        } else {
                            chapter_name.0.contains(filter_text.read().as_str())
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            }
        },
        (filter_text.read().clone(), props.chapters.len()),
    );

    let mut on_select = props.on_select.take();

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
            is_editing: is_inputting,
        )
        Select<ChapterName>(
            top_title: Line::from("目录").centered(),
            empty_message: if filter_text.read().is_empty() {
                "暂无章节".to_owned()
            } else {
                "无匹配章节".to_owned()
            },
            on_select: move |item: ChapterName| {
                on_select(item.1);
            },
            is_editing: !is_inputting.get(),
            state: state,
            bottom_title: Line::from(
                format!(
                    "{}/{}",
                    state.read().selected().unwrap_or(0) + 1,
                    items.len()
                )
            ),
            items: items,
        )
    })
}
