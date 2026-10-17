mod commands;
mod config;
mod model;
mod secure_store;

use std::path::Path;
use std::sync::Mutex;

use base64::Engine as _;
use rv_engine::{AgentEvent, AgentHandle, AgentIdentity, ControllerEvent, ControllerHandle};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc;

use crate::model::{
    AgentIdentityView, AgentStatusPayload, FramePayload, PairedListPayload, PairingSessionPayload,
    SessionNoticePayload, SessionStatePayload,
};

#[derive(Debug)]
pub struct RuntimeStatus {
    pub online: bool,
    pub clients: usize,
    pub message: String,
}

pub struct AppState {
    pub dir: std::path::PathBuf,
    pub settings: Mutex<config::Settings>,
    pub identity: Mutex<Option<AgentIdentity>>,
    pub runtime: Mutex<RuntimeStatus>,
    pub agent: AsyncMutex<Option<AgentHandle>>,
    pub controller: AsyncMutex<Option<ControllerHandle>>,
    pub agent_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    pub session_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&dir)?;
            init_logging(&dir);
            let settings = config::load_settings(&dir);
            // Persist the generated controller id on first launch; pairing
            // tokens are bound to it across restarts.
            let _ = config::save_settings(&dir, &settings);
            let identity = config::load_identity(&dir);
            tracing::info!(
                dir = %dir.display(),
                server = %settings.server_url,
                registered = identity.is_some(),
                "RustVNC desktop starting"
            );
            app.manage(AppState {
                dir,
                settings: Mutex::new(settings),
                identity: Mutex::new(identity),
                runtime: Mutex::new(RuntimeStatus {
                    online: false,
                    clients: 0,
                    message: "被控服务尚未启动".into(),
                }),
                agent: AsyncMutex::new(None),
                controller: AsyncMutex::new(None),
                agent_task: Mutex::new(None),
                session_task: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::save_settings,
            commands::set_theme,
            commands::start_agent,
            commands::stop_agent,
            commands::forget_agent,
            commands::rotate_agent_password,
            commands::enroll_network,
            commands::revoke_controller,
            commands::pair_device,
            commands::list_saved_devices,
            commands::remove_saved_device,
            commands::connect_device,
            commands::disconnect_session,
            commands::send_input,
            commands::send_text,
            commands::window_minimize,
            commands::window_toggle_maximize,
            commands::window_close,
            commands::toggle_fullscreen,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the RustVNC desktop application");
}

fn init_logging(dir: &Path) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,rv_pc_lib=debug,rv_engine=debug")
    });
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("rustvnc.log"))
    {
        Ok(file) => {
            let writer = std::sync::Mutex::new(file);
            let _ = tracing_subscriber::fmt()
                .with_writer(writer)
                .with_ansi(false)
                .with_env_filter(filter)
                .try_init();
        }
        Err(_) => {
            let _ = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_env_filter(filter)
                .try_init();
        }
    }
}

pub(crate) async fn forward_agent_events(
    app: AppHandle,
    mut events: mpsc::UnboundedReceiver<AgentEvent>,
) {
    while let Some(event) = events.recv().await {
        match event {
            AgentEvent::Registered(identity) => {
                let _ = app.emit("agent://identity", AgentIdentityView::from(&identity));
            }
            AgentEvent::Status {
                online,
                listening,
                clients,
                message,
            } => {
                {
                    let state = app.state::<AppState>();
                    let mut runtime = state.runtime.lock().unwrap();
                    runtime.online = online;
                    runtime.clients = clients;
                    runtime.message = message.clone();
                }
                let _ = app.emit(
                    "agent://status",
                    AgentStatusPayload {
                        online,
                        listening,
                        clients,
                        message,
                    },
                );
            }
            AgentEvent::PairedControllers(controllers) => {
                let _ = app.emit("agent://paired", PairedListPayload { controllers });
            }
            AgentEvent::SessionStarted {
                controller,
                session_id,
            } => {
                let _ = app.emit(
                    "agent://session",
                    PairingSessionPayload {
                        state: "started".into(),
                        controller,
                        session_id: Some(session_id),
                        reason: None,
                    },
                );
            }
            AgentEvent::SessionEnded { controller, reason } => {
                let _ = app.emit(
                    "agent://session",
                    PairingSessionPayload {
                        state: "ended".into(),
                        controller,
                        session_id: None,
                        reason: Some(reason),
                    },
                );
            }
        }
    }
}

pub(crate) async fn forward_controller_events(
    app: AppHandle,
    mut events: mpsc::Receiver<ControllerEvent>,
) {
    while let Some(event) = events.recv().await {
        match event {
            ControllerEvent::Frame(frame) => {
                let payload = FramePayload {
                    seq: frame.seq,
                    width: frame.width,
                    height: frame.height,
                    timestamp_ms: frame.timestamp_ms,
                    data_b64: base64::engine::general_purpose::STANDARD.encode(&frame.data),
                };
                if app.emit("session://frame", payload).is_err() {
                    break;
                }
            }
            ControllerEvent::Status(message) => {
                let _ = app.emit("session://notice", SessionNoticePayload { message });
            }
            ControllerEvent::Disconnected { reason } => {
                let _ = app.emit(
                    "session://state",
                    SessionStatePayload {
                        state: "disconnected".into(),
                        reason: Some(reason),
                        info: None,
                    },
                );
                let app_for_cleanup = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_for_cleanup.state::<AppState>();
                    let mut slot = state.controller.lock().await;
                    if let Some(handle) = slot.take() {
                        handle.stop();
                    }
                });
                break;
            }
        }
    }
}
