mod admin;
mod api;
mod db;
mod error;
mod headscale;
mod state;
mod web;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, post};
use tracing_subscriber::EnvFilter;

use crate::db::Db;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rv_server=debug")),
        )
        .with_target(false)
        .compact()
        .init();

    let bind: SocketAddr = std::env::var("RV_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    let db_path = std::env::var("RV_DB").unwrap_or_else(|_| "data/rvnc.db".into());
    let trust_proxy = matches!(
        std::env::var("RV_TRUST_PROXY").unwrap_or_default().as_str(),
        "1" | "true" | "TRUE" | "yes"
    );

    let (admin_token, generated) = match std::env::var("RV_ADMIN_TOKEN")
        .or_else(|_| std::env::var("ADMIN_TOKEN"))
        .map(|value| value.trim().to_owned())
        .ok()
        .filter(|value| !value.is_empty())
    {
        Some(token) => (token, false),
        None => (generate_admin_token(), true),
    };

    let db = Db::open(&db_path)?;
    seed_settings(&db);

    let state = AppState::new(db, admin_token.clone(), trust_proxy);

    let protected = Router::new()
        .route("/api/admin/stats", get(admin::stats))
        .route("/api/admin/devices", get(admin::devices))
        .route("/api/admin/devices/{code}", delete(admin::delete_device))
        .route("/api/admin/tokens", get(admin::tokens))
        .route("/api/admin/tokens/{id}", delete(admin::revoke_token))
        .route("/api/admin/pairings", get(admin::pairings))
        .route(
            "/api/admin/settings",
            get(admin::get_settings).put(admin::update_settings),
        )
        .route("/api/admin/headscale/nodes", get(admin::headscale_nodes))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            admin::require_admin,
        ));

    let app = Router::new()
        .route("/healthz", get(api::health))
        .route("/api/v1/info", get(api::info))
        .route("/api/v1/devices/register", post(api::register))
        .route("/api/v1/controllers/enroll", post(api::enroll_controller))
        .route("/api/v1/devices/heartbeat", post(api::heartbeat))
        .route("/api/v1/pair", post(api::pair))
        .route("/api/v1/connect", post(api::connect))
        .route("/api/v1/session/verify", post(api::verify_session))
        .route("/api/v1/agent/tokens/revoke", post(api::revoke_token))
        .route("/api/v1/agent/password", post(api::change_password))
        .route("/api/admin/login", post(admin::login))
        .merge(protected)
        .merge(web::routes())
        .with_state(state.clone());

    spawn_maintenance(state.clone());

    tracing::info!(%bind, %db_path, %trust_proxy, "RustVNC pairing server starting");
    let shown = if bind.ip().is_unspecified() {
        format!("127.0.0.1:{}", bind.port())
    } else {
        bind.to_string()
    };
    println!();
    println!("  RustVNC pairing backend");
    println!("  Admin console : http://{shown}/admin/");
    println!("  Health        : http://{shown}/healthz");
    if generated {
        println!();
        println!("  RV_ADMIN_TOKEN was not set. Generated administrator token:");
        println!("  {admin_token}");
        println!("  Store it now; it is not written to disk.");
    } else if admin_token.len() < 16 {
        tracing::warn!("RV_ADMIN_TOKEN is shorter than 16 characters");
    }
    println!();

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn seed_settings(db: &Db) {
    for (key, env_name) in [
        ("headscale_url", "HEADSCALE_URL"),
        ("headscale_api_key", "HEADSCALE_API_KEY"),
        ("headscale_user", "HEADSCALE_USER"),
        ("headscale_control_url", "HEADSCALE_CONTROL_URL"),
        ("derp_url", "DERP_URL"),
        ("public_url", "PUBLIC_URL"),
        ("agent_port", "AGENT_PORT"),
        ("token_ttl_days", "TOKEN_TTL_DAYS"),
    ] {
        if let Ok(value) = std::env::var(env_name) {
            let value = value.trim();
            if !value.is_empty() {
                let _ = db.set_setting_if_missing(key, value);
            }
        }
    }
}

fn spawn_maintenance(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let now = db::now_secs();
            let _ = state.db.mark_stale_offline(now - 90);
            let deadline = std::time::Instant::now();
            for limiter in [
                &state.code_limiter,
                &state.ip_limiter,
                &state.register_limiter,
            ] {
                let mut guard = limiter.lock().unwrap_or_else(|poison| poison.into_inner());
                guard.cleanup(deadline);
            }
        }
    });
}

fn generate_admin_token() -> String {
    let mut bytes = [0u8; 24];
    if getrandom::fill(&mut bytes).is_err() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        return format!("admin-{now:x}");
    }
    let mut out = String::from("admin_");
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
