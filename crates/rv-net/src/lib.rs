//! Network transport abstraction.
//!
//! RustVNC embeds Tailscale on the desktop through a small Go sidecar built on
//! `tailscale.com/tsnet`. The sidecar listens on a loopback SOCKS5 port inside
//! the tailnet; Rust dials `100.x.y.z` through it. Android uses the official
//! Tailscale VpnService instead, so the same address reaches the endpoint.
//!
//! `Direct` exists for local development and for deployments where the host
//! already runs Tailscale in kernel mode.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NetMode {
    #[default]
    Direct,
    Socks5 {
        proxy: String,
    },
}

impl NetMode {
    /// `socks5://127.0.0.1:1055` -> `Socks5 { proxy: "127.0.0.1:1055" }`.
    pub fn from_setting(value: &str) -> Self {
        let value = value.trim();
        if let Some(rest) = value.strip_prefix("socks5://") {
            Self::Socks5 {
                proxy: rest.trim_end_matches('/').to_owned(),
            }
        } else if value.is_empty() || value.eq_ignore_ascii_case("direct") {
            Self::Direct
        } else if value.starts_with("127.0.0.1:") || value.starts_with("localhost:") {
            Self::Socks5 {
                proxy: value.to_owned(),
            }
        } else {
            Self::Direct
        }
    }
}

