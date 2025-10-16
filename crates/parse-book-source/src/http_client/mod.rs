use crate::{HttpConfig, Result};
use anyhow::anyhow;
use rate_limiter::TokenBucket;
use reqwest::{
    Body, Client, ClientBuilder,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use std::time::Duration;
pub mod rate_limiter;

#[derive(Debug, Clone)]
pub struct HttpClient {
    pub client: Client,
    pub base_url: String,
    pub rate_limiter: Option<TokenBucket>,
}

impl HttpClient {
    pub fn new(base_url: &str, config: &HttpConfig) -> Result<Self> {
        let mut client = ClientBuilder::new().cookie_store(true);

        if let Some(header) = &config.header {
            let mut headers = HeaderMap::new();

            for (k, v) in header {
                headers.insert(
                    HeaderName::try_from(k)
                        .map_err(|e| anyhow!("header name is not valid: {}", e))?,
                    HeaderValue::from_str(v)
                        .map_err(|e| anyhow!("header value is not valid: {}", e))?,
                );
            }
            client = client.default_headers(headers);
        }

        if let Some(timeout) = config.timeout {
            client = client.timeout(Duration::from_millis(timeout));
        }

        Ok(Self {
            client: client.build()?,
            base_url: base_url.to_string(),
            rate_limiter: config.rate_limit.as_ref().map(|rate_limit| {
                TokenBucket::new(
                    rate_limit.max_count as usize,
                    Duration::from_secs_f64(rate_limit.fill_duration),
                )
            }),
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
        if let Some(rate_limiter) = &self.rate_limiter {
            rate_limiter.acquire().await;
        }

        let url = self.url_with_base(url);

        Ok(self.client.get(url).send().await?)
    }

    pub async fn post<T: Into<Body>>(&self, url: &str, body: T) -> Result<reqwest::Response> {
        if let Some(rate_limiter) = &self.rate_limiter {
            rate_limiter.acquire().await;
        }

        let url = self.url_with_base(url);

        Ok(self.client.post(url).body(body).send().await?)
    }
}
