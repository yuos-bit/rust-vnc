//! Local configuration and DPAPI-protected secrets.

use std::path::Path;

use rv_engine::AgentIdentity;
use serde::{Deserialize, Serialize};

use crate::secure_store;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub server_url: String,
    pub device_name: String,
    pub controller_name: String,
    pub listen_bind: String,
    pub listen_port: u16,
    pub fps: u16,
    pub quality: u8,
    /// 0 keeps the native desktop resolution.
    pub max_width: u32,
    /// `direct` or `socks5://127.0.0.1:1055` (tsnet sidecar).
    pub network_mode: String,
    /// Address reported to the pairing server. Empty lets Headscale resolve it.
    pub tailscale_ip: String,
    pub controller_id: String,
    pub theme: String,
    pub start_minimized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let host = hostname();
        Self {
            server_url: "http://127.0.0.1:8080".into(),
            device_name: host.clone(),
            controller_name: host,
            listen_bind: "127.0.0.1".into(),
            listen_port: rv_proto::DEFAULT_PORT,
            fps: 15,
            quality: 70,
            max_width: 0,
            network_mode: "direct".into(),
            tailscale_ip: String::new(),
            controller_id: random_id(),
            theme: "dark".into(),
            start_minimized: false,
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) -> Result<(), String> {
        self.server_url = self.server_url.trim().trim_end_matches('/').to_owned();
        if !(self.server_url.starts_with("http://") || self.server_url.starts_with("https://")) {
            return Err("配对服务器地址必须以 http:// 或 https:// 开头".into());
        }
        self.device_name = clean_name(&self.device_name, &hostname());
        self.controller_name = clean_name(&self.controller_name, &hostname());

        self.listen_bind = self.listen_bind.trim().to_owned();
        if self.listen_bind.parse::<std::net::IpAddr>().is_err() {
            return Err("监听地址必须是合法 IP（推荐 127.0.0.1）".into());
        }
        if self.listen_port == 0 {
            return Err("监听端口不能为 0".into());
        }
        if !(1..=60).contains(&self.fps) {
            return Err("帧率应在 1–60 之间".into());
        }
        if !(20..=95).contains(&self.quality) {
            return Err("画质应在 20–95 之间".into());
        }
        if self.max_width != 0 && !(320..=7680).contains(&self.max_width) {
            return Err("最大宽度应为 0（原分辨率）或 320–7680".into());
        }

        self.network_mode = self.network_mode.trim().to_owned();
        if self.network_mode.is_empty() {
            self.network_mode = "direct".into();
        }
        if self.network_mode != "direct" && !self.network_mode.starts_with("socks5://") {
            return Err("网络模式必须是 direct 或 socks5://主机:端口".into());
        }

        self.tailscale_ip = self.tailscale_ip.trim().to_owned();
        if !self.tailscale_ip.is_empty() && self.tailscale_ip.parse::<std::net::IpAddr>().is_err() {
            return Err("Tailscale IP 格式不正确".into());
        }
        if self.controller_id.trim().is_empty() {
            self.controller_id = random_id();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedDevice {
    pub device_code: String,
    pub device_name: String,
    pub server_url: String,
    pub token: String,
    pub token_id: String,
    pub tailscale_ip: Option<String>,
    pub platform: String,
    pub online: bool,
    pub last_connected: Option<i64>,
}

pub fn load_settings(dir: &Path) -> Settings {
    let mut settings: Settings = std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if settings.controller_id.trim().is_empty() {
        settings.controller_id = random_id();
    }
    settings
}

pub fn save_settings(dir: &Path, settings: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    atomic_write(&dir.join("settings.json"), &bytes)
}

pub fn load_identity(dir: &Path) -> Option<AgentIdentity> {
    let bytes = std::fs::read(dir.join("agent-identity.bin")).ok()?;
    let plain = secure_store::unprotect(&bytes).ok()?;
    serde_json::from_slice(&plain).ok()
}

pub fn save_identity(dir: &Path, identity: &AgentIdentity) -> Result<(), String> {
    let plain = serde_json::to_vec(identity).map_err(|e| e.to_string())?;
    let protected = secure_store::protect(&plain)?;
    atomic_write(&dir.join("agent-identity.bin"), &protected)
}

pub fn clear_identity(dir: &Path) {
    let _ = std::fs::remove_file(dir.join("agent-identity.bin"));
}

pub fn load_devices(dir: &Path) -> Vec<SavedDevice> {
    let path = dir.join("devices.bin");
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Ok(plain) = secure_store::unprotect(&bytes) else {
        tracing::warn!("could not decrypt saved devices");
        return Vec::new();
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn save_devices(dir: &Path, devices: &[SavedDevice]) -> Result<(), String> {
    let plain = serde_json::to_vec(devices).map_err(|e| e.to_string())?;
    let protected = secure_store::protect(&plain)?;
    atomic_write(&dir.join("devices.bin"), &protected)
}

pub fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|e| format!("写入失败: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("保存失败: {e}"))
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "My PC".into())
}

fn clean_name(value: &str, fallback: &str) -> String {
    let cleaned: String = value
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

pub fn random_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        return format!("{now:032x}");
    }
    let mut out = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
