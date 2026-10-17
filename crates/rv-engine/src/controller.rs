//! Controller engine: resolves a device code through the pairing backend,
//! dials the controlled endpoint inside the tailnet, and streams input.

use std::time::Duration;

use rv_proto::{
    ClientMessage, ConnectRequest, Hello, InputEvent, ServerMessage, VideoFrame, WIRE_VERSION,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::EngineError;
use crate::client::ServerClient;
use crate::codec::{read_message, write_message};
use rv_net::NetMode;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(12);
const READ_TIMEOUT: Duration = Duration::from_secs(20);
const PING_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct ControllerConfig {
    pub server_url: String,
    pub device_code: String,
    pub token: String,
    pub controller_id: String,
    pub controller_name: String,
    pub platform: String,
    pub net_mode: NetMode,
    pub connect_timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct ConnectedInfo {
    pub device_code: String,
    pub device_name: String,
    pub session_id: String,
    pub width: u32,
    pub height: u32,
    pub fps: u16,
    pub transport: String,
    pub tailscale_ip: String,
}

#[derive(Debug)]
pub enum ControllerEvent {
    Frame(VideoFrame),
    Status(String),
    Disconnected { reason: String },
}

pub struct ControllerHandle {
    pub info: ConnectedInfo,
    pub events: mpsc::Receiver<ControllerEvent>,
    out_tx: mpsc::UnboundedSender<ClientMessage>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl ControllerHandle {
    pub fn send_input(&self, event: InputEvent) -> bool {
        let result = self.out_tx.send(ClientMessage::Input(event));
        if result.is_err() {
            debug!("input channel closed");
        }
        result.is_ok()
    }

    pub fn stop(&self) {
        let _ = self.out_tx.send(ClientMessage::Bye);
        self.cancel.cancel();
    }

    pub fn is_running(&self) -> bool {
        !self.task.is_finished()
    }

    /// Moves the event stream out of the handle.
    pub fn take_events(&mut self) -> mpsc::Receiver<ControllerEvent> {
        let (_tx, replacement) = mpsc::channel(1);
        std::mem::replace(&mut self.events, replacement)
    }
}

impl Drop for ControllerHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub struct ControllerService;

impl ControllerService {
    pub async fn connect(config: ControllerConfig) -> Result<ControllerHandle, EngineError> {
        let client = ServerClient::new(&config.server_url)?;
        let target = client
            .connect(&ConnectRequest {
                device_code: config.device_code.clone(),
                token: config.token.clone(),
                controller_id: config.controller_id.clone(),
            })
            .await?;

        if target.tailscale_ip.trim().is_empty() {
            return Err(EngineError::Protocol(
                "server returned an empty tailnet address".into(),
            ));
        }

        let transport = match &config.net_mode {
            NetMode::Direct => "direct".to_string(),
            NetMode::Socks5 { proxy } => format!("tailscale via {proxy}"),
        };

        let stream = rv_net::connect(
            &target.tailscale_ip,
            target.port,
            &config.net_mode,
            config.connect_timeout,
        )
        .await?;
        let (mut read_half, mut write_half) = stream.into_split();

        let hello = ClientMessage::Hello(Hello::new(
            config.device_code.clone(),
            config.token.clone(),
            config.controller_id.clone(),
            config.controller_name.clone(),
            config.platform.clone(),
        ));
        write_message(&mut write_half, &hello).await?;

        let response = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            read_message::<_, ServerMessage>(&mut read_half),
        )
        .await
        .map_err(|_| EngineError::Timeout("waiting for session hello".into()))??;

        let server_hello = match response {
            Some(ServerMessage::Hello(hello)) => hello,
            Some(ServerMessage::Error(error)) => {
                return Err(EngineError::Protocol(format!(
                    "{}: {}",
                    error.code, error.message
                )));
            }
            other => {
                return Err(EngineError::Protocol(format!(
                    "unexpected handshake message: {other:?}"
                )));
            }
        };

        if !server_hello.accepted {
            return Err(EngineError::Protocol(
                "endpoint rejected the session".into(),
            ));
        }
        if server_hello.wire_version != WIRE_VERSION {
            return Err(EngineError::Protocol(format!(
                "wire version {} is not supported (expected {WIRE_VERSION})",
                server_hello.wire_version
            )));
        }

        let info = ConnectedInfo {
            device_code: config.device_code.clone(),
            device_name: server_hello.device_name.clone(),
            session_id: server_hello.session_id.clone(),
            width: server_hello.width,
            height: server_hello.height,
            fps: server_hello.fps,
            transport,
            tailscale_ip: target.tailscale_ip.clone(),
        };

        let (events_tx, events_rx) = mpsc::channel(4);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let cancel = CancellationToken::new();

        let writer_cancel = cancel.clone();
        let writer = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = writer_cancel.cancelled() => break,
                    message = out_rx.recv() => {
                        match message {
                            Some(message) => {
                                if let Err(error) = write_message(&mut write_half, &message).await {
                                    debug!(%error, "session write failed");
                                    writer_cancel.cancel();
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        let ping_tx = out_tx.clone();
        let ping_cancel = cancel.clone();
        let pinger = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(PING_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut id = 0u64;
            loop {
                tokio::select! {
                    _ = ping_cancel.cancelled() => break,
                    _ = ticker.tick() => {
                        id += 1;
                        if ping_tx.send(ClientMessage::Ping { id }).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let reader_cancel = cancel.clone();
        let reader_events = events_tx.clone();
        let reader = tokio::spawn(async move {
            let mut last_error: Option<String> = None;
            loop {
                let incoming = tokio::select! {
                    _ = reader_cancel.cancelled() => break,
                    value = tokio::time::timeout(
                        READ_TIMEOUT,
                        read_message::<_, ServerMessage>(&mut read_half),
                    ) => value,
                };

                match incoming {
                    Err(_) => {
                        last_error = Some("connection timed out".into());
                        break;
                    }
                    Ok(Err(error)) => {
                        last_error = Some(format!("protocol error: {error}"));
                        break;
                    }
                    Ok(Ok(None)) => {
                        last_error = Some("endpoint closed the session".into());
                        break;
                    }
                    Ok(Ok(Some(ServerMessage::Video(frame)))) => {
                        if reader_events
                            .try_send(ControllerEvent::Frame(frame))
                            .is_err()
                        {
                            // The UI is behind; dropping the newest frame keeps
                            // latency bounded instead of queueing stale video.
                            debug!("dropping frame: UI queue full");
                        }
                    }
                    Ok(Ok(Some(ServerMessage::Pong { .. }))) => {}
                    Ok(Ok(Some(ServerMessage::Hello(_)))) => {}
                    Ok(Ok(Some(ServerMessage::Error(error)))) => {
                        last_error = Some(format!("{}: {}", error.code, error.message));
                        break;
                    }
                }
            }

            reader_cancel.cancel();
            let reason = last_error.unwrap_or_else(|| "disconnected".into());
            let _ = reader_events
                .send(ControllerEvent::Disconnected {
                    reason: reason.clone(),
                })
                .await;
            info!(%reason, "controller session ended");
        });

        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = writer => {}
                _ = pinger => {}
                _ = reader => {}
                _ = task_cancel.cancelled() => {}
            }
        });

        Ok(ControllerHandle {
            info,
            events: events_rx,
            out_tx,
            cancel,
            task,
        })
    }
}
