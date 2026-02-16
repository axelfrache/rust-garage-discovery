use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

#[derive(Debug)]
pub enum Error<E> {
    CircuitOpen,
    Inner(E),
}

struct Inner {
    last_failure: Option<Instant>,
}

pub struct CircuitBreaker {
    failures: AtomicUsize,
    threshold: usize,
    reset_timeout: Duration,
    inner: Mutex<Inner>,
}

impl CircuitBreaker {
    pub fn new(threshold: usize, reset_timeout: Duration) -> Self {
        Self {
            failures: AtomicUsize::new(0),
            threshold,
            reset_timeout,
            inner: Mutex::new(Inner { last_failure: None }),
        }
    }

    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, Error<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if self.failures.load(Ordering::Relaxed) >= self.threshold {
            let mut inner = self.inner.lock().await;
            if let Some(last_failure) = inner.last_failure {
                if last_failure.elapsed() < self.reset_timeout {
                    return Err(Error::CircuitOpen);
                }
            }
        }

        match f().await {
            Ok(val) => {
                self.failures.store(0, Ordering::Relaxed);
                Ok(val)
            }
            Err(err) => {
                let _count = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
                let mut inner = self.inner.lock().await;
                inner.last_failure = Some(Instant::now());
                Err(Error::Inner(err))
            }
        }
    }
}
