use crate::hooks::UseThemeConfig;
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, component, element, prelude::SearchInput as KitSearchInput,
};

#[derive(Default, Props)]
pub struct SearchInputProps {
    pub value: String,
    pub placeholder: String,
    pub validate: Handler<'static, String, (bool, String)>,
    pub on_submit: Handler<'static, String, bool>,
    pub clear_on_submit: bool,
    pub clear_on_escape: bool,
    pub is_editing: bool,
    pub on_clear: Handler<'static, ()>,
}

/// 搜索框:薄主题适配层,委托框架 `SearchInput`。
///
/// 框架 0.7 的 `SearchInput` 自带 `activate_key` + 独占输入层(编辑时 blocks_lower),
/// 因此本组件**不再需要全局 `is_inputting` 门控**,也从根上消除了旧广播模型下
/// 「提交搜索的同一个 Enter 被父级列表误当作选中」的跨帧竞态。wrapper 仅把 TRNovel
/// 主题映射成内置所需的 Style props,并透传校验/提交/清空回调与编辑开关。
#[component]
pub fn SearchInput(
    props: &mut SearchInputProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();

    element!(KitSearchInput(
        width: Constraint::Fill(1),
        value: props.value.clone(),
        placeholder: props.placeholder.clone(),
        is_editing: props.is_editing,
        validate: props.validate.take(),
        on_submit: props.on_submit.take(),
        on_clear: props.on_clear.take(),
        clear_on_submit: props.clear_on_submit,
        clear_on_escape: props.clear_on_escape,
        border_style: theme.basic.border,
        active_border_style: theme.basic.border,
        success_border_style: theme.search.success_border,
        error_border_style: theme.search.error_border,
        input_style: theme.search.text,
        placeholder_style: theme.search.placeholder,
        cursor_style: Style::new().bg(Color::DarkGray),
        success_status_style: theme.search.success_border_info,
        error_status_style: theme.search.error_border_info,
    ))
}
