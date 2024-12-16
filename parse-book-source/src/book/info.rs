use crate::HttpClient;
use crate::Result;
use reqwest::Response;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookInfo {
    pub author: String,
    pub cover_url: Option<String>,
    pub intro: String,
    pub kind: String,
    pub last_chapter: String,
    pub name: String,
    pub toc_url: String,
    pub word_count: String,
}

impl BookInfo {
    pub async fn get_chapter_list(&self, client: &HttpClient) -> Result<Response> {
        client.get(&self.toc_url).await
    }
}
