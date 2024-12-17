pub mod book_detail;
pub mod book_history;
pub mod book_source_manager;
pub mod find_books;
use super::PageWrapper;
use crate::Result;
use book_source_manager::{BookSourceManager, BookSourceManagerMsg};

pub fn network_novel_first_page(
) -> Result<Box<PageWrapper<BookSourceManager, (), BookSourceManagerMsg>>> {
    Ok(Box::new(PageWrapper::new((), None)))
}
