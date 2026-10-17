//! Controlled-endpoint engine: registers with the pairing server, accepts
//! authenticated control sessions, streams JPEG frames and injects input.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rv_proto::{
    ChangePasswordRequest, ClientMessage, FrameFormat, HeartbeatRequest, MAGIC, PairedController,
    RegisterDeviceRequest, ServerError, ServerHello, ServerMessage, VerifySessionRequest,
    VideoFrame, WIRE_VERSION,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::client::ServerClient;
use crate::codec::{read_message, write_message};
use crate::{EngineError, now_ms, random_hex};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TEXT_LEN: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub device_id: String,
    pub device_code: String,
    /// Kept locally so the controlled PC can display it. The server stores
    /// only an Argon2id hash.
    pub password: String,
    pub agent_secret: String,
    pub headscale: Option<rv_proto::HeadscaleBootstrap>,
    #[serde(default)]
    pub tailscale_ip: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AgentConfig {
    pub server_url: String,
    pub name: String,
    pub platform: String,
    pub version: String,
    /// Loopback by default; the tsnet sidecar forwards the tailnet port here.
    pub bind: String,
    pub port: u16,
    pub fps: u16,
    pub quality: u8,
    pub max_width: u32,
    pub max_sessions: usize,
    pub tailscale_ip: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8080".into(),
            name: "My PC".into(),
            platform: std::env::consts::OS.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            bind: "127.0.0.1".into(),
            port: rv_proto::DEFAULT_PORT,
            fps: 15,
            quality: 70,
            max_width: 0,
            max_sessions: 1,
            tailscale_ip: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Registered(AgentIdentity),
    Status {
        online: bool,
        listening: bool,
        clients: usize,
        message: String,
    },
    PairedControllers(Vec<PairedController>),
    SessionStarted {
        controller: String,
        session_id: String,
    },
    SessionEnded {
        controller: String,
        reason: String,
    },
}

pub struct AgentHandle {
    pub identity: AgentIdentity,
    pub events: mpsc::UnboundedReceiver<AgentEvent>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl AgentHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn is_running(&self) -> bool {
        !self.task.is_finished()
    }

    /// Moves the event stream out of the handle so a UI layer can forward it
    /// while the handle itself stays in shared state.
    pub fn take_events(&mut self) -> mpsc::UnboundedReceiver<AgentEvent> {
        let (_, replacement) = mpsc::unbounded_channel();
        std::mem::replace(&mut self.events, replacement)
    }
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub struct AgentService;

impl AgentService {
    /// Binds the local listener, then registers (unless an identity exists)
    /// and spawns the accept/heartbeat loops.
    pub async fn start(
        config: AgentConfig,
        identity: Option<AgentIdentity>,
    ) -> Result<AgentHandle, EngineError> {
        let listener = rv_net::listen(&config.bind, config.port).await?;
        let client = ServerClient::new(&config.server_url)?;

        let mut identity = match identity {
            Some(identity) => identity,
            None => register(&client, &config).await?,
        };
        if let Some(ip) = &config.tailscale_ip {
            identity.tailscale_ip = Some(ip.clone());
        }

        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run_agent(
            config,
            identity.clone(),
            client,
            listener,
            events_tx,
            cancel.clone(),
        ));

        Ok(AgentHandle {
            identity,
            events: events_rx,
            cancel,
            task,
        })
    }
}

async fn register(
    client: &ServerClient,
    config: &AgentConfig,
) -> Result<AgentIdentity, EngineError> {
    let response = client
        .register(&RegisterDeviceRequest {
            name: config.name.clone(),
            platform: config.platform.clone(),
            fingerprint: fingerprint(),
            version: config.version.clone(),
        })
        .await?;

    Ok(AgentIdentity {
        device_id: response.device_id,
        device_code: response.device_code,
        password: response.password,
        agent_secret: response.agent_secret,
        headscale: response.headscale,
        tailscale_ip: config.tailscale_ip.clone(),
    })
}

