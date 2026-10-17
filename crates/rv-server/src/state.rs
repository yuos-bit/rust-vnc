use std::sync::{Arc, Mutex};

use rv_core::{AttemptLimiter, RateLimitPolicy};

use crate::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub admin_token: Arc<String>,
    /// Per-device-code failure window.
    pub code_limiter: Arc<Mutex<AttemptLimiter>>,
    /// Per-source-IP failure window.
    pub ip_limiter: Arc<Mutex<AttemptLimiter>>,
    /// Registration flood control.
    pub register_limiter: Arc<Mutex<AttemptLimiter>>,
    /// Constant-work dummy hash so unknown codes cost the same as a wrong
    /// password, preventing device-code enumeration by timing.
    pub dummy_hash: Arc<String>,
    pub trust_proxy: bool,
}

impl AppState {
    pub fn new(db: Db, admin_token: String, trust_proxy: bool) -> Self {
        let dummy_hash =
            rv_core::hash_password("rvnc-timing-equalizer").expect("Argon2 must initialize");
        Self {
            db: Arc::new(db),
            admin_token: Arc::new(admin_token),
            code_limiter: Arc::new(Mutex::new(AttemptLimiter::new(RateLimitPolicy::default()))),
            ip_limiter: Arc::new(Mutex::new(AttemptLimiter::new(RateLimitPolicy {
                max_failures: 15,
                window: std::time::Duration::from_secs(10 * 60),
                lockout: std::time::Duration::from_secs(10 * 60),
            }))),
            register_limiter: Arc::new(Mutex::new(AttemptLimiter::new(RateLimitPolicy {
                max_failures: 30,
                window: std::time::Duration::from_secs(60 * 60),
                lockout: std::time::Duration::from_secs(30 * 60),
            }))),
            dummy_hash: Arc::new(dummy_hash),
            trust_proxy,
        }
    }
}
