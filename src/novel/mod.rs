pub mod local_novel;
pub use local_novel::*;
pub mod line_adapter;
pub use line_adapter::LineAdapter;
pub mod network_novel;
pub mod new_local_novel;
use crate::Result;

pub trait Novel {
    type Chapter: Sync + Send;

    fn set_chapters(&mut self, chapters: &[Self::Chapter]);

    fn get_current_chapter(&self) -> Result<Self::Chapter>;

    fn get_current_chapter_name(&self) -> Result<String>;

    fn chapter_percent(&self) -> Result<f64>;

    fn request_chapters<T: FnMut(Result<Vec<Self::Chapter>>) + Send + 'static>(
        &self,
        callback: T,
    ) -> Result<()>;

    fn get_chapters_result(&self) -> Result<&Vec<Self::Chapter>>;

    fn get_chapters(&self) -> Option<&Vec<Self::Chapter>>;

    fn get_chapters_names(&self) -> Result<Vec<String>>;

    fn get_content<T: FnMut(Result<String>) + Send + 'static>(&mut self, callback: T)
        -> Result<()>;

    fn next_chapter(&mut self) -> Result<()>;

    fn set_chapter(&mut self, chapter: usize) -> Result<()>;

    fn prev_chapter(&mut self) -> Result<()>;
}
