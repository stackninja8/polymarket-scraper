use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Shared metrics state
#[derive(Clone)]
pub struct Metrics {
    total_scrapes: Arc<AtomicU64>,
    successful_scrapes: Arc<AtomicU64>,
    failed_scrapes: Arc<AtomicU64>,
    last_scrape_time: Arc<Mutex<Option<chrono::DateTime<Utc>>>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            total_scrapes: Arc::new(AtomicU64::new(0)),
            successful_scrapes: Arc::new(AtomicU64::new(0)),
            failed_scrapes: Arc::new(AtomicU64::new(0)),
            last_scrape_time: Arc::new(Mutex::new(None)),
        }
    }

    pub fn record_scrape(&self, success: bool) {
        self.total_scrapes.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successful_scrapes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_scrapes.fetch_add(1, Ordering::Relaxed);
        }
        
        // Update timestamp - quick operation, safe to use blocking Mutex
        if let Ok(mut last_time) = self.last_scrape_time.lock() {
            *last_time = Some(Utc::now());
        }
    }

    pub fn get_total_scrapes(&self) -> u64 {
        self.total_scrapes.load(Ordering::Relaxed)
    }

    pub fn get_successful_scrapes(&self) -> u64 {
        self.successful_scrapes.load(Ordering::Relaxed)
    }

    pub fn get_failed_scrapes(&self) -> u64 {
        self.failed_scrapes.load(Ordering::Relaxed)
    }

    pub fn get_last_scrape_time(&self) -> Option<chrono::DateTime<Utc>> {
        self.last_scrape_time
            .lock()
            .ok()
            .and_then(|guard| *guard)
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        let metrics = Metrics::new();
        
        assert_eq!(metrics.get_total_scrapes(), 0);
        assert_eq!(metrics.get_successful_scrapes(), 0);
        assert_eq!(metrics.get_failed_scrapes(), 0);
        
        metrics.record_scrape(true);
        assert_eq!(metrics.get_total_scrapes(), 1);
        assert_eq!(metrics.get_successful_scrapes(), 1);
        assert_eq!(metrics.get_failed_scrapes(), 0);
        
        metrics.record_scrape(false);
        assert_eq!(metrics.get_total_scrapes(), 2);
        assert_eq!(metrics.get_successful_scrapes(), 1);
        assert_eq!(metrics.get_failed_scrapes(), 1);
        
        let last_time = metrics.get_last_scrape_time();
        assert!(last_time.is_some());
    }
}

