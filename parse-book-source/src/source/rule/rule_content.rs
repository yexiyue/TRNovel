use crate::HttpClient;
use crate::Result;
use crate::Variables;
use crate::{utils::JsonData, ParseError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
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

impl TryFrom<&RuleContent> for JsonRuleContent {
    type Error = ParseError;

    fn try_from(value: &RuleContent) -> std::result::Result<Self, Self::Error> {
        match value {
            RuleContent::More {
                content,
                next_content_url,
                start,
                end,
            } => Ok(JsonRuleContent::More {
                content: content.try_into()?,
                next_content_url: next_content_url.try_into()?,
                start: *start,
                end: end.try_into()?,
            }),
            RuleContent::One { content } => Ok(JsonRuleContent::One {
                content: content.try_into()?,
            }),
        }
    }
}

impl TryFrom<RuleContent> for JsonRuleContent {
    type Error = ParseError;
    fn try_from(value: RuleContent) -> std::result::Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

#[derive(Debug, Clone)]
pub enum JsonRuleContent {
    More {
        content: JsonData,
        next_content_url: JsonData,
        start: usize,
        end: JsonData,
    },
    One {
        content: JsonData,
    },
}

impl JsonRuleContent {
    pub async fn parse_content(
        &self,
        data: &Value,
        variables: &mut Variables,
        client: &HttpClient,
    ) -> Result<String> {
        match self {
            JsonRuleContent::One { content } => Ok(content.parse_data(data, variables)?),
            JsonRuleContent::More {
                content,
                start,
                end,
                next_content_url,
            } => {
                let mut content_str = content.parse_data(data, variables)?;
                let end = end.parse_data(data, variables)?.parse::<usize>()?;
                for i in *start..=end {
                    variables.insert("index", i);

                    let url = &next_content_url.parse_data(data, variables)?;

                    let res: Value = client.get(url).await?.json().await?;
                    content_str += &content.parse_data(&res, variables)?;
                }

                Ok(content_str)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_rule_content() -> Result<()> {
        let json = json!({
            "nextContentUrl": "http://www.baidu.com",
            "start": 1,
            "end": "2",
            "content":"hello world",
        });

        let rule_content: RuleContent = serde_json::from_value(json)?;
        assert!(matches!(rule_content, RuleContent::More { .. }));
        Ok(())
    }
}
