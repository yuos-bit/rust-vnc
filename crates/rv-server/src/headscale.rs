//! Minimal Headscale REST client.
//!
//! Only two operations are needed by the pairing backend:
//!   * mint a single-use pre-auth key when a controlled endpoint registers;
//!   * resolve a node's 100.x address by its deterministic hostname.
//!
//! The API key never leaves the server and is never returned by any endpoint.

use rv_proto::HeadscaleNodeView;
use serde_json::json;

use crate::db::Db;

#[derive(Clone)]
pub struct HeadscaleClient {
    base: String,
    api_key: String,
    http: reqwest::Client,
}

impl HeadscaleClient {
    pub fn from_db(db: &Db) -> Result<Option<Self>, HeadscaleError> {
        let base = db
            .get_setting("headscale_url")?
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_owned();
        let api_key = db
            .get_setting("headscale_api_key")?
            .unwrap_or_default()
            .trim()
            .to_owned();
        if base.is_empty() || api_key.is_empty() {
            return Ok(None);
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent(concat!("RustVNC-Server/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| HeadscaleError::Transport(e.to_string()))?;
        Ok(Some(Self {
            base,
            api_key,
            http,
        }))
    }

    pub async fn create_preauth_key(
        &self,
        user: &str,
        _hostname: &str,
    ) -> Result<String, HeadscaleError> {
        let url = format!("{}/api/v1/preauthkey", self.base);
        // The controller endpoint is a one-shot key; an explicit one-hour
        // expiry bounds the window if it leaks before first use.
        let expiration = (time::OffsetDateTime::now_utc() + time::Duration::hours(1))
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|error| HeadscaleError::Api(format!("expiration format: {error}")))?;
        // Headscale expects a uint64 user id; protojson also accepts numeric
        // strings, but sending a number avoids version-specific surprises.
        let user_value = user
            .trim()
            .parse::<u64>()
            .map(serde_json::Value::from)
            .unwrap_or_else(|_| serde_json::Value::from(user));
        let body = json!({
            "user": user_value,
            "reusable": false,
            "ephemeral": false,
            "expiration": expiration,
            "aclTags": [],
        });
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| HeadscaleError::Transport(e.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| HeadscaleError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(HeadscaleError::Api(format!(
                "HTTP {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }

        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| HeadscaleError::Api(format!("invalid JSON: {e}")))?;
        value
            .pointer("/preAuthKey/key")
            .or_else(|| value.get("key"))
            .and_then(|key| key.as_str())
            .map(str::to_owned)
            .ok_or_else(|| HeadscaleError::Api("response did not contain a pre-auth key".into()))
    }

    pub async fn list_nodes(&self) -> Result<Vec<HeadscaleNodeView>, HeadscaleError> {
        let url = format!("{}/api/v1/node", self.base);
        let response = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| HeadscaleError::Transport(e.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| HeadscaleError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(HeadscaleError::Api(format!(
                "HTTP {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }

        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| HeadscaleError::Api(format!("invalid JSON: {e}")))?;
        let nodes = value
            .get("nodes")
            .and_then(|nodes| nodes.as_array())
            .ok_or_else(|| HeadscaleError::Api("response did not contain nodes".into()))?;

        Ok(nodes
            .iter()
            .map(|node| HeadscaleNodeView {
                id: node
                    .get("id")
                    .map(|id| match id {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default(),
                name: node
                    .get("givenName")
                    .and_then(|v| v.as_str())
                    .or_else(|| node.get("name").and_then(|v| v.as_str()))
                    .unwrap_or_default()
                    .to_owned(),
                ip_addresses: node
                    .get("ipAddresses")
                    .and_then(|v| v.as_array())
                    .map(|addresses| {
                        addresses
                            .iter()
                            .filter_map(|a| a.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                online: node
                    .get("online")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                last_seen: node
                    .get("lastSeen")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
            })
            .collect())
    }

    /// Finds the tailnet address of the node named `rvnc-<device_code>`.
    pub async fn resolve_node_ip(
        &self,
        device_code: &str,
    ) -> Result<Option<String>, HeadscaleError> {
        let hostname = format!("rvnc-{}", device_code.to_ascii_lowercase());
        let nodes = self.list_nodes().await?;
        for node in nodes {
            if node.name.eq_ignore_ascii_case(&hostname)
                && let Some(ip) = node.ip_addresses.into_iter().find(|ip| {
                    ip.parse::<std::net::Ipv4Addr>()
                        .map(rv_net::is_tailnet_ip)
                        .unwrap_or(false)
                })
            {
                return Ok(Some(ip));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HeadscaleError {
    #[error(transparent)]
    Db(#[from] crate::db::DbError),
    #[error("headscale transport error: {0}")]
    Transport(String),
    #[error("headscale API error: {0}")]
    Api(String),
}
