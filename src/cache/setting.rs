use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    title_color: String,
    text_color: String,
    selected_color: String,
    border_color: String,
    border_title_color: String,
    loading_color: String,
    loading_text_color: String,
    loading_border_color: String,
    loading_background_color: String,
}
