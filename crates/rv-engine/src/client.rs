//! Typed HTTP client for the pairing backend.

use std::time::Duration;

use reqwest::Response;
use rv_proto::{
    ApiError, ChangePasswordRequest, ChangePasswordResponse, ConnectRequest, ConnectResponse,
    EnrollControllerRequest, EnrollControllerResponse, HeartbeatRequest, HeartbeatResponse,
    PairRequest, PairResponse, RegisterDeviceRequest, RegisterDeviceResponse, RevokeTokenRequest,
    VerifySessionRequest, VerifySessionResponse,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::EngineError;

#[derive(Clone)]
pub struct ServerClient {
    base: String,
    http: reqwest::Client,
}

impl ServerClient {
    pub fn new(base_url: &str) -> Result<Self, EngineError> {
        let base = base_url.trim().trim_end_matches('/').to_owned();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(EngineError::Config(format!(
                "server URL must start with http:// or https://: {base}"
            )));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .user_agent(concat!("RustVNC/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| EngineError::Http(e.to_string()))?;
        Ok(Self { base, http })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    async fn post<B, R>(&self, path: &str, body: &B) -> Result<R, EngineError>
    where
        B: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| EngineError::Http(format!("{url}: {e}")))?;
        parse_response(response).await
    }

    pub async fn register(
        &self,
        request: &RegisterDeviceRequest,
    ) -> Result<RegisterDeviceResponse, EngineError> {
        self.post("/api/v1/devices/register", request).await
    }

    pub async fn heartbeat(
        &self,
        request: &HeartbeatRequest,
    ) -> Result<HeartbeatResponse, EngineError> {
        self.post("/api/v1/devices/heartbeat", request).await
    }

    pub async fn enroll_controller(
        &self,
        request: &EnrollControllerRequest,
    ) -> Result<EnrollControllerResponse, EngineError> {
        self.post("/api/v1/controllers/enroll", request).await
    }

    pub async fn pair(&self, request: &PairRequest) -> Result<PairResponse, EngineError> {
        self.post("/api/v1/pair", request).await
    }

    pub async fn connect(&self, request: &ConnectRequest) -> Result<ConnectResponse, EngineError> {
        self.post("/api/v1/connect", request).await
    }

    pub async fn verify_session(
        &self,
        request: &VerifySessionRequest,
    ) -> Result<VerifySessionResponse, EngineError> {
        self.post("/api/v1/session/verify", request).await
    }

    pub async fn revoke_token(&self, request: &RevokeTokenRequest) -> Result<(), EngineError> {
        let _: serde_json::Value = self.post("/api/v1/agent/tokens/revoke", request).await?;
        Ok(())
    }

    pub async fn change_password(
        &self,
        request: &ChangePasswordRequest,
    ) -> Result<ChangePasswordResponse, EngineError> {
        self.post("/api/v1/agent/password", request).await
    }
}

async fn parse_response<R: DeserializeOwned>(response: Response) -> Result<R, EngineError> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| EngineError::Http(format!("reading response failed: {e}")))?;

    if status.is_success() {
        serde_json::from_slice(&bytes)
            .map_err(|e| EngineError::Protocol(format!("invalid server JSON: {e}")))
    } else {
        match serde_json::from_slice::<ApiError>(&bytes) {
            Ok(api) => Err(api.into()),
            Err(_) => Err(EngineError::Http(format!(
                "server returned HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(200)
                    .collect::<String>()
            ))),
        }
    }
}
