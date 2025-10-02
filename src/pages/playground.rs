use ratatui_kit::{AnyElement, component, element, prelude::Fragment};

use crate::components::search_input::SearchInput;

#[component]
pub fn Playground() -> impl Into<AnyElement<'static>> {
    element!(Fragment{
        SearchInput(
            placeholder:"这是一个输入框，按e开始编辑，按Esc结束编辑",
            validate:|val:String|{
                (!val.trim().is_empty(),"输入不能为空".to_string())
            },
            on_submit:|val:String|{
                if !val.trim().is_empty() {
                    true
                } else {
                    false
                }
            }
        )
    })
}
