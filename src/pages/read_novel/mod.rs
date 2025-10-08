use anyhow::anyhow;
use ratatui::{text::Line, widgets::ListItem};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseMemo, UseRouter, UseState, component, element,
    prelude::{Fragment, View},
};

use crate::{
    components::{Loading, WarningModal, search_input::SearchInput, select::Select},
    errors::Errors,
    hooks::UseInitState,
    novel::Novel,
};

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

#[component]
pub fn ReadNovel<T>(mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    T: Novel + Send + Sync + Unpin + 'static,
{
    let route_state = hooks.use_route_state::<T::Args>();
    let (novel, loading, error) = hooks.use_init_state(async move {
        let args = route_state.clone().ok_or(anyhow!("缺少参数"))?;
        let args = args.as_ref().clone();
        let res = T::init(args).await?;

        Ok::<T, Errors>(res)
    });

    if loading.get() {
        return element!(Loading(tip:"搜索小说中...")).into_any();
    }

    element!(Fragment {
        View{

            WarningModal(
                tip: format!("加载失败:{:?}", error.read().as_ref()),
                is_error: error.read().is_some(),
                open: error.read().is_some(),
            )
        }
    })
    .into_any()
}

#[derive(Default, Props)]
pub struct SelectChapterProps {
    chapters: Vec<ChapterName>,
    on_select: Handler<'static, usize>,
}

#[component]
pub fn SelectChapter(
    props: &mut SelectChapterProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let mut filter_text = hooks.use_state(String::default);
    let is_inputting = hooks.use_state(|| false);
    let items = hooks.use_memo(
        || {
            props
                .chapters
                .iter()
                .filter(|&chapter_name| {
                    if filter_text.read().is_empty() {
                        true
                    } else if filter_text.read().starts_with('$') {
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
        },
        filter_text.read().clone(),
    );

    let mut on_select = props.on_select.take();

    element!(Fragment {
        SearchInput(
            placeholder: "按s搜索章节,以$开头输入数字表示索引",
            on_submit: move |text| {
                filter_text.set(text);
                true
            },
            on_clear: move |_| {
                filter_text.set(String::default());
            },
            validate: |input: String| {
                if input.starts_with('$') {
                    if input[1..].parse::<usize>().is_ok() {
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
            items: items,
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
        )
    })
}
