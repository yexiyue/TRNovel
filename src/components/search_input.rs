use ratatui::layout::Constraint;
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

/// 搜索框项目 wrapper:保留 TRNovel props 形状,委托框架 `SearchInput` 的输入层与主题。
#[component]
pub fn SearchInput(props: &mut SearchInputProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
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
    ))
}
