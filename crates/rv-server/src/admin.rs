//! Administration API (`/api/admin`). Every route except `/login` requires
//! `Authorization: Bearer <RV_ADMIN_TOKEN>`.

use axum::Json;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, header::AUTHORIZATION};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rv_core::constant_time_eq;
use rv_proto::{
    AdminLoginRequest, DeviceView, HeadscaleNodeView, OkResponse, PairingView, SettingsUpdate,
    SettingsView, StatsView, TokenView,
};
use serde::Deserialize;

use crate::db::{Db, DbError, now_secs};
use crate::error::ApiFailure;
use crate::headscale::HeadscaleClient;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PairingsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<AdminLoginRequest>,
) -> Result<Json<OkResponse>, ApiFailure> {
    let provided = request.token.trim();
    if provided.is_empty() || !constant_time_eq(provided, &state.admin_token) {
        return Err(ApiFailure::unauthorized("管理员令牌无效"));
    }
    Ok(Json(OkResponse { ok: true }))
}

pub async fn require_admin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    match bearer_token(request.headers()) {
        Some(token) if constant_time_eq(token, &state.admin_token) => next.run(request).await,
        _ => ApiFailure::unauthorized("需要管理员令牌").into_response(),
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() { None } else { Some(token) }
}

pub async fn stats(State(state): State<AppState>) -> Result<Json<StatsView>, ApiFailure> {
    let now = now_secs();
    let _ = state.db.mark_stale_offline(now - 90);
    Ok(Json(state.db.stats(now, day_start(now))?))
}

pub async fn devices(State(state): State<AppState>) -> Result<Json<Vec<DeviceView>>, ApiFailure> {
    let now = now_secs();
    let _ = state.db.mark_stale_offline(now - 90);
    Ok(Json(state.db.all_devices(now)?))
}

pub async fn delete_device(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<OkResponse>, ApiFailure> {
    let code = rv_core::normalize_device_code(&code)
        .map_err(|_| ApiFailure::bad_request("设备码格式不正确"))?;
    if state.db.delete_device(&code)? {
        tracing::info!(%code, "device deleted by administrator");
        Ok(Json(OkResponse { ok: true }))
    } else {
        Err(ApiFailure::not_found("设备不存在"))
    }
}

pub async fn tokens(State(state): State<AppState>) -> Result<Json<Vec<TokenView>>, ApiFailure> {
    Ok(Json(state.db.all_tokens()?))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, ApiFailure> {
    if state.db.revoke_token_admin(&id, now_secs())? {
        tracing::info!(%id, "token revoked by administrator");
        Ok(Json(OkResponse { ok: true }))
    } else {
        Err(ApiFailure::not_found("令牌不存在或已被吊销"))
    }
}

pub async fn pairings(
    State(state): State<AppState>,
    Query(query): Query<PairingsQuery>,
) -> Result<Json<Vec<PairingView>>, ApiFailure> {
    Ok(Json(state.db.recent_pairings(query.limit)?))
}

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<SettingsView>, ApiFailure> {
    Ok(Json(load_settings(&state.db)?))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(update): Json<SettingsUpdate>,
) -> Result<Json<SettingsView>, ApiFailure> {
    if let Some(value) = update.headscale_url {
        let value = value.trim().trim_end_matches('/').to_owned();
        if !value.is_empty() && !(value.starts_with("http://") || value.starts_with("https://")) {
            return Err(ApiFailure::bad_request(
                "Headscale 地址必须以 http:// 或 https:// 开头",
            ));
        }
        state.db.set_setting("headscale_url", &value)?;
    }
    if let Some(value) = update.headscale_api_key {
        // An empty string clears the stored key.
        state.db.set_setting("headscale_api_key", value.trim())?;
    }
    if let Some(value) = update.headscale_user {
        state.db.set_setting("headscale_user", value.trim())?;
    }
    if let Some(value) = update.derp_url {
        state.db.set_setting("derp_url", value.trim())?;
    }
    if let Some(value) = update.public_url {
        state
            .db
            .set_setting("public_url", value.trim().trim_end_matches('/'))?;
    }
    if let Some(value) = update.agent_port {
        if value == 0 {
            return Err(ApiFailure::bad_request("端口范围应为 1–65535"));
        }
        state.db.set_setting("agent_port", &value.to_string())?;
    }
    if let Some(value) = update.token_ttl_days {
        if !(1..=3650).contains(&value) {
            return Err(ApiFailure::bad_request("令牌有效期应为 1–3650 天"));
        }
        state.db.set_setting("token_ttl_days", &value.to_string())?;
    }
    state
        .db
        .set_setting("settings_updated_at", &now_secs().to_string())?;
    tracing::info!("administrator updated server settings");
    Ok(Json(load_settings(&state.db)?))
}

pub async fn headscale_nodes(
    State(state): State<AppState>,
) -> Result<Json<Vec<HeadscaleNodeView>>, ApiFailure> {
    let Some(client) = HeadscaleClient::from_db(&state.db)
        .map_err(|error| ApiFailure::internal(error.to_string()))?
    else {
        return Err(ApiFailure::bad_request("Headscale 尚未配置"));
    };
    let nodes = client
        .list_nodes()
        .await
        .map_err(|error| ApiFailure::bad_gateway(error.to_string()))?;
    Ok(Json(nodes))
}

fn load_settings(db: &Db) -> Result<SettingsView, DbError> {
    let api_key_set = db
        .get_setting("headscale_api_key")?
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    Ok(SettingsView {
        headscale_url: db.setting_or("headscale_url", "")?,
        headscale_user: db.setting_or("headscale_user", "1")?,
        headscale_api_key_set: api_key_set,
        derp_url: db.setting_or("derp_url", "")?,
        public_url: db.setting_or("public_url", "")?,
        agent_port: db
            .setting_or("agent_port", "21416")?
            .parse()
            .unwrap_or(21416),
        token_ttl_days: db.setting_or("token_ttl_days", "30")?.parse().unwrap_or(30),
        updated_at: db
            .get_setting("settings_updated_at")?
            .and_then(|value| value.parse().ok()),
    })
}

fn day_start(now: i64) -> i64 {
    now - now.rem_euclid(86_400)
}
