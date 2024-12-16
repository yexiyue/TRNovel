use crate::Variables;
use crate::{chapter_list::Chapter, Result};
use crate::{
    chapter_list::ChapterList,
    utils::{json_path, JsonData},
    ParseError,
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RuleToc {
    pub chapter_list: String,
    pub chapter_name: String,
    pub chapter_url: String,
}

#[derive(Debug, Clone)]
pub struct JsonRuleToc {
    pub chapter_list: String,
    pub chapter_name: JsonData,
    pub chapter_url: JsonData,
}

impl TryFrom<&RuleToc> for JsonRuleToc {
    type Error = ParseError;
    fn try_from(value: &RuleToc) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            chapter_list: value.chapter_list.clone(),
            chapter_name: value.chapter_name.as_str().try_into()?,
            chapter_url: value.chapter_url.as_str().try_into()?,
        })
    }
}

impl TryFrom<RuleToc> for JsonRuleToc {
    type Error = ParseError;
    fn try_from(value: RuleToc) -> std::result::Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl JsonRuleToc {
    pub fn parse_chapter_list(
        &self,
        data: &Value,
        variables: &mut Variables,
    ) -> Result<ChapterList> {
        let chapter_list = if self.chapter_list.as_str().ends_with("[*]") {
            json_path(data, self.chapter_list.as_str())?
        } else {
            json_path(data, &format!("{}[*]", self.chapter_list))?
        };

        if chapter_list.is_null() {
            return Ok(ChapterList::new(vec![]));
        }

        let mut res = vec![];
        for item in chapter_list
            .as_array()
            .ok_or(anyhow!("book_list is not array"))?
        {
            res.push(Chapter {
                chapter_name: self.chapter_name.parse_data(item, variables)?,
                chapter_url: self.chapter_url.parse_data(item, variables)?,
            });
        }

        Ok(res.into())
    }
}
