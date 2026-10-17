//! End-to-end session test: controller -> mock pairing server -> agent.
//!
//! Requires an interactive Windows desktop because the agent captures the real
//! screen. Run manually with:
//!
//! ```text
//! cargo test -p rv-engine --test session_roundtrip -- --ignored --nocapture
//! ```

use std::time::Duration;

use rv_engine::{
    AgentConfig, AgentIdentity, AgentService, ControllerConfig, ControllerEvent, ControllerService,
};
use rv_net::NetMode;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const CODE: &str = "A2B3C4";
const AGENT_SECRET: &str = "agt_test_secret";
const TOKEN: &str = "rvt_test_token";

async fn read_http_request(stream: &mut TcpStream) -> Option<String> {
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = stream.read(&mut byte).await.ok()?;
        if read == 0 {
            return None;
        }
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() > 32 * 1024 {
            return None;
        }
    }
    let text = String::from_utf8_lossy(&header).to_string();
    let content_length = text
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        stream.read_exact(&mut body).await.ok()?;
    }
    Some(text)
}

async fn mock_server(agent_port: u16) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                while let Some(request) = read_http_request(&mut stream).await {
                    let path = request
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .to_owned();
                    let body = if path.contains("/heartbeat") {
                        json!({"online": true, "server_time": 0, "paired_controllers": []})
                    } else if path.contains("/session/verify") {
                        json!({"valid": true, "token_id": "tok_test", "controller_name": "Integration", "expires_at": null})
                    } else if path.contains("/connect") {
                        json!({
                            "device_code": CODE,
                            "device_name": "Mock Desktop",
                            "tailscale_ip": "127.0.0.1",
                            "port": agent_port,
                            "online": true
                        })
                    } else {
                        json!({"ok": true})
                    }
                    .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    if stream.write_all(response.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    format!("http://127.0.0.1:{}", address.port())
}

#[tokio::test]
#[ignore = "requires an interactive Windows desktop for screen capture"]
async fn controller_receives_a_frame_from_the_agent() {
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let agent_port = probe.local_addr().unwrap().port();
    drop(probe);

    let server_url = mock_server(agent_port).await;

    let identity = AgentIdentity {
        device_id: "dev_test".into(),
        device_code: CODE.into(),
        password: "TESTPASS".into(),
        agent_secret: AGENT_SECRET.into(),
        headscale: None,
        tailscale_ip: Some("127.0.0.1".into()),
    };

    let agent = AgentService::start(
        AgentConfig {
            server_url: server_url.clone(),
            name: "Mock Desktop".into(),
            platform: "windows".into(),
            version: "0.1.0".into(),
            bind: "127.0.0.1".into(),
            port: agent_port,
            fps: 5,
            quality: 50,
            max_width: 640,
            max_sessions: 1,
            tailscale_ip: Some("127.0.0.1".into()),
        },
        Some(identity),
    )
    .await
    .expect("agent should start");

    let mut controller = ControllerService::connect(ControllerConfig {
        server_url,
        device_code: CODE.into(),
        token: TOKEN.into(),
        controller_id: "ctrl_test".into(),
        controller_name: "Integration".into(),
        platform: "windows".into(),
        net_mode: NetMode::Direct,
        connect_timeout: Duration::from_secs(8),
    })
    .await
    .expect("controller should connect and finish the handshake");

    assert_eq!(controller.info.device_name, "Mock Desktop");
    assert!(controller.info.width > 0 && controller.info.height > 0);

    let frame = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match controller.events.recv().await {
                Some(ControllerEvent::Frame(frame)) => break Some(frame),
                Some(ControllerEvent::Disconnected { reason }) => {
                    panic!("session disconnected: {reason}");
                }
                Some(ControllerEvent::Status(_)) => {}
                None => break None,
            }
        }
    })
    .await
    .expect("timed out waiting for a video frame")
    .expect("event stream closed");

    assert!(frame.width > 0 && frame.height > 0);
    assert!(frame.width <= 640, "frame was not downscaled as configured");
    assert!(frame.data.starts_with(&[0xFF, 0xD8]), "frame is not JPEG");

    controller.stop();
    agent.stop();
}
