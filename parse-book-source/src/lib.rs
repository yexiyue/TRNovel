use anyhow::anyhow;
use serde_json::json;

pub mod analyzer;
pub mod book;
pub mod book_source;
pub mod error;
pub mod http_client;
pub mod utils;
pub use analyzer::*;
pub use book::*;
pub use book_source::*;
pub use error::*;
pub use http_client::*;

#[derive(Debug, Clone)]
pub struct BookSourceParser {
    pub book_source: BookSource,
    pub http_client: HttpClient,
    pub analyzer: AnalyzerManager,
    pub temp: Option<String>,
}

impl TryFrom<BookSource> for BookSourceParser {
    type Error = ParseError;

    fn try_from(book_source: BookSource) -> Result<Self> {
        let mut http_config = book_source.http_config.clone();
        if let Some(ref header) = book_source.header {
            http_config.header = Some(serde_json::from_str(header)?);
        }

        if let Some(ref response_time) = book_source.respond_time {
            http_config.timeout = Some(*response_time);
        }

        Ok(Self {
            http_client: HttpClient::new(&book_source.book_source_url, &http_config)?,
            book_source,
            analyzer: AnalyzerManager::new()?,
            temp: None,
        })
    }
}

impl BookSourceParser {
    pub fn new(book_source: BookSource) -> Result<Self> {
        Self::try_from(book_source)
    }

    /// 获取分类信息
    pub async fn get_explores(&mut self) -> Result<ExploreList> {
        if let Some(ref explore_url) = self.book_source.explore_url {
            if let Some(ref rule_explore_item) = self.book_source.rule_explore_item {
                let res = self
                    .http_client
                    .get(&self.book_source.book_source_url)
                    .await?
                    .text()
                    .await?;

                let list = self.analyzer.get_element(&explore_url, &res)?;

                let res = list
                    .into_iter()
                    .flat_map(|item| {
                        rule_explore_item.parse_to_explore_item(&mut self.analyzer, &item)
                    })
                    .collect::<Vec<_>>();
                return Ok(res);
            } else {
                return Ok(serde_json::from_str(&explore_url)?);
            }
        }

        Ok(vec![])
    }

    /// 搜索书籍
    pub async fn search_books(&mut self, key: &str, page: u32, page_size: u32) -> Result<BookList> {
        let url = self.analyzer.get_string(
            &self.book_source.search_url,
            "",
            Some(json!({
                "key": key,
                "page": page,
                "page_size": page_size,
            })),
        )?;

        let res = self.http_client.get(url.as_str()).await?.text().await?;

        let list = self
            .analyzer
            .get_element(&self.book_source.rule_search.book_list, &res)?;

        let res = list
            .into_iter()
            .flat_map(|item| {
                self.book_source
                    .rule_search
                    .parse_to_book_list_item(&mut self.analyzer, &item)
            })
            .collect::<Vec<_>>();

        Ok(res)
    }

    /// 使用explore_item的url获取书籍列表
    pub async fn explore_books(
        &mut self,
        url: &str,
        page: u32,
        page_size: u32,
    ) -> Result<BookList> {
        if self.book_source.rule_explore.is_none() {
            return Err(anyhow!("explore rule is none").into());
        }
        let url = self.analyzer.get_string(
            url,
            "",
            Some(json!({
                "page": page,
                "page_size": page_size,
            })),
        )?;

        let res = self.http_client.get(url.as_str()).await?.text().await?;

        let list = self.analyzer.get_element(
            &self.book_source.rule_explore.as_ref().unwrap().book_list,
            &res,
        )?;

        let res = list
            .into_iter()
            .flat_map(|item| {
                self.book_source
                    .rule_explore
                    .as_ref()
                    .unwrap()
                    .parse_to_book_list_item(&mut self.analyzer, &item)
            })
            .collect::<Vec<_>>();

        Ok(res)
    }

    /// 获取书籍信息
    pub async fn get_book_info(&mut self, book_url: &str) -> Result<BookInfo> {
        let res = self.http_client.get(book_url).await?.text().await?;

        let book_info = self
            .book_source
            .rule_book_info
            .parse_to_book_info(&mut self.analyzer, &res);

        self.temp = Some(res);

        book_info
    }

    pub async fn get_chapters(&mut self, toc_url: &str) -> Result<Vec<Chapter>> {
        // 如果toc_url是http开头的url，直接请求
        let res = if toc_url.starts_with("/") || toc_url.starts_with("http") {
            self.http_client.get(toc_url).await?.text().await?
        } else {
            self.temp.take().ok_or(anyhow!("temp is none"))?
        };

        let list = self
            .analyzer
            .get_element(&self.book_source.rule_toc.chapter_list, &res)?;

        let res = list
            .into_iter()
            .flat_map(|item| {
                self.book_source
                    .rule_toc
                    .parse_to_chapter(&mut self.analyzer, &item)
            })
            .collect::<Vec<_>>();

        Ok(res)
    }

    pub async fn get_content(&mut self, chapter_url: &str) -> Result<String> {
        let mut res = self.http_client.get(chapter_url).await?.text().await?;

        match &self.book_source.rule_content {
            RuleContent::One { content } => self.analyzer.get_string(content, &res, None),

            RuleContent::More {
                content,
                next_content_url,
                start,
                end,
            } => {
                let end = self
                    .analyzer
                    .get_string(end, &res, None)?
                    .parse::<usize>()?;
                let mut contents = vec![];
                let mut start = *start;

                loop {
                    let content = self.analyzer.get_string(content, &res, None)?;
                    contents.push(content);

                    if start > end {
                        break;
                    }

                    let next_url = self.analyzer.get_string(
                        next_content_url,
                        &res,
                        Some(json!({
                            "index": start,
                        })),
                    )?;
                    res = self
                        .http_client
                        .get(next_url.as_str())
                        .await?
                        .text()
                        .await?;
                    start += 1;
                }

                Ok(contents.join("  "))
            }
        }
    }
}
