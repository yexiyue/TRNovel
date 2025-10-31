use crate::{
    ThemeConfig,
    book_source::BookSourceCache,
    components::{WarningModal, multi_list_select::MultiListSelect, search_input::SearchInput},
    hooks::{UseInitState, UseThemeConfig},
    utils::time_to_string,
};

use parse_book_source::BookSource;
use ratatui::{
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Block, Padding, Widget, WidgetRef},
};
use ratatui_kit::prelude::*;
use std::collections::HashSet;
use tui_widget_list::ListBuildContext;

struct ListItem {
    pub book_source: BookSource,
    pub selected: bool,
    pub height_light: bool,
    pub theme: ThemeConfig,
}

impl WidgetRef for ListItem {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let block = if self.height_light {
            Block::bordered()
                .padding(Padding::horizontal(2))
                .style(self.theme.highlight)
        } else if self.selected {
            Block::bordered()
                .padding(Padding::horizontal(2))
                .style(self.theme.selected)
        } else {
            Block::bordered().padding(Padding::horizontal(2))
        };
        let [left, _right] = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
            .areas(block.inner(area));

        let inner_area = block.inner(area);
        block.render(area, buf);

        let text_style = if self.selected {
            self.theme.basic.text.patch(self.theme.selected)
        } else if self.height_light {
            self.theme.basic.text.patch(self.theme.highlight)
        } else {
            self.theme.basic.text
        };

        let [top, bottom] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner_area);

        let [bottom_left, bottom_right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom);

        Line::from(self.book_source.book_source_name.clone())
            .style(text_style)
            .centered()
            .render(top, buf);

        Line::from(format!("网址: {}", self.book_source.book_source_url))
            .style(self.theme.basic.text.patch(text_style))
            .left_aligned()
            .render(bottom_left, buf);

        Line::from(format!(
            "最后更新: {}",
            time_to_string(self.book_source.last_update_time).unwrap_or_default()
        ))
        .style(self.theme.basic.border_info.patch(text_style))
        .right_aligned()
        .render(bottom_right, buf);

        if self.selected {
            Span::from("✔").render(left, buf);
        } else {
            Span::from("☐").render(left, buf);
        }
    }
}

#[derive(Default, Props)]
pub struct ImportBookSourceProps {
    pub is_editing: bool,
}

#[component]
pub fn ImportBookSource(
    props: &ImportBookSourceProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let selected = hooks.use_state(HashSet::<usize>::default);
    let mut book_source_url = hooks.use_state(String::new);
    let is_editing = props.is_editing;
    let is_inputting = *hooks.use_context::<State<bool>>();
    let theme = hooks.use_theme_config();

    let book_source_cache = *hooks.use_context::<State<Option<BookSourceCache>>>();

    let (book_source, loading, error) = hooks.use_effect_state(
        async move {
            let query = book_source_url.read().clone();
            if query.is_empty() {
                return Ok(vec![]);
            }
            if query.starts_with("http") {
                BookSource::from_url(query.trim()).await
            } else {
                BookSource::from_path(query.trim())
            }
        },
        book_source_url.read().clone(),
    );

    element!(View {
        SearchInput(
            value: book_source_url.read().clone(),
            placeholder: "按s输入书源地址 (支持 http, https, file)",
            is_editing: is_editing,
            validate:|value:String|{
                if value.starts_with("http")
                    || value.starts_with("https")
                {
                    (true, String::new())
                } else {
                   let path= std::path::Path::new(&value);
                    if path.exists(){
                        (true, String::new())
                    }else{
                        (false, "请输入正确的书源地址".to_string())
                    }
                }
            },
            on_submit:move |value:String|{
                book_source_url.set(value.clone());
                true
            }
        )
        MultiListSelect<BookSource>(
            state: selected,
            is_editing: !is_inputting.get() && is_editing,
            empty_message: "暂无数据",
            loading: loading.get(),
            top_title: Line::from("选择要导入的书源 (空格选择, 回车确认)").style(
               theme.basic.border_title
            ).centered(),
            loading_tip:"解析中...".to_string(),
            items: book_source.read().clone().unwrap_or_default(),
            render_item:move|ctx:&ListBuildContext|{
                let book_source=book_source.read().clone().unwrap_or_default();
                (ListItem{
                    book_source: book_source[ctx.index].clone(),
                    selected: selected.read().contains(&ctx.index),
                    height_light: ctx.is_selected,
                    theme: theme.clone(),
                }.into(),4)
            },
            on_select: move|items:Vec<BookSource>|{
                if let Some(book_source_cache)=book_source_cache.write().as_mut(){
                    for item in items{
                        book_source_cache.add_book_source(item);
                    }
                }
            }
        )
        WarningModal(
            tip: format!("解析失败:{:?}", error.read().as_ref()),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
    })
}