async fn run_agent(
    config: AgentConfig,
    identity: AgentIdentity,
    client: ServerClient,
    listener: tokio::net::TcpListener,
    events: mpsc::UnboundedSender<AgentEvent>,
    cancel: CancellationToken,
) {
    info!(code = %identity.device_code, port = config.port, "agent listening");
    let _ = events.send(AgentEvent::Status {
        online: false,
        listening: true,
        clients: 0,
        message: "监听中，等待配对".into(),
    });

    let active = Arc::new(AtomicUsize::new(0));
    let heartbeat = tokio::spawn(heartbeat_loop(
        client.clone(),
        config.clone(),
        identity.clone(),
        events.clone(),
        active.clone(),
        cancel.clone(),
    ));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(value) => value,
                    Err(error) => {
                        warn!(%error, "accept failed");
                        continue;
                    }
                };
                if active.load(Ordering::Relaxed) >= config.max_sessions.max(1) {
                    warn!(%peer, "rejecting session: concurrency limit reached");
                    tokio::spawn(reject_busy(stream));
                    continue;
                }
                active.fetch_add(1, Ordering::Relaxed);
                let client = client.clone();
                let config = config.clone();
                let identity = identity.clone();
                let events = events.clone();
                let cancel = cancel.clone();
                let active = active.clone();
                tokio::spawn(async move {
                    serve_session(
                        stream,
                        peer.to_string(),
                        client,
                        config,
                        identity,
                        events,
                        active,
                        cancel,
                    )
                    .await;
                });
            }
        }
    }

    heartbeat.abort();
    info!("agent stopped");
}

async fn heartbeat_loop(
    client: ServerClient,
    config: AgentConfig,
    identity: AgentIdentity,
    events: mpsc::UnboundedSender<AgentEvent>,
    active: Arc<AtomicUsize>,
    cancel: CancellationToken,
) {
    // Give the UI a moment to attach its event listeners before the first
    // status event, so "starting" does not overwrite a freshly delivered
    // "online" snapshot.
    tokio::select! {
        _ = cancel.cancelled() => return,
        _ = tokio::time::sleep(Duration::from_millis(450)) => {}
    }

    let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = ticker.tick() => {}
        }

        let request = HeartbeatRequest {
            device_code: identity.device_code.clone(),
            agent_secret: identity.agent_secret.clone(),
            tailscale_ip: identity.tailscale_ip.clone(),
            listen_port: config.port,
            version: config.version.clone(),
        };
        match client.heartbeat(&request).await {
            Ok(response) => {
                let _ = events.send(AgentEvent::Status {
                    online: true,
                    listening: true,
                    clients: active.load(Ordering::Relaxed),
                    message: "已连接配对服务器".into(),
                });
                let _ = events.send(AgentEvent::PairedControllers(response.paired_controllers));
            }
            Err(error) => {
                let _ = events.send(AgentEvent::Status {
                    online: false,
                    listening: true,
                    clients: active.load(Ordering::Relaxed),
                    message: format!("配对服务器不可达：{error}"),
                });
            }
        }
    }
}

async fn reject_busy(mut stream: TcpStream) {
    let error = ServerMessage::Error(ServerError {
        code: "busy".into(),
        message: "controlled endpoint already has an active session".into(),
    });
    let _ = tokio::time::timeout(Duration::from_secs(2), write_message(&mut stream, &error)).await;
}

