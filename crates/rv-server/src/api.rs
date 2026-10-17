//! Public pairing API (`/api/v1`).

use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use rv_core::{
    AttemptLimiter, LimitDecision, constant_time_eq, generate_agent_secret, generate_device_code,
    generate_password, generate_token, hash_password, hash_secret_hex, normalize_device_code,
    verify_password,
};
use rv_proto::{
    API_VERSION, ChangePasswordRequest, ChangePasswordResponse, ConnectRequest, ConnectResponse,
    EnrollControllerRequest, EnrollControllerResponse, HeadscaleBootstrap, HeartbeatRequest,
    HeartbeatResponse, PairRequest, PairResponse, RegisterDeviceRequest, RegisterDeviceResponse,
    RevokeTokenRequest, ServerInfo, VerifySessionRequest, VerifySessionResponse,
};

use crate::db::{DeviceRecord, TokenRecord, now_secs};
use crate::error::ApiFailure;
use crate::headscale::HeadscaleClient;
use crate::state::AppState;

const DEVICE_CODE_ATTEMPTS: usize = 8;
const STALE_AFTER_SECS: i64 = 90;

pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<RegisterDeviceRequest>,
) -> Result<Json<RegisterDeviceResponse>, ApiFailure> {
    let ip = client_ip(&state, &headers, addr);
    check_limit(&state.register_limiter, &format!("reg|{ip}"))?;

    let name = sanitize_text(&request.name, 64, false)
        .ok_or_else(|| ApiFailure::bad_request("设备名称不能为空"))?;
    let platform = sanitize_text(&request.platform, 32, true).unwrap_or_else(|| "unknown".into());
    let fingerprint = sanitize_text(&request.fingerprint, 128, true).unwrap_or_default();
    let version = sanitize_text(&request.version, 32, true).unwrap_or_default();
    tracing::info!(%ip, %name, %platform, %version, %fingerprint, "device registration");

    let password = generate_password().map_err(ApiFailure::internal)?;
    let agent_secret = generate_agent_secret().map_err(ApiFailure::internal)?;
    let password_hash = hash_password(&password).map_err(ApiFailure::internal)?;
    let agent_secret_hash = hash_secret_hex(&agent_secret);
    let now = now_secs();
    let device_id = format!("dev_{}", random_id());
    let listen_port = state
        .db
        .setting_or("agent_port", "21416")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(rv_proto::DEFAULT_PORT);

    let mut last_error = None;
    let mut inserted = None;
    for _ in 0..DEVICE_CODE_ATTEMPTS {
        let code =
            generate_device_code().map_err(|error| ApiFailure::internal(error.to_string()))?;
        let record = DeviceRecord {
            id: device_id.clone(),
            code: code.clone(),
            name: name.clone(),
            password_hash: password_hash.clone(),
            agent_secret_hash: agent_secret_hash.clone(),
            tailscale_ip: None,
            platform: platform.clone(),
            online: false,
            last_seen: None,
            created_at: now,
            listen_port,
        };
        match state.db.insert_device(&record) {
            Ok(()) => {
                inserted = Some(record);
                break;
            }
            Err(error) if error.is_unique_violation() => continue,
            Err(error) => {
                last_error = Some(error);
                break;
            }
        }
    }
    let Some(device) = inserted else {
        // Count the attempt even when registration failed.
        record_failure(&state.register_limiter, &format!("reg|{ip}"));
        return Err(match last_error {
            Some(error) => ApiFailure::internal(error.to_string()),
            None => ApiFailure::internal("could not allocate a unique device code"),
        });
    };
    record_failure(&state.register_limiter, &format!("reg|{ip}"));

    let hostname = hostname_for(&device.code);
    let headscale = bootstrap_headscale(&state, &hostname).await;
    if headscale.is_none() {
        tracing::warn!(
            code = %device.code,
            "Headscale is not configured or pre-auth key creation failed; \
             the endpoint was registered but cannot join the tailnet yet"
        );
    }

    Ok(Json(RegisterDeviceResponse {
        device_id: device.id,
        device_code: device.code,
        password,
        agent_secret,
        protocol_version: rv_core::PROTOCOL_VERSION,
        headscale,
    }))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Json(request): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ApiFailure> {
    let device = authenticate_device(&state, &request.device_code, &request.agent_secret)?;

    let reported_ip = match request
        .tailscale_ip
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None => None,
        Some(value) => match value.parse::<IpAddr>() {
            Ok(ip) => Some(ip.to_string()),
            Err(_) => return Err(ApiFailure::bad_request("tailscale_ip 不是合法 IP 地址")),
        },
    };
    if let Some(ip) = &reported_ip
        && !ip
            .parse::<std::net::Ipv4Addr>()
            .map(rv_net::is_tailnet_ip)
            .unwrap_or(false)
    {
        tracing::warn!(code = %device.code, %ip, "endpoint reported a non-tailnet address");
    }

    let now = now_secs();
    state.db.update_heartbeat(
        &device.code,
        reported_ip.as_deref(),
        request.listen_port,
        now,
    )?;
    let _ = state.db.mark_stale_offline(now - STALE_AFTER_SECS);

    // If the endpoint cannot report its own address yet, ask Headscale.
    if reported_ip.is_none()
        && device.tailscale_ip.is_none()
        && let Ok(Some(client)) = HeadscaleClient::from_db(&state.db)
        && let Ok(Ok(Some(ip))) =
            tokio::time::timeout(Duration::from_secs(4), client.resolve_node_ip(&device.code)).await
    {
        let _ = state
            .db
            .update_heartbeat(&device.code, Some(&ip), request.listen_port, now);
    }

    let paired = state.db.active_controllers(&device.code, now)?;
    Ok(Json(HeartbeatResponse {
        online: true,
        server_time: now,
        paired_controllers: paired,
    }))
}

