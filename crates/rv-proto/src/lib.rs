//! Shared protocol definitions for RustVNC.

pub mod api;
pub mod wire;

pub use api::*;
pub use wire::{
    ClientMessage, CodecError, DEFAULT_PORT, FrameFormat, Hello, InputEvent, MAGIC,
    MAX_FRAME_BYTES, MouseButton, ServerError, ServerHello, ServerMessage, VideoFrame,
    WIRE_VERSION, decode, encode,
};
