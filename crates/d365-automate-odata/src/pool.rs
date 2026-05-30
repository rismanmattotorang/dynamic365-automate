//! Connection pool primitive.
//!
//! A thin semaphore-based concurrency limiter — it does not manage live HTTP
//! connections (the trait-based [`crate::client::D365Client`] design lets
//! backends decide whether to pool or open per-call). What it enforces is the
//! upper bound: at most `cap` concurrent in-flight calls, with a pool-exhausted
//! error when the bound is hit.

use crate::error::{D365Error, D365Result};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
pub struct ConnectionPool {
    cap: usize,
    sem: Arc<Semaphore>,
}

impl ConnectionPool {
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            cap,
            sem: Arc::new(Semaphore::new(cap)),
        }
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    pub fn available(&self) -> usize {
        self.sem.available_permits()
    }

    pub async fn acquire(&self) -> D365Result<OwnedSemaphorePermit> {
        Arc::clone(&self.sem)
            .acquire_owned()
            .await
            .map_err(|_| D365Error::PoolExhausted { cap: self.cap })
    }

    /// Try to acquire without waiting.  Returns `PoolExhausted` if no slot
    /// is immediately free.
    pub fn try_acquire(&self) -> D365Result<OwnedSemaphorePermit> {
        Arc::clone(&self.sem)
            .try_acquire_owned()
            .map_err(|_| D365Error::PoolExhausted { cap: self.cap })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cap_is_enforced() {
        let pool = ConnectionPool::new(2);
        let _a = pool.acquire().await.unwrap();
        let _b = pool.acquire().await.unwrap();
        assert!(pool.try_acquire().is_err());
        assert_eq!(pool.available(), 0);
    }

    #[tokio::test]
    async fn slot_released_on_drop() {
        let pool = ConnectionPool::new(1);
        {
            let _g = pool.acquire().await.unwrap();
        }
        assert_eq!(pool.available(), 1);
    }
}
