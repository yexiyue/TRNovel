use std::sync::Arc;

use tokio::{
    sync::Semaphore,
    time::{Duration, interval},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub sem: Arc<Semaphore>,
    pub cancel_token: CancellationToken,
}

impl TokenBucket {
    pub fn new(capacity: usize, rate: Duration) -> Self {
        let sem = Arc::new(Semaphore::new(capacity));
        let cancel_token = CancellationToken::new();
        tokio::spawn({
            let sem = sem.clone();
            let cancel_token = cancel_token.clone();
            let mut interval = interval(rate);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            async move {
                loop {
                    tokio::select! {
                        _ = cancel_token.cancelled() => break,
                        _ = interval.tick() => {
                            if sem.available_permits() < capacity {
                                sem.add_permits(1);
                            }
                        }
                    }
                }
            }
        });

        Self { sem, cancel_token }
    }

    pub async fn acquire(&self) {
        let permits = self.sem.acquire().await.unwrap();
        permits.forget();
    }
}

impl Drop for TokenBucket {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}
