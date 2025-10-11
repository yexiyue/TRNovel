use crate::{AnalyzerManager, BookInfo, BookListItem, Chapter, ExploreItem, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleSearch {
    pub book_list: String,
    pub book_url: String,
    #[serde(flatten)]
    pub book_info: RuleBookInfo,
}

impl RuleSearch {
    pub fn parse_to_book_list_item(
        &self,
        analyzer: &mut AnalyzerManager,
        content: &str,
    ) -> Result<BookListItem> {
        Ok(BookListItem {
            book_url: analyzer.get_string(&self.book_url, content, None)?,
            book_info: self.book_info.parse_to_book_info(analyzer, content)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleExploreItem {
    pub title: String,
    pub url: String,
}

impl RuleExploreItem {
    pub fn parse_to_explore_item(
        &self,
        analyzer: &mut AnalyzerManager,
        content: &str,
    ) -> Result<ExploreItem> {
        Ok(ExploreItem {
            title: analyzer.get_string(&self.title, content, None)?,
            url: analyzer.get_string(&self.url, content, None)?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleBookInfo {
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub cover_url: String,
    #[serde(default)]
    pub intro: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub last_chapter: String,
    #[serde(default)]
    pub toc_url: String,
    #[serde(default)]
    pub word_count: String,
}

impl RuleBookInfo {
    pub fn parse_to_book_info(
        &self,
        analyzer: &mut AnalyzerManager,
        content: &str,
    ) -> Result<BookInfo> {
        Ok(BookInfo {
            name: analyzer.get_string(&self.name, content, None)?,
            author: analyzer.get_string(&self.author, content, None)?,
            cover_url: analyzer.get_string(&self.cover_url, content, None)?,
            intro: analyzer.get_string(&self.intro, content, None)?,
            kind: analyzer.get_string(&self.kind, content, None)?,
            last_chapter: analyzer.get_string(&self.last_chapter, content, None)?,
            toc_url: analyzer.get_string(&self.toc_url, content, None)?,
            word_count: analyzer.get_string(&self.word_count, content, None)?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleToc {
    pub chapter_list: String,
    pub chapter_name: String,
    pub chapter_url: String,
}

impl RuleToc {
    pub fn parse_to_chapter(
        &self,
        analyzer: &mut AnalyzerManager,
        content: &str,
    ) -> Result<Chapter> {
        Ok(Chapter {
            chapter_name: analyzer.get_string(&self.chapter_name, content, None)?,
            chapter_url: analyzer.get_string(&self.chapter_url, content, None)?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, Eq)]
#[serde(untagged)]
pub enum RuleContent {
    #[serde(rename_all = "camelCase")]
    More {
        content: String,
        next_content_url: String,
        start: usize,
        end: String,
    },
    One {
        content: String,
    },
}
