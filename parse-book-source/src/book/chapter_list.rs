use reqwest::Response;
use serde::{Deserialize, Serialize};

use crate::{HttpClient, Result};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChapterList {
    #[serde(flatten)]
    pub chapter_list: Vec<Chapter>,
}

impl ChapterList {
    pub fn new(chapter_list: Vec<Chapter>) -> Self {
        ChapterList { chapter_list }
    }
}

impl Deref for ChapterList {
    type Target = Vec<Chapter>;

    fn deref(&self) -> &Self::Target {
        &self.chapter_list
    }
}

impl DerefMut for ChapterList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.chapter_list
    }
}

impl From<Vec<Chapter>> for ChapterList {
    fn from(chapter_list: Vec<Chapter>) -> Self {
        ChapterList { chapter_list }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub chapter_name: String,
    pub chapter_url: String,
}

impl Chapter {
    pub async fn get_content(&self, client: &HttpClient) -> Result<Response> {
        client.get(&self.chapter_url).await
    }
}
