use axum::Router;
use axum::http::header;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;

use crate::state::AppState;

const INDEX: &str = include_str!("../web/index.html");
const STYLES: &str = include_str!("../web/styles.css");
const APP_JS: &str = include_str!("../web/app.js");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/admin", get(index))
        .route("/admin/", get(index))
        .route("/admin/styles.css", get(styles))
        .route("/admin/app.js", get(app_js))
        .route("/admin/favicon.svg", get(favicon))
}

async fn root() -> Redirect {
    Redirect::permanent("/admin/")
}

async fn index() -> Html<&'static str> {
    Html(INDEX)
}

async fn styles() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLES)
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        APP_JS,
    )
}

async fn favicon() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<rect width="64" height="64" rx="16" fill="#16213e"/>
<rect x="10" y="14" width="44" height="30" rx="5" fill="none" stroke="#8ec5fc" stroke-width="3"/>
<path d="M22 54h20M32 44v10" stroke="#8ec5fc" stroke-width="3" stroke-linecap="round"/>
<circle cx="32" cy="29" r="6" fill="#e0c3fc"/>
</svg>"##,
    )
}
