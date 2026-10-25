use std::time::{SystemTime, UNIX_EPOCH};

use rv_core::normalize_device_code;
use rv_engine::agent::change_password as rotate_password_call;
use rv_engine::{
    AgentConfig, AgentService, ControllerConfig, ControllerService, EngineError, ServerClient,
};
use rv_proto::{
    EnrollControllerRequest, EnrollControllerResponse, InputEvent, PairRequest, RevokeTokenRequest,
};
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::AppState;
use crate::config::{self, SavedDevice, Settings};
use crate::model::{
    AgentIdentityView, AgentSnapshot, BootstrapPayload, SavedDeviceView, SessionInfoPayload,
    SessionStatePayload,
};

#[tauri::command]
pub async fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapPayload, String> {
    let settings = state.settings.lock().unwrap().clone();
    let saved_devices = config::load_devices(&state.dir)
        .iter()
        .map(SavedDeviceView::from)
        .collect();
    let agent = snapshot(&state).await;
    Ok(BootstrapPayload {
        settings,
        agent,
        saved_devices,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        controller_id: state.settings.lock().unwrap().controller_id.clone(),
    })
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    mut settings: Settings,
) -> Result<Settings, String> {
    settings.sanitize()?;
    config::save_settings(&state.dir, &settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings://updated", &settings);
    Ok(settings)
}

#[tauri::command]
pub fn embedded_env_entries() -> Vec<crate::embedded_env::Entry> {
    crate::embedded_env::entries()
}

#[tauri::command]
pub async fn start_agent(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AgentSnapshot, String> {
    let mut slot = state.agent.lock().await;
    if slot.is_some() {
        drop(slot);
        return Ok(snapshot(&state).await);
    }

    let settings = state.settings.lock().unwrap().clone();
    let identity = state
        .identity
        .lock()
        .unwrap()
        .clone()
        .or_else(|| config::load_identity(&state.dir));

    let config = AgentConfig {
        server_url: settings.server_url.clone(),
        name: settings.device_name.clone(),
        platform: std::env::consts::OS.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        bind: settings.listen_bind.clone(),
        port: settings.listen_port,
        fps: settings.fps,
        quality: settings.quality,
        max_width: settings.max_width,
        max_sessions: 1,
        tailscale_ip: non_empty(&settings.tailscale_ip),
    };

    let mut handle = AgentService::start(config, identity)
        .await
        .map_err(|error| format!("启动被控服务失败：{}", engine_error(error)))?;
    let identity = handle.identity.clone();
    config::save_identity(&state.dir, &identity)?;
    *state.identity.lock().unwrap() = Some(identity.clone());

    let events = handle.take_events();
    let app_for_task = app.clone();
    let task = tauri::async_runtime::spawn(crate::forward_agent_events(app_for_task, events));
    *state.agent_task.lock().unwrap() = Some(task);
    *slot = Some(handle);
    drop(slot);

    {
        let mut runtime = state.runtime.lock().unwrap();
        runtime.message = "正在连接配对服务器…".into();
    }

    let _ = app.emit("agent://identity", AgentIdentityView::from(&identity));
    Ok(snapshot(&state).await)
}

#[tauri::command]
pub async fn stop_agent(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AgentSnapshot, String> {
    {
        let mut slot = state.agent.lock().await;
        if let Some(handle) = slot.take() {
            handle.stop();
        }
    }
    if let Some(task) = state.agent_task.lock().unwrap().take() {
        task.abort();
    }
    {
        let mut runtime = state.runtime.lock().unwrap();
        runtime.online = false;
        runtime.clients = 0;
        runtime.message = "被控服务已停止".into();
    }
    let _ = app.emit(
        "agent://status",
        crate::model::AgentStatusPayload {
            online: false,
            listening: false,
            clients: 0,
            message: "被控服务已停止".into(),
        },
    );
    Ok(snapshot(&state).await)
}

#[tauri::command]
pub async fn forget_agent(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AgentSnapshot, String> {
    {
        let mut slot = state.agent.lock().await;
        if let Some(handle) = slot.take() {
            handle.stop();
        }
    }
    if let Some(task) = state.agent_task.lock().unwrap().take() {
        task.abort();
    }
    config::clear_identity(&state.dir);
    *state.identity.lock().unwrap() = None;
    {
        let mut runtime = state.runtime.lock().unwrap();
        runtime.online = false;
        runtime.clients = 0;
        runtime.message = "尚未注册".into();
    }
    let _ = app.emit("agent://identity", Option::<AgentIdentityView>::None);
    Ok(snapshot(&state).await)
}

#[tauri::command]
pub async fn rotate_agent_password(
    app: AppHandle,
    state: State<'_, AppState>,
    new_password: Option<String>,
) -> Result<String, String> {
    let server_url = state.settings.lock().unwrap().server_url.clone();
    let mut identity = state
        .identity
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "尚未注册被控端".to_owned())?;
    rotate_password_call(&server_url, &mut identity, new_password)
        .await
        .map_err(engine_error)?;
    config::save_identity(&state.dir, &identity)?;
    *state.identity.lock().unwrap() = Some(identity.clone());
    let _ = app.emit("agent://identity", AgentIdentityView::from(&identity));
    Ok(identity.password)
}

/// Persists the UI theme ("dark" or "light") without touching other settings.
#[tauri::command]
pub fn set_theme(
    app: AppHandle,
    state: State<'_, AppState>,
    theme: String,
) -> Result<String, String> {
    let theme = if theme == "light" { "light" } else { "dark" };
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        settings.theme = theme.to_owned();
        config::save_settings(&state.dir, &settings)?;
        settings.clone()
    };
    let _ = app.emit("settings://updated", &updated);
    Ok(theme.to_owned())
}

/// Requests a single-use tailnet pre-auth key for this controller. The key is
/// displayed once and is not persisted by the application.
#[tauri::command]
pub async fn enroll_network(
    state: State<'_, AppState>,
) -> Result<EnrollControllerResponse, String> {
    let settings = state.settings.lock().unwrap().clone();
    let client = ServerClient::new(&settings.server_url).map_err(engine_error)?;
    client
        .enroll_controller(&EnrollControllerRequest {
            name: settings.controller_name.clone(),
            platform: std::env::consts::OS.to_owned(),
            fingerprint: config::hostname(),
            controller_id: settings.controller_id.clone(),
        })
        .await
        .map_err(engine_error)
}

#[tauri::command]
pub async fn revoke_controller(state: State<'_, AppState>, token_id: String) -> Result<(), String> {
    let server_url = state.settings.lock().unwrap().server_url.clone();
    let identity = state
        .identity
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "尚未注册被控端".to_owned())?;
    let client = ServerClient::new(&server_url).map_err(engine_error)?;
    client
        .revoke_token(&RevokeTokenRequest {
            device_code: identity.device_code,
            agent_secret: identity.agent_secret,
            token_id,
        })
        .await
        .map_err(engine_error)
}

#[tauri::command]
pub async fn pair_device(
    app: AppHandle,
    state: State<'_, AppState>,
    code: String,
    password: String,
    name: Option<String>,
) -> Result<SavedDeviceView, String> {
    let settings = state.settings.lock().unwrap().clone();
    let code = normalize_device_code(&code).map_err(|error| error.to_string())?;
    let client = ServerClient::new(&settings.server_url).map_err(engine_error)?;
    let response = client
        .pair(&PairRequest {
            device_code: code,
            password,
            controller_id: settings.controller_id.clone(),
            controller_name: settings.controller_name.clone(),
            platform: std::env::consts::OS.to_owned(),
        })
        .await
        .map_err(engine_error)?;

    let saved = SavedDevice {
        device_code: response.device_code.clone(),
        device_name: name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(response.device_name),
        server_url: settings.server_url.clone(),
        token: response.token,
        token_id: response.token_id,
        tailscale_ip: response.tailscale_ip,
        platform: std::env::consts::OS.to_owned(),
        online: response.online,
        last_connected: None,
    };

    let mut devices = config::load_devices(&state.dir);
    devices.retain(|device| {
        !(device.device_code == saved.device_code && device.server_url == saved.server_url)
    });
    devices.insert(0, saved.clone());
    config::save_devices(&state.dir, &devices)?;

    let view = SavedDeviceView::from(&saved);
    let _ = app.emit("devices://updated", &view);
    Ok(view)
}

#[tauri::command]
pub fn list_saved_devices(state: State<'_, AppState>) -> Vec<SavedDeviceView> {
    config::load_devices(&state.dir)
        .iter()
        .map(SavedDeviceView::from)
        .collect()
}

#[tauri::command]
pub fn remove_saved_device(
    app: AppHandle,
    state: State<'_, AppState>,
    code: String,
) -> Result<Vec<SavedDeviceView>, String> {
    let mut devices = config::load_devices(&state.dir);
    devices.retain(|device| device.device_code != code);
    config::save_devices(&state.dir, &devices)?;
    let views: Vec<_> = devices.iter().map(SavedDeviceView::from).collect();
    let _ = app.emit("devices://updated", &views);
    Ok(views)
}

#[tauri::command]
pub async fn connect_device(
    app: AppHandle,
    state: State<'_, AppState>,
    code: String,
) -> Result<SessionInfoPayload, String> {
    let settings = state.settings.lock().unwrap().clone();
    let devices = config::load_devices(&state.dir);
    let saved = devices
        .iter()
        .find(|device| device.device_code == code)
        .cloned()
        .ok_or_else(|| "未找到该设备的免密令牌，请先配对".to_owned())?;

    {
        let mut slot = state.controller.lock().await;
        if let Some(handle) = slot.take() {
            handle.stop();
        }
    }
    if let Some(task) = state.session_task.lock().unwrap().take() {
        task.abort();
    }

    let server_url = if saved.server_url.is_empty() {
        settings.server_url.clone()
    } else {
        saved.server_url.clone()
    };
    let config = ControllerConfig {
        server_url,
        device_code: saved.device_code.clone(),
        token: saved.token.clone(),
        controller_id: settings.controller_id.clone(),
        controller_name: settings.controller_name.clone(),
        platform: std::env::consts::OS.to_owned(),
        net_mode: rv_net::NetMode::from_setting(&settings.network_mode),
        connect_timeout: std::time::Duration::from_secs(12),
    };

    let mut handle = ControllerService::connect(config)
        .await
        .map_err(engine_error)?;
    let info = SessionInfoPayload::from(&handle.info);
    let events = handle.take_events();
    let app_for_task = app.clone();
    let task = tauri::async_runtime::spawn(crate::forward_controller_events(app_for_task, events));
    *state.session_task.lock().unwrap() = Some(task);
    *state.controller.lock().await = Some(handle);

    let now = now_secs();
    let mut devices = config::load_devices(&state.dir);
    if let Some(entry) = devices.iter_mut().find(|device| device.device_code == code) {
        entry.last_connected = Some(now);
        entry.tailscale_ip = Some(info.tailscale_ip.clone());
        entry.online = true;
        let _ = config::save_devices(&state.dir, &devices);
    }

    let _ = app.emit(
        "session://state",
        SessionStatePayload {
            state: "connected".into(),
            reason: None,
            info: Some(info.clone()),
        },
    );
    Ok(info)
}

#[tauri::command]
pub async fn disconnect_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut slot = state.controller.lock().await;
        if let Some(handle) = slot.take() {
            handle.stop();
        }
    }
    if let Some(task) = state.session_task.lock().unwrap().take() {
        task.abort();
    }
    let _ = app.emit(
        "session://state",
        SessionStatePayload {
            state: "disconnected".into(),
            reason: Some("用户断开连接".into()),
            info: None,
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn send_input(state: State<'_, AppState>, event: InputEvent) -> Result<(), String> {
    let slot = state.controller.lock().await;
    let handle = slot.as_ref().ok_or_else(|| "当前没有活动会话".to_owned())?;
    if handle.send_input(event) {
        Ok(())
    } else {
        Err("会话已断开".into())
    }
}

#[tauri::command]
pub async fn send_text(state: State<'_, AppState>, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    if text.chars().count() > 4096 {
        return Err("单次发送的文本过长".into());
    }
    let slot = state.controller.lock().await;
    let handle = slot.as_ref().ok_or_else(|| "当前没有活动会话".to_owned())?;
    if handle.send_input(InputEvent::Text { text }) {
        Ok(())
    } else {
        Err("会话已断开".into())
    }
}

#[tauri::command]
pub fn window_minimize(window: WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn window_toggle_maximize(window: WebviewWindow) -> Result<bool, String> {
    let maximized = window.is_maximized().map_err(|error| error.to_string())?;
    if maximized {
        window.unmaximize().map_err(|error| error.to_string())?;
    } else {
        window.maximize().map_err(|error| error.to_string())?;
    }
    Ok(!maximized)
}

#[tauri::command]
pub fn window_close(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn toggle_fullscreen(window: WebviewWindow) -> Result<bool, String> {
    let fullscreen = window.is_fullscreen().map_err(|error| error.to_string())?;
    window
        .set_fullscreen(!fullscreen)
        .map_err(|error| error.to_string())?;
    Ok(!fullscreen)
}

pub(crate) async fn snapshot(state: &AppState) -> AgentSnapshot {
    let running = state.agent.lock().await.is_some();
    let (online, clients, message) = {
        let runtime = state.runtime.lock().unwrap();
        (runtime.online, runtime.clients, runtime.message.clone())
    };
    let identity = state
        .identity
        .lock()
        .unwrap()
        .clone()
        .or_else(|| config::load_identity(&state.dir));
    AgentSnapshot {
        running,
        online,
        clients,
        message,
        identity: identity.as_ref().map(AgentIdentityView::from),
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn engine_error(error: EngineError) -> String {
    match error {
        EngineError::Api { message, .. } => message,
        EngineError::Http(message) => format!("无法连接配对服务器：{message}"),
        EngineError::Timeout(context) => format!("连接超时：{context}"),
        EngineError::Net(error) => format!("网络连接失败：{error}"),
        EngineError::Capture(error) => format!("屏幕采集失败：{error}"),
        other => other.to_string(),
    }
}
