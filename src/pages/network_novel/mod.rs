pub mod book_detail;
pub mod book_source_manager;
pub mod find_books;
use super::PageWrapper;
use crate::{Result, RoutePage};
use book_source_manager::{BookSourceManager, BookSourceManagerMsg};

pub fn network_novel_first_page() -> Result<Box<dyn RoutePage>> {
    Ok(Box::new(PageWrapper::<
        BookSourceManager,
        (),
        BookSourceManagerMsg,
    >::new((), None)))
}