#[allow(unused_assignments)]
#[allow(clippy::too_many_arguments)]
async fn serve_session(
    stream: TcpStream,
    peer: String,
    client: ServerClient,
    config: AgentConfig,
    identity: AgentIdentity,
    events: mpsc::UnboundedSender<AgentEvent>,
    active: Arc<AtomicUsize>,
    cancel: CancellationToken,
) {
    let peers_now = active.load(Ordering::Relaxed);
    let _ = events.send(AgentEvent::Status {
        online: true,
        listening: true,
        clients: peers_now,
        message: "控制端已连接".into(),
    });

    let mut reason = String::new();
    let (mut read_half, mut write_half) = stream.into_split();

    let hello = match tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        read_message::<_, ClientMessage>(&mut read_half),
    )
    .await
    {
        Ok(Ok(Some(ClientMessage::Hello(hello)))) => hello,
        other => {
            warn!(%peer, ?other, "session rejected: expected hello");
            let _ = write_message(
                &mut write_half,
                &ServerMessage::Error(ServerError {
                    code: "bad_handshake".into(),
                    message: "expected hello".into(),
                }),
            )
            .await;
            active.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    };

    let rejection = if hello.magic != MAGIC {
        Some(("bad_magic", "wrong protocol magic"))
    } else if hello.wire_version != WIRE_VERSION {
        Some(("version_mismatch", "unsupported wire version"))
    } else if hello.device_code != identity.device_code {
        Some(("wrong_device", "device code does not match this endpoint"))
    } else {
        None
    };
    if let Some((code, message)) = rejection {
        let _ = write_message(
            &mut write_half,
            &ServerMessage::Error(ServerError {
                code: code.into(),
                message: message.into(),
            }),
        )
        .await;
        active.fetch_sub(1, Ordering::Relaxed);
        return;
    }

    let verification = client
        .verify_session(&VerifySessionRequest {
            device_code: identity.device_code.clone(),
            agent_secret: identity.agent_secret.clone(),
            token: hello.token.clone(),
            controller_id: hello.controller_id.clone(),
        })
        .await;

    match verification {
        Ok(response) if response.valid => {}
        Ok(_) => {
            let _ = write_message(
                &mut write_half,
                &ServerMessage::Error(ServerError {
                    code: "unauthorized".into(),
                    message: "pairing token is invalid or revoked".into(),
                }),
            )
            .await;
            active.fetch_sub(1, Ordering::Relaxed);
            return;
        }
        Err(error) => {
            warn!(%peer, %error, "token verification failed");
            let _ = write_message(
                &mut write_half,
                &ServerMessage::Error(ServerError {
                    code: "auth_unavailable".into(),
                    message: "pairing server unreachable; failing closed".into(),
                }),
            )
            .await;
            active.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    }

    let capture_config = rv_capture::CaptureConfig {
        max_width: config.max_width,
        quality: config.quality,
    };
    let first_frame =
        tokio::task::spawn_blocking(move || rv_capture::capture_jpeg(&capture_config)).await;

    let first_frame = match first_frame {
        Ok(Ok(frame)) => frame,
        Ok(Err(error)) => {
            let _ = write_message(
                &mut write_half,
                &ServerMessage::Error(ServerError {
                    code: "capture_failed".into(),
                    message: error.to_string(),
                }),
            )
            .await;
            active.fetch_sub(1, Ordering::Relaxed);
            return;
        }
        Err(error) => {
            let _ = write_message(
                &mut write_half,
                &ServerMessage::Error(ServerError {
                    code: "capture_failed".into(),
                    message: error.to_string(),
                }),
            )
            .await;
            active.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    };

    let session_id = random_hex(8);
    let hello_out = ServerMessage::Hello(ServerHello {
        wire_version: WIRE_VERSION,
        accepted: true,
        session_id: session_id.clone(),
        device_name: config.name.clone(),
        width: first_frame.width,
        height: first_frame.height,
        fps: config.fps,
    });
    if write_message(&mut write_half, &hello_out).await.is_err() {
        active.fetch_sub(1, Ordering::Relaxed);
        return;
    }

    let _ = events.send(AgentEvent::SessionStarted {
        controller: hello.controller_name.clone(),
        session_id: session_id.clone(),
    });
    info!(%peer, controller = %hello.controller_name, "session started");

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if write_message(&mut write_half, &message).await.is_err() {
                break;
            }
        }
    });

    let mut seq = 0u64;
    let frame_delay = Duration::from_millis(1000 / u64::from(config.fps.max(1)));
    let mut ticker = tokio::time::interval(frame_delay);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_activity = Instant::now();

    // Send the frame used to size the handshake immediately.
    seq += 1;
    let _ = out_tx.send(ServerMessage::Video(VideoFrame {
        seq,
        width: first_frame.width,
        height: first_frame.height,
        timestamp_ms: now_ms(),
        format: FrameFormat::Jpeg,
        data: first_frame.jpeg,
    }));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                reason = "agent stopped".into();
                break;
            }
            _ = ticker.tick() => {
                if last_activity.elapsed() > SESSION_IDLE_TIMEOUT {
                    reason = "controller timed out".into();
                    break;
                }
                let capture_config = rv_capture::CaptureConfig {
                    max_width: config.max_width,
                    quality: config.quality,
                };
                let captured = tokio::task::spawn_blocking(move || {
                    rv_capture::capture_jpeg(&capture_config)
                })
                .await;
                match captured {
                    Ok(Ok(frame)) => {
                        seq += 1;
                        let message = ServerMessage::Video(VideoFrame {
                            seq,
                            width: frame.width,
                            height: frame.height,
                            timestamp_ms: now_ms(),
                            format: FrameFormat::Jpeg,
                            data: frame.jpeg,
                        });
                        if out_tx.send(message).is_err() {
                            reason = "writer stopped".into();
                            break;
                        }
                    }
                    Ok(Err(error)) => {
                        warn!(%error, "frame capture failed");
                    }
                    Err(error) => {
                        warn!(%error, "capture task failed");
                    }
                }
            }
            incoming = read_message::<_, ClientMessage>(&mut read_half) => {
                last_activity = Instant::now();
                match incoming {
                    Ok(Some(ClientMessage::Input(mut event))) => {
                        if let rv_proto::InputEvent::Text { text } = &mut event
                            && text.len() > MAX_TEXT_LEN
                        {
                            text.truncate(MAX_TEXT_LEN);
                        }
                        rv_input::inject(&event);
                    }
                    Ok(Some(ClientMessage::Ping { id })) => {
                        let _ = out_tx.send(ServerMessage::Pong { id });
                    }
                    Ok(Some(ClientMessage::Bye)) => {
                        reason = "controller disconnected".into();
                        break;
                    }
                    Ok(Some(ClientMessage::Hello(_))) => {
                        debug!("ignoring duplicate hello");
                    }
                    Ok(None) => {
                        reason = "controller closed the connection".into();
                        break;
                    }
                    Err(error) => {
                        reason = format!("protocol error: {error}");
                        break;
                    }
                }
            }
        }
    }

    if reason.is_empty() {
        reason = "connection closed".into();
    }
    writer.abort();
    let remaining = active.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
    let _ = events.send(AgentEvent::SessionEnded {
        controller: hello.controller_name.clone(),
        reason: reason.clone(),
    });
    let _ = events.send(AgentEvent::Status {
        online: true,
        listening: true,
        clients: remaining,
        message: "等待配对".into(),
    });
    info!(%peer, %reason, "session ended");
}

/// Rotates the pairing password via the backend and updates the local copy.
pub async fn change_password(
    server_url: &str,
    identity: &mut AgentIdentity,
    new_password: Option<String>,
) -> Result<(), EngineError> {
    let client = ServerClient::new(server_url)?;
    let response = client
        .change_password(&ChangePasswordRequest {
            device_code: identity.device_code.clone(),
            agent_secret: identity.agent_secret.clone(),
            new_password,
        })
        .await?;
    identity.password = response.password;
    Ok(())
}

fn fingerprint() -> String {
    // The identity file already stores the device code; the fingerprint only
    // helps an administrator recognise duplicate registrations.
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into());
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    format!("{host}/{user}")
}
