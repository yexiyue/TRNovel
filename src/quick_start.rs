use std::sync::Arc;

use anyhow::anyhow;
use tokio::sync::Mutex;

use crate::{
    book_source::BookSourceCache,
    novel::{local_novel::LocalNovel, network_novel::NetworkNovel},
    pages::{network_novel::book_detail::BookDetail, ReadNovel},
    History, HistoryItem, Result, RoutePage,
};

pub async fn quick_start(
    history: History,
    book_sources: Arc<Mutex<BookSourceCache>>,
) -> Result<Box<dyn RoutePage>> {
    let (path, history_item) = history.histories.first().ok_or(anyhow!("没有历史记录"))?;

    match history_item {
        HistoryItem::Local { .. } => Ok(Box::new(ReadNovel::<LocalNovel>::to_page_route(
            path.into(),
        ))),
        HistoryItem::Network { .. } => {
            let novel = NetworkNovel::from_url(path, book_sources).await?;
            Ok(BookDetail::to_page_route(novel))
        }
    }
}