pub async fn pair(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<PairRequest>,
) -> Result<Json<PairResponse>, ApiFailure> {
    let ip = client_ip(&state, &headers, addr);
    let normalized = normalize_device_code(&request.device_code);
    let attempted_code = request
        .device_code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_ascii_uppercase();
    let controller_name =
        sanitize_text(&request.controller_name, 64, true).unwrap_or_else(|| "controller".into());
    let platform = sanitize_text(&request.platform, 32, true).unwrap_or_else(|| "unknown".into());
    let now = now_secs();

    let ip_key = format!("pair_ip|{ip}");
    check_limit(&state.ip_limiter, &ip_key)?;

    let Ok(code) = normalized else {
        record_failure(&state.ip_limiter, &ip_key);
        let _ = state.db.record_pairing(
            &attempted_code,
            Some(&request.controller_id),
            &controller_name,
            &ip,
            false,
            Some("invalid_code"),
            now,
        );
        return Err(invalid_credentials());
    };

    let code_key = format!("pair_code|{code}");
    let compound_key = format!("pair|{code}|{ip}");
    check_limit(&state.code_limiter, &code_key)?;
    check_limit(&state.ip_limiter, &compound_key)?;

    let Some(device) = state.db.device_by_code(&code)? else {
        // Burn comparable CPU on unknown codes to avoid a timing oracle.
        let _ = verify_password(&request.password, &state.dummy_hash);
        record_failure(&state.ip_limiter, &ip_key);
        record_failure(&state.ip_limiter, &compound_key);
        record_failure(&state.code_limiter, &code_key);
        let _ = state.db.record_pairing(
            &code,
            Some(&request.controller_id),
            &controller_name,
            &ip,
            false,
            Some("unknown_device"),
            now,
        );
        return Err(invalid_credentials());
    };

    if !verify_password(&request.password, &device.password_hash) {
        record_failure(&state.ip_limiter, &ip_key);
        record_failure(&state.ip_limiter, &compound_key);
        record_failure(&state.code_limiter, &code_key);
        let _ = state.db.record_pairing(
            &code,
            Some(&request.controller_id),
            &controller_name,
            &ip,
            false,
            Some("bad_password"),
            now,
        );
        return Err(invalid_credentials());
    }

    // Successful pairing clears the per-code failure streak.
    record_success(&state.code_limiter, &code_key);
    record_success(&state.ip_limiter, &compound_key);

    let token = generate_token().map_err(ApiFailure::internal)?;
    let token_hash = hash_secret_hex(&token);
    let token_id = format!("tok_{}", random_id());
    let ttl_days = state
        .db
        .setting_or("token_ttl_days", "30")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(30)
        .clamp(1, 3650);

    state.db.insert_token(&TokenRecord {
        id: token_id.clone(),
        device_code: code.clone(),
        controller_id: sanitize_text(&request.controller_id, 64, true)
            .unwrap_or_else(|| "controller".into()),
        controller_name: controller_name.clone(),
        platform: platform.clone(),
        token_hash,
        created_at: now,
        expires_at: now + ttl_days * 86_400,
        revoked_at: None,
        last_used: None,
    })?;

    let _ = state.db.record_pairing(
        &code,
        Some(&request.controller_id),
        &controller_name,
        &ip,
        true,
        None,
        now,
    );

    tracing::info!(%code, %controller_name, %ip, "pairing accepted");
    Ok(Json(PairResponse {
        token,
        token_id,
        device_code: code,
        device_name: device.name,
        tailscale_ip: device.tailscale_ip,
        online: device.online,
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
    }))
}

