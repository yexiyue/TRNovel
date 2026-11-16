//! Common setting rows, tabs, scrollable forms and exclusive choice pickers.
pub mod navigation;
pub mod view;
pub use crate::hooks::{Callback, callback, use_callback};
pub use navigation::Focus;
use ratatui::layout::Rect;
use ratatui_kit::prelude::*;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Toggle(bool),
    Number { decrease: bool, increase: bool },
    Choice,
    Action,
    ReadOnly,
    Link,
}
#[derive(Clone)]
pub struct Row<K> {
    pub group: &'static str,
    pub key: K,
    pub label: String,
    pub value: String,
    pub detail: String,
    pub control: Control,
    pub swatch: Option<ratatui::style::Color>,
}
impl<K> Row<K> {
    pub fn map_key<L>(self, f: impl FnOnce(K) -> L) -> Row<L> {
        Row {
            key: f(self.key),
            group: self.group,
            label: self.label,
            value: self.value,
            detail: self.detail,
            control: self.control,
            swatch: self.swatch,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    Focus(usize),
    Activate(usize),
    Adjust(usize, i32),
}
pub type Editor = Callback<Edit>;
pub fn use_area(hooks: &mut Hooks) -> view::FrameArea {
    hooks
        .use_state(|| Arc::new(Mutex::new(Rect::default())))
        .read()
        .clone()
}

mod button;
mod form;
mod picker;
mod tabs;
pub use button::{ButtonProps, SettingsButton};
pub use form::{FormProps, SettingsForm};
pub use picker::{ChoiceListProps, PickerProps, SettingsChoiceList, SettingsPicker};
pub use tabs::{SettingsTabs, TabsProps};
