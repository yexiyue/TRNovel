use crate::utils::Params;
use crate::Result;
use crate::{http_client::HttpClient, utils::parse_url};
use reqwest::Response;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploreItem {
    pub title: String,
    pub url: String,
}

impl ExploreItem {
    pub async fn get_book_list(&self, client: &HttpClient, params: Params) -> Result<Response> {
        let url = parse_url(&self.url, &params, None)?;
        client.get(&url).await
    }
}
