use axum::Json;
use axum::http::{HeaderValue, StatusCode, header::RETRY_AFTER};
use axum::response::{IntoResponse, Response};
use rv_proto::ApiError;

#[derive(Debug)]
pub struct ApiFailure {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retry_after: Option<u64>,
}

impl ApiFailure {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn rate_limited(retry_after: u64) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limited",
            message: "尝试过于频繁，请稍后再试".into(),
            retry_after: Some(retry_after.max(1)),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        let message = message.into();
        tracing::error!(%message, "internal server error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "服务器内部错误".into(),
            retry_after: None,
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "upstream_error",
            message: message.into(),
            retry_after: None,
        }
    }
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        let mut response =
            (self.status, Json(ApiError::new(self.code, self.message))).into_response();
        if let Some(seconds) = self.retry_after
            && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
        {
            response.headers_mut().insert(RETRY_AFTER, value);
        }
        response
    }
}

impl From<crate::db::DbError> for ApiFailure {
    fn from(value: crate::db::DbError) -> Self {
        Self::internal(value.to_string())
    }
}
