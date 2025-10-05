use std::path::PathBuf;

use ratatui_kit::{AnyElement, component, element, prelude::Fragment};
use tui_tree_widget::TreeItem;

use crate::components::{
    ConfirmModal, file_select::FileSelect, search_input::SearchInput, select::Select,
};

#[component]
pub fn Playground() -> impl Into<AnyElement<'static>> {
    let tree_items = vec![
        TreeItem::new(
            PathBuf::from("1"),
            "1",
            vec![
                TreeItem::new(PathBuf::from("1.1"), "1.1", vec![]).unwrap(),
                TreeItem::new(PathBuf::from("1.2"), "1.2", vec![]).unwrap(),
                TreeItem::new(PathBuf::from("1.4"), "1.3", vec![]).unwrap(),
            ],
        )
        .unwrap(),
        TreeItem::new(
            PathBuf::from("2"),
            "2",
            vec![
                TreeItem::new(PathBuf::from("2.1"), "2.1", vec![]).unwrap(),
                TreeItem::new(PathBuf::from("2.2"), "2.2", vec![]).unwrap(),
                TreeItem::new(PathBuf::from("2.4"), "2.3", vec![]).unwrap(),
            ],
        )
        .unwrap(),
        TreeItem::new(
            PathBuf::from("3"),
            "3",
            vec![
                TreeItem::new(
                    PathBuf::from("3.1"),
                    "3.1",
                    vec![
                        TreeItem::new(
                            PathBuf::from("5"),
                            "5",
                            vec![
                                TreeItem::new(PathBuf::from("5.1"), "5.1", vec![]).unwrap(),
                                TreeItem::new(PathBuf::from("5.2"), "5.2", vec![]).unwrap(),
                                TreeItem::new(PathBuf::from("5.4"), "5.3", vec![]).unwrap(),
                            ],
                        )
                        .unwrap(),
                        TreeItem::new(
                            PathBuf::from("6"),
                            "6",
                            vec![
                                TreeItem::new(PathBuf::from("6.1"), "6.1", vec![]).unwrap(),
                                TreeItem::new(PathBuf::from("6.2"), "6.2", vec![]).unwrap(),
                                TreeItem::new(PathBuf::from("6.4"), "6.3", vec![]).unwrap(),
                            ],
                        )
                        .unwrap(),
                    ],
                )
                .unwrap(),
                TreeItem::new(PathBuf::from("3.2"), "3.2", vec![]).unwrap(),
                TreeItem::new(PathBuf::from("3.4"), "3.3", vec![]).unwrap(),
            ],
        )
        .unwrap(),
        TreeItem::new(PathBuf::from("7"), "7", vec![]).unwrap(),
    ];
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
        // Select<String>(
        //     items:vec![
        //         "选项一".to_string(),
        //         "选项二".to_string(),
        //         "选项三".to_string(),
        //         "选项四".to_string(),
        //         "选项五".to_string(),
        //         "选项六".to_string(),
        //         "选项七".to_string(),
        //         "选项八".to_string(),
        //         "选项九".to_string(),
        //         "选项十".to_string(),
        //         "选项一".to_string(),
        //         "选项二".to_string(),
        //         "选项三".to_string(),
        //         "选项四".to_string(),
        //         "选项五".to_string(),
        //         "选项六".to_string(),
        //         "选项七".to_string(),
        //         "选项八".to_string(),
        //         "选项九".to_string(),
        //         "选项十".to_string(),
        //         "选项一".to_string(),
        //     ],
        //     is_editing:true,
        // )
        // FileSelect(
        //     items:tree_items,
        //     is_editing:true,
        // )
    })
}
