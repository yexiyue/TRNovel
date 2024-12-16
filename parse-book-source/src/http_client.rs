use std::time::Duration;

use crate::Result;
use anyhow::anyhow;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Body, Client, ClientBuilder,
};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct HttpClient {
    pub client: Client,
    pub base_url: String,
}

impl HttpClient {
    pub fn new(base_url: &str, header: &str, timeout: u64) -> Result<Self> {
        let header: Value = serde_json::from_str(header)?;
        let header = header.as_object().ok_or(anyhow!("header is not object"))?;
        let mut headers = HeaderMap::new();

        for (k, v) in header {
            let v = v.as_str().ok_or(anyhow!("header value is not string"))?;
            headers.insert(
                HeaderName::try_from(k).map_err(|e| anyhow!("header name is not valid: {}", e))?,
                HeaderValue::from_str(v)
                    .map_err(|e| anyhow!("header value is not valid: {}", e))?,
            );
        }

        let client = ClientBuilder::new()
            .default_headers(headers)
            .timeout(Duration::from_millis(timeout))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.to_string(),
        })
    }

    fn url_with_base(&self, url: &str) -> String {
        if url.starts_with("http") {
            url.to_string()
        } else if url.starts_with('/') {
            format!("{}{}", self.base_url, url)
        } else {
            format!("{}/{}", self.base_url, url)
        }
    }

    pub async fn get(&self, url: &str) -> Result<reqwest::Response> {
        let url = self.url_with_base(url);
        Ok(self.client.get(url).send().await?)
    }

    pub async fn post<T: Into<Body>>(&self, url: &str, body: T) -> Result<reqwest::Response> {
        let url = self.url_with_base(url);

        Ok(self.client.post(url).body(body).send().await?)
    }
}
