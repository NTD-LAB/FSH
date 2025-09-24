use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct RateLimiter {
    requests: RwLock<HashMap<String, Vec<Instant>>>,
    max_requests: usize,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_duration: Duration) -> Self {
        Self {
            requests: RwLock::new(HashMap::new()),
            max_requests,
            window_duration,
        }
    }

    pub async fn allow(&self, identifier: String) -> bool {
        let now = Instant::now();

        let mut requests = self.requests.write().await;
        let request_times = requests.entry(identifier).or_insert_with(Vec::new);

        // Remove old requests outside the window
        request_times.retain(|&time| now.duration_since(time) < self.window_duration);

        // Check if we're within the limit
        if request_times.len() < self.max_requests {
            request_times.push(now);
            true
        } else {
            false
        }
    }

    pub async fn get_remaining(&self, identifier: &str) -> usize {
        let now = Instant::now();

        let requests = self.requests.read().await;
        if let Some(request_times) = requests.get(identifier) {
            let recent_requests = request_times.iter()
                .filter(|&&time| now.duration_since(time) < self.window_duration)
                .count();

            self.max_requests.saturating_sub(recent_requests)
        } else {
            self.max_requests
        }
    }

    pub async fn reset(&self, identifier: &str) {
        let mut requests = self.requests.write().await;
        requests.remove(identifier);
    }

    pub async fn cleanup_expired(&self) {
        let now = Instant::now();
        let mut requests = self.requests.write().await;

        for request_times in requests.values_mut() {
            request_times.retain(|&time| now.duration_since(time) < self.window_duration);
        }

        // Remove empty entries
        requests.retain(|_, times| !times.is_empty());
    }

    pub async fn get_stats(&self) -> RateLimiterStats {
        let requests = self.requests.read().await;
        RateLimiterStats {
            tracked_identifiers: requests.len(),
            max_requests_per_window: self.max_requests,
            window_duration_secs: self.window_duration.as_secs(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimiterStats {
    pub tracked_identifiers: usize,
    pub max_requests_per_window: usize,
    pub window_duration_secs: u64,
}

#[derive(Debug)]
pub struct AdaptiveRateLimiter {
    base_limiter: RateLimiter,
    suspicious_ips: RwLock<HashMap<String, SuspiciousActivity>>,
}

#[derive(Debug, Clone)]
struct SuspiciousActivity {
    violations: usize,
    last_violation: Instant,
    reduced_limit: usize,
}

impl AdaptiveRateLimiter {
    pub fn new(max_requests: usize, window_duration: Duration) -> Self {
        Self {
            base_limiter: RateLimiter::new(max_requests, window_duration),
            suspicious_ips: RwLock::new(HashMap::new()),
        }
    }

    pub async fn allow(&self, identifier: String) -> bool {
        let effective_limit = {
            let suspicious_ips = self.suspicious_ips.read().await;
            if let Some(activity) = suspicious_ips.get(&identifier) {
                // If this IP has suspicious activity, use reduced limit
                activity.reduced_limit
            } else {
                self.base_limiter.max_requests
            }
        };

        // Create a temporary limiter with the effective limit
        let _temp_limiter = RateLimiter::new(effective_limit, self.base_limiter.window_duration);

        // Check the base limiter first
        let allowed = self.base_limiter.allow(identifier.clone()).await;

        if !allowed {
            // Record this as a violation
            self.record_violation(identifier).await;
        }

        allowed
    }

    async fn record_violation(&self, identifier: String) {
        let mut suspicious_ips = self.suspicious_ips.write().await;
        let activity = suspicious_ips.entry(identifier).or_insert_with(|| SuspiciousActivity {
            violations: 0,
            last_violation: Instant::now(),
            reduced_limit: self.base_limiter.max_requests / 2, // Start with half the normal limit
        });

        activity.violations += 1;
        activity.last_violation = Instant::now();

        // Progressively reduce the limit for repeat offenders
        if activity.violations > 5 {
            activity.reduced_limit = activity.reduced_limit.saturating_sub(1).max(1);
        }
    }

    pub async fn mark_suspicious(&self, identifier: String) {
        let mut suspicious_ips = self.suspicious_ips.write().await;
        suspicious_ips.insert(identifier, SuspiciousActivity {
            violations: 10, // High violation count
            last_violation: Instant::now(),
            reduced_limit: 1, // Severely limited
        });
    }

    pub async fn cleanup_expired(&self) {
        // Clean up base limiter
        self.base_limiter.cleanup_expired().await;

        // Clean up suspicious activity (expire after 1 hour)
        let now = Instant::now();
        let expire_duration = Duration::from_secs(3600);

        let mut suspicious_ips = self.suspicious_ips.write().await;
        suspicious_ips.retain(|_, activity| {
            now.duration_since(activity.last_violation) < expire_duration
        });
    }

    pub async fn get_remaining(&self, identifier: &str) -> usize {
        let suspicious_ips = self.suspicious_ips.read().await;
        let effective_limit = if let Some(activity) = suspicious_ips.get(identifier) {
            activity.reduced_limit
        } else {
            self.base_limiter.max_requests
        };

        let used = self.base_limiter.max_requests - self.base_limiter.get_remaining(identifier).await;
        effective_limit.saturating_sub(used)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(3, Duration::from_secs(1));

        // Should allow first 3 requests
        assert!(limiter.allow("client1".to_string()).await);
        assert!(limiter.allow("client1".to_string()).await);
        assert!(limiter.allow("client1".to_string()).await);

        // Should block the 4th request
        assert!(!limiter.allow("client1".to_string()).await);

        // Different client should still be allowed
        assert!(limiter.allow("client2".to_string()).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_window() {
        let limiter = RateLimiter::new(2, Duration::from_millis(100));

        // Use up the limit
        assert!(limiter.allow("client1".to_string()).await);
        assert!(limiter.allow("client1".to_string()).await);
        assert!(!limiter.allow("client1".to_string()).await);

        // Wait for window to expire
        sleep(Duration::from_millis(150)).await;

        // Should be allowed again
        assert!(limiter.allow("client1".to_string()).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_get_remaining() {
        let limiter = RateLimiter::new(5, Duration::from_secs(1));

        assert_eq!(limiter.get_remaining("client1").await, 5);

        limiter.allow("client1".to_string()).await;
        assert_eq!(limiter.get_remaining("client1").await, 4);

        limiter.allow("client1".to_string()).await;
        assert_eq!(limiter.get_remaining("client1").await, 3);
    }

    #[tokio::test]
    async fn test_adaptive_rate_limiter() {
        let limiter = AdaptiveRateLimiter::new(3, Duration::from_secs(1));

        // Normal operation
        assert!(limiter.allow("client1".to_string()).await);
        assert!(limiter.allow("client1".to_string()).await);
        assert!(limiter.allow("client1".to_string()).await);

        // This should trigger violation recording
        assert!(!limiter.allow("client1".to_string()).await);

        // Mark as suspicious
        limiter.mark_suspicious("client2".to_string()).await;

        // Suspicious client should have very limited access
        assert!(limiter.get_remaining("client2").await <= 1);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let limiter = RateLimiter::new(2, Duration::from_millis(50));

        // Generate some requests
        limiter.allow("client1".to_string()).await;
        limiter.allow("client2".to_string()).await;

        // Wait for expiration
        sleep(Duration::from_millis(100)).await;

        // Clean up
        limiter.cleanup_expired().await;

        // Should have cleaned up the old entries
        let stats = limiter.get_stats().await;
        assert_eq!(stats.tracked_identifiers, 0);
    }
}