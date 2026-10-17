//! End-to-end control protocol.
//!
//! Transport: a single bidirectional TCP stream inside the Tailscale/WireGuard
//! tunnel. Each message is `u32_le length || bincode(message)` and the length
//! is capped by [`MAX_FRAME_BYTES`] before any allocation happens.
//!
//! The protocol deliberately carries no images other than encoded video
//! frames; the controller never scales non-uniformly (see `rv-core::geometry`).

use serde::{Deserialize, Serialize};

pub const MAGIC: [u8; 4] = *b"RVNC";
pub const WIRE_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 21416;

/// 16 MiB is enough for a 4K JPEG at high quality while keeping a corrupted
/// length prefix from causing an unbounded allocation.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClientMessage {
    Hello(Hello),
    Input(InputEvent),
    Ping { id: u64 },
    Bye,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hello {
    pub magic: [u8; 4],
    pub wire_version: u16,
    pub device_code: String,
    pub token: String,
    pub controller_id: String,
    pub controller_name: String,
    pub platform: String,
}

impl Hello {
    pub fn new(
        device_code: impl Into<String>,
        token: impl Into<String>,
        controller_id: impl Into<String>,
        controller_name: impl Into<String>,
        platform: impl Into<String>,
    ) -> Self {
        Self {
            magic: MAGIC,
            wire_version: WIRE_VERSION,
            device_code: device_code.into(),
            token: token.into(),
            controller_id: controller_id.into(),
            controller_name: controller_name.into(),
            platform: platform.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServerMessage {
    Hello(ServerHello),
    Video(VideoFrame),
    Pong { id: u64 },
    Error(ServerError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerHello {
    pub wire_version: u16,
    pub accepted: bool,
    pub session_id: String,
    pub device_name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoFrame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub timestamp_ms: u64,
    pub format: FrameFormat,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FrameFormat {
    Jpeg,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InputEvent {
    MouseMove {
        x: f64,
        y: f64,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    Wheel {
        delta_x: f64,
        delta_y: f64,
    },
    Key {
        code: String,
        pressed: bool,
    },
    /// Unicode text, used for IME input and characters without a stable
    /// physical key code.
    Text {
        text: String,
    },
    /// Best-effort clipboard sync from controller to controlled endpoint.
    Clipboard {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("frame too large: {0} bytes")]
    TooLarge(usize),
    #[error("bincode error: {0}")]
    Bincode(String),
}

/// Encodes a message body; the caller is responsible for the length prefix.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|e| CodecError::Bincode(e.to_string()))
}

/// Decodes a message body after the length prefix has been validated.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    let (value, consumed) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
        .map_err(|e| CodecError::Bincode(e.to_string()))?;
    if consumed != bytes.len() {
        return Err(CodecError::Bincode(format!(
            "trailing bytes: {consumed}/{}",
            bytes.len()
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrip() {
        let msg = ClientMessage::Hello(Hello::new("A2B3C4", "rvt_x", "ctrl-1", "Phone", "android"));
        let bytes = encode(&msg).unwrap();
        let decoded: ClientMessage = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn input_roundtrip() {
        for event in [
            InputEvent::MouseMove { x: 0.25, y: 0.75 },
            InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true,
            },
            InputEvent::Wheel {
                delta_x: 0.0,
                delta_y: -120.0,
            },
            InputEvent::Key {
                code: "KeyA".into(),
                pressed: false,
            },
            InputEvent::Text {
                text: "你好".into(),
            },
        ] {
            let msg = ClientMessage::Input(event);
            let bytes = encode(&msg).unwrap();
            let decoded: ClientMessage = decode(&bytes).unwrap();
            assert_eq!(msg, decoded);
        }
    }

    #[test]
    fn rejects_trailing_garbage() {
        let bytes = encode(&ClientMessage::Ping { id: 7 }).unwrap();
        let mut extended = bytes.clone();
        extended.push(0);
        assert!(decode::<ClientMessage>(&extended).is_err());
    }

    #[test]
    fn video_frame_roundtrip() {
        let msg = ServerMessage::Video(VideoFrame {
            seq: 10,
            width: 1920,
            height: 1080,
            timestamp_ms: 1234,
            format: FrameFormat::Jpeg,
            data: vec![1, 2, 3, 4],
        });
        let bytes = encode(&msg).unwrap();
        let decoded: ServerMessage = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }
}
