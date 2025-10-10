use crate::{
    ThemeConfig,
    components::{WarningModal, multi_list_select::MultiListSelect, search_input::SearchInput},
    hooks::{UseInitState, UseThemeConfig},
};
use anyhow::anyhow;
use chrono::DateTime;
use parse_book_source::BookSource;
use ratatui::{
    layout::{Constraint, Layout},
    text::{Line, Span, Text},
    widgets::{Block, Padding, Paragraph, Widget, WidgetRef},
};
use ratatui_kit::{AnyElement, Hooks, UseState, component, element, prelude::View};
use std::collections::HashSet;
use tui_widget_list::ListBuildContext;

pub fn time_to_string(timestamp: u64) -> anyhow::Result<String> {
    // 将时间戳转换为NaiveDateTime
    let naive = DateTime::from_timestamp_millis(timestamp as i64).ok_or(anyhow!("时间戳无效"))?;

    // 格式化为指定的字符串格式
    Ok(naive.format("%Y-%m-%d %H:%M:%S").to_string())
}

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
        let [left, right] = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
            .areas(block.inner(area));

        block.render(area, buf);

        let text_style = if self.selected {
            self.theme.basic.text.patch(self.theme.selected)
        } else if self.height_light {
            self.theme.basic.text.patch(self.theme.highlight)
        } else {
            self.theme.basic.text
        };

        Paragraph::new(Text::from(vec![
            Line::from(self.book_source.book_source_name.clone())
                .style(text_style)
                .centered(),
            Line::from(format!(
                "{} {}",
                self.book_source.book_source_url,
                time_to_string(self.book_source.last_update_time).unwrap()
            ))
            .style(self.theme.basic.border_info.patch(text_style))
            .right_aligned(),
        ]))
        .render(right, buf);

        if self.selected {
            Span::from("✔").render(left, buf);
        } else {
            Span::from("☐").render(left, buf);
        }
    }
}

#[component]
pub fn ImportBookSource(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let selected = hooks.use_state(HashSet::<usize>::default);
    let mut book_source_url = hooks.use_state(String::new);
    let is_inputting = hooks.use_state(|| false);
    let theme = hooks.use_theme_config();

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
            is_editing:is_inputting,
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
            value: selected,
            is_editing:! is_inputting.get(),
            empty_message: "暂无数据",
            loading: loading.get(),
            top_title:Line::from("选择要导入的书源 (空格选择, 回车确认)").centered(),
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
            }
        )
        WarningModal(
            tip: format!("解析失败:{:?}", error.read().as_ref()),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
    })
}
