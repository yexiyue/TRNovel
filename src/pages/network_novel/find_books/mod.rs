pub mod book_list;
pub mod search;
pub mod select_explore;
use parse_book_source::{BookList, ExploreItem};
pub use select_explore::*;

use crate::{app::State, errors::Errors};

pub enum FindBooksMsg {
    Search(String),
    SelectExplore(ExploreItem),
    BookList(Vec<BookList>),
    Error(Errors),
}

pub struct FindBooks {
    pub state: State,
}