pub async fn connect(
    State(state): State<AppState>,
    Json(request): Json<ConnectRequest>,
) -> Result<Json<ConnectResponse>, ApiFailure> {
    let code = normalize_device_code(&request.device_code)
        .map_err(|_| ApiFailure::unauthorized("设备码或令牌无效"))?;
    let token_hash = hash_secret_hex(&request.token);
    let now = now_secs();
    let Some(token) = state.db.valid_token(&code, &token_hash, now)? else {
        return Err(ApiFailure::unauthorized("配对令牌无效、已吊销或已过期"));
    };
    if token.controller_id != request.controller_id {
        return Err(ApiFailure::unauthorized("配对令牌不属于该控制端"));
    }
    let Some(device) = state.db.device_by_code(&code)? else {
        return Err(ApiFailure::unauthorized("设备不存在"));
    };
    let Some(ip) = device.tailscale_ip.clone() else {
        return Err(ApiFailure::conflict(
            "device_offline",
            "被控端尚未加入虚拟网络，请确认其网络层已启动",
        ));
    };
    if !device.online {
        return Err(ApiFailure::conflict(
            "device_offline",
            "被控端离线，请确认设备已开机并运行 RustVNC",
        ));
    }

    state.db.touch_token(&token.id, now)?;
    Ok(Json(ConnectResponse {
        device_code: code,
        device_name: device.name,
        tailscale_ip: ip,
        port: device.listen_port,
        online: true,
    }))
}

pub async fn verify_session(
    State(state): State<AppState>,
    Json(request): Json<VerifySessionRequest>,
) -> Result<Json<VerifySessionResponse>, ApiFailure> {
    let device = authenticate_device(&state, &request.device_code, &request.agent_secret)?;
    let now = now_secs();
    let token_hash = hash_secret_hex(&request.token);
    let token = state
        .db
        .valid_token(&device.code, &token_hash, now)?
        .filter(|token| token.controller_id == request.controller_id);

    match token {
        Some(token) => {
            state.db.touch_token(&token.id, now)?;
            Ok(Json(VerifySessionResponse {
                valid: true,
                token_id: Some(token.id),
                controller_name: Some(token.controller_name),
                expires_at: Some(token.expires_at),
            }))
        }
        None => Ok(Json(VerifySessionResponse {
            valid: false,
            token_id: None,
            controller_name: None,
            expires_at: None,
        })),
    }
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Json(request): Json<RevokeTokenRequest>,
) -> Result<Json<rv_proto::OkResponse>, ApiFailure> {
    let device = authenticate_device(&state, &request.device_code, &request.agent_secret)?;
    let revoked = state
        .db
        .revoke_token(&request.token_id, &device.code, now_secs())?;
    if !revoked {
        return Err(ApiFailure::not_found("令牌不存在或已被吊销"));
    }
    Ok(Json(rv_proto::OkResponse { ok: true }))
}