/// Returns true when `ip` belongs to the Tailscale CGNAT range 100.64.0.0/10.
pub fn is_tailnet_ip(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

/// Parses and validates a Tailscale address. Loopback/private LAN addresses
/// are rejected for control sessions because they would bypass the tailnet.
pub fn parse_tailnet_ip(value: &str) -> Result<Ipv4Addr, NetError> {
    let ip: Ipv4Addr = value
        .trim()
        .parse()
        .map_err(|_| NetError::InvalidAddress(value.to_owned()))?;
    if !is_tailnet_ip(ip) {
        return Err(NetError::InvalidAddress(value.to_owned()));
    }
    Ok(ip)
}

/// Opens a TCP connection to `host:port`, either directly or through the
/// tailnet sidecar's SOCKS5 listener.
pub async fn connect(
    host: &str,
    port: u16,
    mode: &NetMode,
    timeout: Duration,
) -> Result<TcpStream, NetError> {
    if host.is_empty() || port == 0 {
        return Err(NetError::InvalidAddress(host.to_owned()));
    }

    match mode {
        NetMode::Direct => {
            let addr = format!("{host}:{port}");
            let stream = tokio::time::timeout(timeout, TcpStream::connect(&addr))
                .await
                .map_err(|_| NetError::Timeout(addr.clone()))??;
            stream.set_nodelay(true)?;
            Ok(stream)
        }
        NetMode::Socks5 { proxy } => {
            let stream = tokio::time::timeout(timeout, socks5::connect(proxy, host, port))
                .await
                .map_err(|_| NetError::Timeout(proxy.clone()))??;
            stream.set_nodelay(true)?;
            Ok(stream)
        }
    }
}

/// Binds an inbound listener. Agents default to loopback and let the sidecar
/// forward tailnet port 21416 to it.
pub async fn listen(bind: &str, port: u16) -> Result<tokio::net::TcpListener, NetError> {
    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .map_err(|_| NetError::InvalidAddress(format!("{bind}:{port}")))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    Ok(listener)
}

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("connection timed out: {0}")]
    Timeout(String),
    #[error("socks5 negotiation failed: {0}")]
    Socks5(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub mod socks5 {
    //! Minimal SOCKS5 CONNECT client (RFC 1928), no authentication.
    //!
    //! Names are sent as domain names so MagicDNS resolution happens inside
    //! the tsnet stack instead of the host resolver.

    use std::net::IpAddr;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::NetError;

    const SOCKS_VERSION: u8 = 0x05;
    const METHOD_NO_AUTH: u8 = 0x00;
    const CMD_CONNECT: u8 = 0x01;
    const ATYP_IPV4: u8 = 0x01;
    const ATYP_DOMAIN: u8 = 0x03;
    const ATYP_IPV6: u8 = 0x04;

    pub async fn connect(proxy: &str, host: &str, port: u16) -> Result<TcpStream, NetError> {
        let mut stream = TcpStream::connect(proxy)
            .await
            .map_err(|e| NetError::Socks5(format!("connect {proxy}: {e}")))?;

        stream
            .write_all(&[SOCKS_VERSION, 1, METHOD_NO_AUTH])
            .await?;
        let mut greeting = [0u8; 2];
        stream.read_exact(&mut greeting).await?;
        if greeting != [SOCKS_VERSION, METHOD_NO_AUTH] {
            return Err(NetError::Socks5(format!(
                "proxy rejected no-auth method: {greeting:?}"
            )));
        }

        let mut request = vec![SOCKS_VERSION, CMD_CONNECT, 0x00];
        match host.parse::<IpAddr>() {
            Ok(IpAddr::V4(ip)) => {
                request.push(ATYP_IPV4);
                request.extend_from_slice(&ip.octets());
            }
            Ok(IpAddr::V6(ip)) => {
                request.push(ATYP_IPV6);
                request.extend_from_slice(&ip.octets());
            }
            Err(_) => {
                let bytes = host.as_bytes();
                if bytes.len() > 255 {
                    return Err(NetError::Socks5("hostname too long".into()));
                }
                request.push(ATYP_DOMAIN);
                request.push(bytes.len() as u8);
                request.extend_from_slice(bytes);
            }
        }
        request.extend_from_slice(&port.to_be_bytes());
        stream.write_all(&request).await?;

        let mut reply = [0u8; 4];
        stream.read_exact(&mut reply).await?;
        if reply[0] != SOCKS_VERSION {
            return Err(NetError::Socks5("bad reply version".into()));
        }
        if reply[1] != 0x00 {
            return Err(NetError::Socks5(format!(
                "connect rejected with code 0x{:02x}",
                reply[1]
            )));
        }

        match reply[3] {
            ATYP_IPV4 => {
                let mut skip = [0u8; 4 + 2];
                stream.read_exact(&mut skip).await?;
            }
            ATYP_IPV6 => {
                let mut skip = [0u8; 16 + 2];
                stream.read_exact(&mut skip).await?;
            }
            ATYP_DOMAIN => {
                let mut len = [0u8; 1];
                stream.read_exact(&mut len).await?;
                let mut skip = vec![0u8; len[0] as usize + 2];
                stream.read_exact(&mut skip).await?;
            }
            other => {
                return Err(NetError::Socks5(format!("unknown address type {other}")));
            }
        }

        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailnet_range() {
        assert!(is_tailnet_ip("100.64.0.1".parse().unwrap()));
        assert!(is_tailnet_ip("100.127.255.254".parse().unwrap()));
        assert!(!is_tailnet_ip("100.63.255.255".parse().unwrap()));
        assert!(!is_tailnet_ip("100.128.0.1".parse().unwrap()));
        assert!(!is_tailnet_ip("10.0.0.2".parse().unwrap()));
    }

    #[test]
    fn parse_rejects_lan_addresses() {
        assert!(parse_tailnet_ip("100.100.1.2").is_ok());
        assert!(parse_tailnet_ip("192.168.1.10").is_err());
        assert!(parse_tailnet_ip("localhost").is_err());
    }

    #[test]
    fn settings_parse() {
        assert_eq!(NetMode::from_setting(""), NetMode::Direct);
        assert_eq!(NetMode::from_setting("direct"), NetMode::Direct);
        assert_eq!(
            NetMode::from_setting("socks5://127.0.0.1:1055"),
            NetMode::Socks5 {
                proxy: "127.0.0.1:1055".into()
            }
        );
    }
}
