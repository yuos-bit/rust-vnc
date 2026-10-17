//! Length-prefixed message framing shared by both roles.

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use rv_proto::{MAX_FRAME_BYTES, decode, encode};

use crate::EngineError;

/// Reads one `u32_le length || body` message. Returns `Ok(None)` on a clean
/// EOF at a message boundary.
pub async fn read_message<R, T>(reader: &mut R) -> Result<Option<T>, EngineError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(EngineError::Io(error)),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
        return Err(EngineError::Protocol(format!(
            "invalid frame length: {len} bytes"
        )));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(Some(decode(&body)?))
}

/// Writes one framed message and flushes it.
pub async fn write_message<W, T>(writer: &mut W, message: &T) -> Result<(), EngineError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = encode(message)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(EngineError::Protocol(format!(
            "outgoing message too large: {} bytes",
            body.len()
        )));
    }
    writer.write_all(&(body.len() as u32).to_le_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rv_proto::ClientMessage;

    #[tokio::test]
    async fn roundtrip_through_duplex() {
        let (mut a, mut b) = tokio::io::duplex(4096);
        let message = ClientMessage::Ping { id: 42 };
        write_message(&mut a, &message).await.unwrap();
        let decoded: ClientMessage = read_message(&mut b).await.unwrap().unwrap();
        assert_eq!(decoded, message);
    }

    #[tokio::test]
    async fn clean_eof() {
        let (a, mut b) = tokio::io::duplex(16);
        drop(a);
        let decoded: Option<ClientMessage> = read_message(&mut b).await.unwrap();
        assert!(decoded.is_none());
    }

    #[tokio::test]
    async fn rejects_oversized_frame() {
        let (mut a, mut b) = tokio::io::duplex(16);
        a.write_all(&(MAX_FRAME_BYTES as u32 + 1).to_le_bytes())
            .await
            .unwrap();
        let result: Result<Option<ClientMessage>, _> = read_message(&mut b).await;
        assert!(matches!(result, Err(EngineError::Protocol(_))));
    }
}