pub async fn change_password(
    State(state): State<AppState>,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, ApiFailure> {
    let device = authenticate_device(&state, &request.device_code, &request.agent_secret)?;
    let password = match request.new_password {
        Some(value) => {
            let value = value.trim().to_owned();
            if !(8..=64).contains(&value.chars().count()) || value.chars().any(|c| c.is_control()) {
                return Err(ApiFailure::bad_request("密码长度必须为 8–64 个字符"));
            }
            value
        }
        None => generate_password().map_err(ApiFailure::internal)?,
    };
    let hash = hash_password(&password).map_err(ApiFailure::internal)?;
    state.db.update_password(&device.code, &hash)?;
    tracing::info!(code = %device.code, "pairing password rotated");
    Ok(Json(ChangePasswordResponse { password }))
}

pub async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    let headscale_configured = HeadscaleClient::from_db(&state.db).ok().flatten().is_some();
    Json(ServerInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        protocol_version: rv_core::PROTOCOL_VERSION,
        headscale_configured,
    })
}

/// Mints a single-use pre-auth key for a controller device. The key is shown
/// exactly once and is never persisted by the backend.
pub async fn enroll_controller(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<EnrollControllerRequest>,
) -> Result<Json<EnrollControllerResponse>, ApiFailure> {
    let ip = client_ip(&state, &headers, addr);
    let limiter_key = format!("enroll|{ip}");
    check_limit(&state.register_limiter, &limiter_key)?;
    record_failure(&state.register_limiter, &limiter_key);

    let Some(client) = HeadscaleClient::from_db(&state.db)
        .map_err(|error| ApiFailure::internal(error.to_string()))?
    else {
        return Err(ApiFailure::bad_request(
            "Headscale 尚未配置，无法为控制端签发网络密钥",
        ));
    };
    let name = sanitize_text(&request.name, 48, true).unwrap_or_else(|| "controller".into());
    let platform = sanitize_text(&request.platform, 32, true).unwrap_or_else(|| "unknown".into());
    let suffix = random_id()[..10].to_owned();
    // DNS-safe, unique, and easy to find in the Headscale node list.
    let hostname = format!("rvnc-ctrl-{suffix}");
    let user = state
        .db
        .setting_or("headscale_user", "1")
        .unwrap_or_else(|_| "1".into());
    let preauth_key = tokio::time::timeout(
        Duration::from_secs(8),
        client.create_preauth_key(&user, &hostname),
    )
    .await
    .map_err(|_| ApiFailure::bad_gateway("Headscale 签发密钥超时"))?
    .map_err(|error| ApiFailure::bad_gateway(error.to_string()))?;

    let configured_control = state
        .db
        .setting_or("headscale_control_url", "")
        .unwrap_or_default()
        .trim()
        .to_owned();
    let control_url = if configured_control.is_empty() {
        state
            .db
            .get_setting("headscale_url")
            .unwrap_or_default()
            .unwrap_or_default()
    } else {
        configured_control
    };
    let derp_url = state
        .db
        .get_setting("derp_url")
        .unwrap_or_default()
        .filter(|value| !value.trim().is_empty());

    tracing::info!(%ip, %platform, %name, %hostname, "controller pre-auth key issued");
    Ok(Json(EnrollControllerResponse {
        preauth_key,
        control_url,
        hostname,
        derp_url,
    }))
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "api": API_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn authenticate_device(
    state: &AppState,
    code: &str,
    agent_secret: &str,
) -> Result<DeviceRecord, ApiFailure> {
    let code = normalize_device_code(code).map_err(|_| ApiFailure::unauthorized("设备认证失败"))?;
    let device = state
        .db
        .device_by_code(&code)?
        .ok_or_else(|| ApiFailure::unauthorized("设备认证失败"))?;
    let digest = hash_secret_hex(agent_secret);
    if !constant_time_eq(&digest, &device.agent_secret_hash) {
        return Err(ApiFailure::unauthorized("设备认证失败"));
    }
    Ok(device)
}

async fn bootstrap_headscale(state: &AppState, hostname: &str) -> Option<HeadscaleBootstrap> {
    let client = HeadscaleClient::from_db(&state.db).ok().flatten()?;
    let user = state
        .db
        .setting_or("headscale_user", "1")
        .unwrap_or_else(|_| "1".into());
    let key = tokio::time::timeout(
        Duration::from_secs(8),
        client.create_preauth_key(&user, hostname),
    )
    .await
    .ok()?
    .ok()?;

    let control_url = state
        .db
        .setting_or("headscale_control_url", "")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| state.db.get_setting("headscale_url").ok().flatten())?;
    let derp_url = state
        .db
        .get_setting("derp_url")
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty());

    Some(HeadscaleBootstrap {
        control_url,
        preauth_key: key,
        hostname: hostname.to_owned(),
        derp_url,
    })
}

