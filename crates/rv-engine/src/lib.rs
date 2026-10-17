//! Session engines shared by the desktop application and (through a thin
//! wrapper) the mobile client.

pub mod agent;
pub mod client;
pub mod codec;
pub mod controller;

pub use agent::{AgentConfig, AgentEvent, AgentHandle, AgentIdentity, AgentService};
pub use client::ServerClient;
pub use codec::{read_message, write_message};
pub use controller::{
    ConnectedInfo, ControllerConfig, ControllerEvent, ControllerHandle, ControllerService,
};

use rv_proto::ApiError;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("server rejected request ({code}): {message}")]
    Api { code: String, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Net(#[from] rv_net::NetError),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error(transparent)]
    Codec(#[from] rv_proto::CodecError),
    #[error(transparent)]
    Capture(#[from] rv_capture::CaptureError),
    #[error("timed out: {0}")]
    Timeout(String),
    #[error("configuration error: {0}")]
    Config(String),
}

impl From<ApiError> for EngineError {
    fn from(value: ApiError) -> Self {
        Self::Api {
            code: value.error.code,
            message: value.error.message,
        }
    }
}

pub(crate) fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if getrandom::fill(&mut buf).is_err() {
        // Cryptographic randomness is unavailable; session ids are not
        // credentials, so a time-derived fallback is acceptable here.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        return format!("{now:x}");
    }
    let mut out = String::with_capacity(bytes * 2);
    for byte in buf {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
