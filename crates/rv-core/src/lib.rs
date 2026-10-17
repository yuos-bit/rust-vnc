//! Shared primitives used by every RustVNC component.
//!
//! The crate is deliberately dependency-light so it can be compiled for the
//! desktop endpoints, the pairing server and (through FFI) mobile targets.

pub mod code;
pub mod geometry;
pub mod ratelimit;
pub mod secrets;

pub use code::{CodeError, generate_device_code, normalize_device_code};
pub use geometry::{Size, aspect_fit, normalize_point};
pub use ratelimit::{AttemptLimiter, LimitDecision, RateLimitPolicy};
pub use secrets::{
    constant_time_eq, generate_agent_secret, generate_password, generate_token, hash_password,
    hash_secret_hex, verify_password,
};

/// Protocol version spoken by the pairing API and the control session.
///
/// Bump the major number on incompatible changes; both endpoints refuse to
/// negotiate across majors.
pub const PROTOCOL_VERSION: u16 = 1;