fn hostname_for(code: &str) -> String {
    format!("rvnc-{}", code.to_ascii_lowercase())
}

fn random_id() -> String {
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

fn sanitize_text(input: &str, max: usize, allow_empty: bool) -> Option<String> {
    let cleaned: String = input
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect();
    if cleaned.is_empty() && !allow_empty {
        None
    } else {
        Some(cleaned)
    }
}

fn invalid_credentials() -> ApiFailure {
    ApiFailure::unauthorized("设备码或密码不正确")
}

pub fn client_ip(state: &AppState, headers: &HeaderMap, addr: SocketAddr) -> String {
    if state.trust_proxy {
        if let Some(value) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            && let Some(first) = value.split(',').next()
            && !first.trim().is_empty()
        {
            return first.trim().to_owned();
        }
        if let Some(value) = headers
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok())
            && !value.trim().is_empty()
        {
            return value.trim().to_owned();
        }
    }
    addr.ip().to_string()
}

fn check_limit(limiter: &Mutex<AttemptLimiter>, key: &str) -> Result<(), ApiFailure> {
    let mut guard = limiter.lock().unwrap_or_else(|poison| poison.into_inner());
    match guard.check(key, Instant::now()) {
        LimitDecision::Allowed => Ok(()),
        LimitDecision::Blocked { retry_after } => {
            Err(ApiFailure::rate_limited(retry_after.as_secs().max(1)))
        }
    }
}

fn record_failure(limiter: &Mutex<AttemptLimiter>, key: &str) {
    let mut guard = limiter.lock().unwrap_or_else(|poison| poison.into_inner());
    guard.record_failure(key, Instant::now());
}

fn record_success(limiter: &Mutex<AttemptLimiter>, key: &str) {
    let mut guard = limiter.lock().unwrap_or_else(|poison| poison.into_inner());
    guard.record_success(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_controls_and_limits() {
        assert_eq!(
            sanitize_text("  hello\u{0}world  ", 64, false).unwrap(),
            "helloworld"
        );
        assert_eq!(sanitize_text("abcdef", 3, false).unwrap(), "abc");
        assert!(sanitize_text("   ", 10, false).is_none());
        assert_eq!(sanitize_text("", 10, true).unwrap(), "");
    }

    #[test]
    fn hostname_is_deterministic_and_lowercase() {
        assert_eq!(hostname_for("A2B3C4"), "rvnc-a2b3c4");
    }
}
