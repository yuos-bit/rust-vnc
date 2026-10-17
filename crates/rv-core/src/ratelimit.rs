use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Sliding-window failure limiter with a temporary lockout.
#[derive(Debug, Clone, Copy)]
pub struct RateLimitPolicy {
    pub max_failures: u32,
    pub window: Duration,
    pub lockout: Duration,
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            // CLAUDE.md §10.4: block brute-force enumeration of 6-character codes.
            max_failures: 5,
            window: Duration::from_secs(10 * 60),
            lockout: Duration::from_secs(5 * 60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitDecision {
    Allowed,
    Blocked { retry_after: Duration },
}

#[derive(Debug)]
struct Entry {
    failures: VecDeque<Instant>,
    locked_until: Option<Instant>,
    last_seen: Instant,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            failures: VecDeque::new(),
            locked_until: None,
            last_seen: Instant::now(),
        }
    }
}

/// In-memory limiter keyed by `"<device_code>|<client_ip>"`.
///
/// The server additionally persists every attempt in the pairing log; this
/// structure is the hot-path gate and is intentionally cheap.
#[derive(Debug)]
pub struct AttemptLimiter {
    policy: RateLimitPolicy,
    entries: HashMap<String, Entry>,
}

impl AttemptLimiter {
    pub fn new(policy: RateLimitPolicy) -> Self {
        Self {
            policy,
            entries: HashMap::new(),
        }
    }

    pub fn policy(&self) -> RateLimitPolicy {
        self.policy
    }

    pub fn check(&mut self, key: &str, now: Instant) -> LimitDecision {
        let Some(entry) = self.entries.get_mut(key) else {
            return LimitDecision::Allowed;
        };
        entry.last_seen = now;

        if let Some(until) = entry.locked_until {
            if now < until {
                return LimitDecision::Blocked {
                    retry_after: until.saturating_duration_since(now),
                };
            }
            entry.locked_until = None;
            entry.failures.clear();
        }

        prune(entry, now, self.policy.window);
        LimitDecision::Allowed
    }

    pub fn record_failure(&mut self, key: &str, now: Instant) {
        let policy = self.policy;
        let entry = self.entries.entry(key.to_owned()).or_insert_with(|| Entry {
            last_seen: now,
            ..Default::default()
        });
        entry.last_seen = now;
        entry.failures.push_back(now);
        prune(entry, now, policy.window);
        if entry.failures.len() as u32 >= policy.max_failures {
            entry.locked_until = Some(now + policy.lockout);
            entry.failures.clear();
        }
    }

    pub fn record_success(&mut self, key: &str) {
        self.entries.remove(key);
    }

    /// Drops entries that have been quiet for longer than the window. Call it
    /// periodically from a maintenance task to bound memory.
    pub fn cleanup(&mut self, now: Instant) {
        let window = self.policy.window;
        self.entries.retain(|_, entry| {
            let recent_failure = entry
                .failures
                .back()
                .is_some_and(|t| now.duration_since(*t) < window);
            let locked = entry.locked_until.is_some_and(|t| now < t);
            let fresh = now.duration_since(entry.last_seen) < window;
            recent_failure || locked || fresh
        });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn prune(entry: &mut Entry, now: Instant, window: Duration) {
    while let Some(front) = entry.failures.front() {
        if now.saturating_duration_since(*front) > window {
            entry.failures.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> AttemptLimiter {
        AttemptLimiter::new(RateLimitPolicy {
            max_failures: 3,
            window: Duration::from_secs(60),
            lockout: Duration::from_secs(30),
        })
    }

    #[test]
    fn allows_below_threshold() {
        let now = Instant::now();
        let mut l = limiter();
        l.record_failure("k", now);
        l.record_failure("k", now + Duration::from_secs(1));
        assert_eq!(
            l.check("k", now + Duration::from_secs(2)),
            LimitDecision::Allowed
        );
    }

    #[test]
    fn locks_after_threshold_and_expires() {
        let now = Instant::now();
        let mut l = limiter();
        l.record_failure("k", now);
        l.record_failure("k", now + Duration::from_secs(1));
        l.record_failure("k", now + Duration::from_secs(2));
        assert!(matches!(
            l.check("k", now + Duration::from_secs(3)),
            LimitDecision::Blocked { .. }
        ));
        assert_eq!(
            l.check("k", now + Duration::from_secs(40)),
            LimitDecision::Allowed
        );
    }

    #[test]
    fn failure_window_is_sliding() {
        let now = Instant::now();
        let mut l = limiter();
        l.record_failure("k", now);
        l.record_failure("k", now + Duration::from_secs(40));
        l.record_failure("k", now + Duration::from_secs(80));
        // The first failure aged out of the 60s window.
        assert_eq!(
            l.check("k", now + Duration::from_secs(81)),
            LimitDecision::Allowed
        );
    }

    #[test]
    fn success_resets_state() {
        let now = Instant::now();
        let mut l = limiter();
        l.record_failure("k", now);
        l.record_failure("k", now);
        l.record_success("k");
        assert!(l.is_empty());
    }

    #[test]
    fn cleanup_drops_stale_keys() {
        let now = Instant::now();
        let mut l = limiter();
        l.record_failure("old", now);
        l.cleanup(now + Duration::from_secs(120));
        assert!(l.is_empty());
    }
}
