use clap::builder::Str;
use ratatui_kit::{AnyElement, component, element, prelude::Fragment};

use crate::components::{ConfirmModal, search_input::SearchInput, select::Select};

#[component]
pub fn Playground() -> impl Into<AnyElement<'static>> {
    element!(Fragment{
        // SearchInput(
        //     placeholder:"这是一个输入框，按e开始编辑，按Esc结束编辑",
        //     validate:|val:String|{
        //         (!val.trim().is_empty(),"输入不能为空".to_string())
        //     },
        //     on_submit:|val:String|{
        //         if !val.trim().is_empty() {
        //             true
        //         } else {
        //             false
        //         }
        //     }
        // )
        // ConfirmModal(
        //     title:"这是一个确认框",
        //     content:"这是一个确认框，是否继续？",
        //     open:true,
        // )
        Select<String>(
            items:vec![
                "选项一".to_string(),
                "选项二".to_string(),
                "选项三".to_string(),
                "选项四".to_string(),
                "选项五".to_string(),
                "选项六".to_string(),
                "选项七".to_string(),
                "选项八".to_string(),
                "选项九".to_string(),
                "选项十".to_string(),
            ],
            is_editing:true,
        )
    })
}
