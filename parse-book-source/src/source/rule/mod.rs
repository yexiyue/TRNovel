pub mod rule_book_info;
pub mod rule_content;
pub mod rule_explore;
pub mod rule_search;
pub mod rule_toc;

pub use rule_book_info::*;
pub use rule_content::*;
pub use rule_explore::*;
pub use rule_search::*;
pub use rule_toc::*;

#[derive(Debug, Clone)]
pub struct JsonRule {
    pub book_info: JsonRuleBookInfo,
    pub content: JsonRuleContent,
    pub explore: Option<JsonRuleExplore>,
    pub search: JsonRuleSearch,
    pub toc: JsonRuleToc,
}
