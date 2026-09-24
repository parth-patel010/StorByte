//! Request scheduler with priorities, reserved demand slots, and exponential backoff.
//!
//! UserRead uses a reserved semaphore so Prefetch/Background cannot starve Explorer
//! (STREAMBUS-style dual path). Prefetch shares a separate pool.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Background = 3,
    Prefetch = 2,
    UserRead = 1,
}

pub struct RequestScheduler {
    /// Reserved for demand / open first-byte (never taken by prefetch).
    user: Arc<Semaphore>,
    /// Prefetch + background.
    shared: Arc<Semaphore>,
}

impl RequestScheduler {
    /// Split `max_concurrent` into reserved user slots (≥ half, min 1) and shared slots.
    pub fn new(max_concurrent: usize) -> Self {
        let max = max_concurrent.max(2);
        let user_slots = (max.div_ceil(2)).max(1);
        let shared_slots = (max - user_slots).max(1);
        Self {
            user: Arc::new(Semaphore::new(user_slots)),
            shared: Arc::new(Semaphore::new(shared_slots)),
        }
    }

    pub async fn acquire(&self, priority: Priority) -> OwnedSemaphorePermit {
        match priority {
            // Demand never shares a pool with prefetch — reserved slots only.
            Priority::UserRead => self
                .user
                .clone()
                .acquire_owned()
                .await
                .expect("user semaphore closed"),
            Priority::Prefetch | Priority::Background => self
                .shared
                .clone()
                .acquire_owned()
                .await
                .expect("shared semaphore closed"),
        }
    }
}

pub async fn with_backoff<F, Fut, T, E>(mut attempts: u32, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    attempts = attempts.max(1);
    let mut delay = Duration::from_millis(200);
    let mut last_err = None;
    for i in 0..attempts {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                tracing::warn!(attempt = i + 1, error = %e, "request failed; backing off");
                last_err = Some(e);
                if i + 1 < attempts {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(8));
                }
            }
        }
    }
    Err(last_err.expect("attempts >= 1"))
}
