use reqwest::Response;

use crate::http_client::HttpClient;
use crate::utils::{parse_url, Params};
use crate::Result;

#[derive(Debug, Clone)]
pub struct Search {
    pub url: String,
}

impl<T> From<T> for Search
where
    T: Into<String>,
{
    fn from(url: T) -> Self {
        Self { url: url.into() }
    }
}

impl Search {
    pub async fn get_book_list(&self, client: &HttpClient, params: Params) -> Result<Response> {
        let url = parse_url(&self.url, &params, None)?;
        client.get(&url).await
    }
}
