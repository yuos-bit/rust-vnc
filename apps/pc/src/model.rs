use serde::Serialize;

use rv_engine::{AgentIdentity, ConnectedInfo};
use rv_proto::{HeadscaleBootstrap, PairedController};

use crate::config::SavedDevice;

#[derive(Debug, Clone, Serialize)]
pub struct AgentIdentityView {
    pub device_code: String,
    pub password: String,
    pub tailscale_ip: Option<String>,
    pub hostname: Option<String>,
    pub headscale_configured: bool,
}

impl From<&AgentIdentity> for AgentIdentityView {
    fn from(identity: &AgentIdentity) -> Self {
        Self {
            device_code: identity.device_code.clone(),
            password: identity.password.clone(),
            tailscale_ip: identity.tailscale_ip.clone(),
            hostname: identity
                .headscale
                .as_ref()
                .map(|bootstrap: &HeadscaleBootstrap| bootstrap.hostname.clone()),
            headscale_configured: identity.headscale.is_some(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSnapshot {
    pub running: bool,
    pub online: bool,
    pub clients: usize,
    pub message: String,
    pub identity: Option<AgentIdentityView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedDeviceView {
    pub device_code: String,
    pub device_name: String,
    pub tailscale_ip: Option<String>,
    pub platform: String,
    pub online: bool,
    pub last_connected: Option<i64>,
}

impl From<&SavedDevice> for SavedDeviceView {
    fn from(device: &SavedDevice) -> Self {
        Self {
            device_code: device.device_code.clone(),
            device_name: device.device_name.clone(),
            tailscale_ip: device.tailscale_ip.clone(),
            platform: device.platform.clone(),
            online: device.online,
            last_connected: device.last_connected,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapPayload {
    pub settings: crate::config::Settings,
    pub agent: AgentSnapshot,
    pub saved_devices: Vec<SavedDeviceView>,
    pub version: String,
    pub controller_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfoPayload {
    pub device_code: String,
    pub device_name: String,
    pub session_id: String,
    pub width: u32,
    pub height: u32,
    pub fps: u16,
    pub transport: String,
    pub tailscale_ip: String,
}

impl From<&ConnectedInfo> for SessionInfoPayload {
    fn from(info: &ConnectedInfo) -> Self {
        Self {
            device_code: info.device_code.clone(),
            device_name: info.device_name.clone(),
            session_id: info.session_id.clone(),
            width: info.width,
            height: info.height,
            fps: info.fps,
            transport: info.transport.clone(),
            tailscale_ip: info.tailscale_ip.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FramePayload {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub timestamp_ms: u64,
    pub data_b64: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStatePayload {
    pub state: String,
    pub reason: Option<String>,
    pub info: Option<SessionInfoPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatusPayload {
    pub online: bool,
    pub listening: bool,
    pub clients: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionNoticePayload {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingSessionPayload {
    pub state: String,
    pub controller: String,
    pub session_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairedListPayload {
    pub controllers: Vec<PairedController>,
}
