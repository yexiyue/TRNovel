pub mod explore_item;
use std::ops::Deref;

use crate::{ParseError, Result};
pub use explore_item::*;

#[derive(Debug, Clone)]
pub struct Explores {
    pub explore_list: Vec<ExploreItem>,
}

impl Explores {
    pub fn from_json(json: &str) -> Result<Self> {
        let explore_list = serde_json::from_str(json)?;
        Ok(Self { explore_list })
    }
}

impl TryFrom<&str> for Explores {
    type Error = ParseError;
    fn try_from(value: &str) -> Result<Self> {
        Self::from_json(value)
    }
}

impl Deref for Explores {
    type Target = Vec<ExploreItem>;
    fn deref(&self) -> &Self::Target {
        &self.explore_list
    }
}
