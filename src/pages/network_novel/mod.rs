pub mod find_books;
use find_books::{FindBooks, FindBooksMsg};
pub mod book_history;
pub mod book_detail;
pub mod read_novel;
use crate::Result;

use super::PageWrapper;

pub fn network_novel_first_page<'a>() -> Result<Box<PageWrapper<FindBooks<'a>, (), FindBooksMsg>>> {
    Ok(Box::new(PageWrapper::new((), None)))
}
