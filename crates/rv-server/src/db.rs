use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use rv_proto::{DeviceView, PairedController, PairingView, StatsView, TokenView};

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub id: String,
    pub code: String,
    pub name: String,
    pub password_hash: String,
    pub agent_secret_hash: String,
    pub tailscale_ip: Option<String>,
    pub platform: String,
    pub online: bool,
    #[allow(dead_code)]
    pub last_seen: Option<i64>,
    pub created_at: i64,
    pub listen_port: u16,
}

#[derive(Debug, Clone)]
pub struct TokenRecord {
    pub id: String,
    pub device_code: String,
    pub controller_id: String,
    pub controller_name: String,
    pub platform: String,
    pub token_hash: String,
    pub created_at: i64,
    pub expires_at: i64,
    #[allow(dead_code)]
    pub revoked_at: Option<i64>,
    #[allow(dead_code)]
    pub last_used: Option<i64>,
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &str) -> Result<Self, DbError> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = std::path::Path::new(path).parent()
                && !parent.as_os_str().is_empty()
            {
                let _ = std::fs::create_dir_all(parent);
            }
            Connection::open(path)?
        };
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        if path != ":memory:" {
            let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        }
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn open_memory() -> Result<Self, DbError> {
        Self::open(":memory:")
    }

    fn migrate(conn: &Connection) -> Result<(), DbError> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS devices (
                id                TEXT PRIMARY KEY,
                code              TEXT NOT NULL UNIQUE,
                name              TEXT NOT NULL,
                password_hash     TEXT NOT NULL,
                agent_secret_hash TEXT NOT NULL,
                tailscale_ip      TEXT,
                platform          TEXT NOT NULL,
                online            INTEGER NOT NULL DEFAULT 0,
                last_seen         INTEGER,
                created_at        INTEGER NOT NULL,
                listen_port       INTEGER NOT NULL DEFAULT 21416
            );

            CREATE TABLE IF NOT EXISTS tokens (
                id              TEXT PRIMARY KEY,
                device_code     TEXT NOT NULL,
                controller_id   TEXT NOT NULL,
                controller_name TEXT NOT NULL,
                platform        TEXT NOT NULL,
                token_hash      TEXT NOT NULL UNIQUE,
                created_at      INTEGER NOT NULL,
                expires_at      INTEGER NOT NULL,
                revoked_at      INTEGER,
                last_used       INTEGER,
                FOREIGN KEY (device_code) REFERENCES devices(code) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_tokens_device ON tokens(device_code);

            CREATE TABLE IF NOT EXISTS pairings (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                device_code     TEXT NOT NULL,
                controller_id   TEXT,
                controller_name TEXT NOT NULL,
                ip              TEXT NOT NULL,
                success         INTEGER NOT NULL,
                reason          TEXT,
                created_at      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pairings_created ON pairings(created_at DESC);

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn insert_device(&self, device: &DeviceRecord) -> Result<(), DbError> {
        self.lock().execute(
            "INSERT INTO devices
             (id, code, name, password_hash, agent_secret_hash, tailscale_ip,
              platform, online, last_seen, created_at, listen_port)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, NULL, ?8, ?9)",
            params![
                device.id,
                device.code,
                device.name,
                device.password_hash,
                device.agent_secret_hash,
                device.tailscale_ip,
                device.platform,
                device.created_at,
                device.listen_port,
            ],
        )?;
        Ok(())
    }

    pub fn device_by_code(&self, code: &str) -> Result<Option<DeviceRecord>, DbError> {
        let conn = self.lock();
        let record = conn
            .query_row(
                "SELECT id, code, name, password_hash, agent_secret_hash, tailscale_ip,
                        platform, online, last_seen, created_at, listen_port
                 FROM devices WHERE code = ?1",
                params![code],
                map_device,
            )
            .optional()?;
        Ok(record)
    }

    pub fn update_heartbeat(
        &self,
        code: &str,
        tailscale_ip: Option<&str>,
        listen_port: u16,
        now: i64,
    ) -> Result<bool, DbError> {
        let changed = self.lock().execute(
            "UPDATE devices
             SET online = 1, last_seen = ?2,
                 tailscale_ip = COALESCE(?3, tailscale_ip),
                 listen_port = ?4
             WHERE code = ?1",
            params![code, now, tailscale_ip, listen_port],
        )?;
        Ok(changed > 0)
    }

    pub fn mark_stale_offline(&self, cutoff: i64) -> Result<usize, DbError> {
        let changed = self.lock().execute(
            "UPDATE devices SET online = 0
             WHERE online = 1 AND (last_seen IS NULL OR last_seen < ?1)",
            params![cutoff],
        )?;
        Ok(changed)
    }

    pub fn update_password(&self, code: &str, password_hash: &str) -> Result<bool, DbError> {
        let changed = self.lock().execute(
            "UPDATE devices SET password_hash = ?2 WHERE code = ?1",
            params![code, password_hash],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_device(&self, code: &str) -> Result<bool, DbError> {
        let changed = self
            .lock()
            .execute("DELETE FROM devices WHERE code = ?1", params![code])?;
        Ok(changed > 0)
    }

    pub fn insert_token(&self, token: &TokenRecord) -> Result<(), DbError> {
        self.lock().execute(
            "INSERT INTO tokens
             (id, device_code, controller_id, controller_name, platform,
              token_hash, created_at, expires_at, revoked_at, last_used)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
            params![
                token.id,
                token.device_code,
                token.controller_id,
                token.controller_name,
                token.platform,
                token.token_hash,
                token.created_at,
                token.expires_at,
            ],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn token_by_hash(&self, hash: &str) -> Result<Option<TokenRecord>, DbError> {
        let conn = self.lock();
        let record = conn
            .query_row(
                "SELECT id, device_code, controller_id, controller_name, platform,
                        token_hash, created_at, expires_at, revoked_at, last_used
                 FROM tokens WHERE token_hash = ?1",
                params![hash],
                map_token,
            )
            .optional()?;
        Ok(record)
    }

    /// Returns the token only when it belongs to the device, is not revoked and
    /// has not expired.
    pub fn valid_token(
        &self,
        device_code: &str,
        hash: &str,
        now: i64,
    ) -> Result<Option<TokenRecord>, DbError> {
        let conn = self.lock();
        let record = conn
            .query_row(
                "SELECT id, device_code, controller_id, controller_name, platform,
                        token_hash, created_at, expires_at, revoked_at, last_used
                 FROM tokens
                 WHERE device_code = ?1 AND token_hash = ?2
                   AND revoked_at IS NULL AND expires_at > ?3",
                params![device_code, hash, now],
                map_token,
            )
            .optional()?;
        Ok(record)
    }

    pub fn touch_token(&self, id: &str, now: i64) -> Result<(), DbError> {
        self.lock().execute(
            "UPDATE tokens SET last_used = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn revoke_token(&self, id: &str, device_code: &str, now: i64) -> Result<bool, DbError> {
        let changed = self.lock().execute(
            "UPDATE tokens SET revoked_at = ?3
             WHERE id = ?1 AND device_code = ?2 AND revoked_at IS NULL",
            params![id, device_code, now],
        )?;
        Ok(changed > 0)
    }

    pub fn revoke_token_admin(&self, id: &str, now: i64) -> Result<bool, DbError> {
        let changed = self.lock().execute(
            "UPDATE tokens SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            params![id, now],
        )?;
        Ok(changed > 0)
    }

    pub fn active_controllers(
        &self,
        device_code: &str,
        now: i64,
    ) -> Result<Vec<PairedController>, DbError> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT id, controller_id, controller_name, platform, created_at, last_used
             FROM tokens
             WHERE device_code = ?1 AND revoked_at IS NULL AND expires_at > ?2
             ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map(params![device_code, now], |row| {
            Ok(PairedController {
                token_id: row.get(0)?,
                controller_id: row.get(1)?,
                name: row.get(2)?,
                platform: row.get(3)?,
                paired_at: row.get(4)?,
                last_seen: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_pairing(
        &self,
        device_code: &str,
        controller_id: Option<&str>,
        controller_name: &str,
        ip: &str,
        success: bool,
        reason: Option<&str>,
        now: i64,
    ) -> Result<(), DbError> {
        self.lock().execute(
            "INSERT INTO pairings
             (device_code, controller_id, controller_name, ip, success, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                device_code,
                controller_id,
                controller_name,
                ip,
                i64::from(success),
                reason,
                now
            ],
        )?;
        Ok(())
    }

    pub fn all_devices(&self, now: i64) -> Result<Vec<DeviceView>, DbError> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT d.id, d.code, d.name, d.platform, d.tailscale_ip, d.online,
                    d.last_seen, d.created_at,
                    (SELECT COUNT(DISTINCT controller_id) FROM tokens t
                     WHERE t.device_code = d.code AND t.revoked_at IS NULL
                       AND t.expires_at > ?1) AS paired,
                    (SELECT COUNT(*) FROM tokens t
                     WHERE t.device_code = d.code AND t.revoked_at IS NULL
                       AND t.expires_at > ?1) AS active
             FROM devices d
             ORDER BY d.created_at DESC",
        )?;
        let rows = statement.query_map(params![now], |row| {
            Ok(DeviceView {
                id: row.get(0)?,
                code: row.get(1)?,
                name: row.get(2)?,
                platform: row.get(3)?,
                tailscale_ip: row.get(4)?,
                online: row.get::<_, i64>(5)? != 0,
                last_seen: row.get(6)?,
                created_at: row.get(7)?,
                paired_controllers: row.get(8)?,
                active_tokens: row.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn all_tokens(&self) -> Result<Vec<TokenView>, DbError> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT t.id, t.device_code, COALESCE(d.name, ''), t.controller_id,
                    t.controller_name, t.platform, t.created_at, t.expires_at,
                    t.revoked_at, t.last_used
             FROM tokens t
             LEFT JOIN devices d ON d.code = t.device_code
             ORDER BY t.created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(TokenView {
                id: row.get(0)?,
                device_code: row.get(1)?,
                device_name: row.get(2)?,
                controller_id: row.get(3)?,
                controller_name: row.get(4)?,
                platform: row.get(5)?,
                created_at: row.get(6)?,
                expires_at: row.get(7)?,
                revoked_at: row.get(8)?,
                last_used: row.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn recent_pairings(&self, limit: i64) -> Result<Vec<PairingView>, DbError> {
        let conn = self.lock();
        let mut statement = conn.prepare(
            "SELECT id, device_code, controller_name, ip, success, reason, created_at
             FROM pairings ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit.clamp(1, 500)], |row| {
            Ok(PairingView {
                id: row.get(0)?,
                device_code: row.get(1)?,
                controller_name: row.get(2)?,
                ip: row.get(3)?,
                success: row.get::<_, i64>(4)? != 0,
                reason: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn stats(&self, now: i64, day_start: i64) -> Result<StatsView, DbError> {
        let conn = self.lock();
        let devices_total = conn.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))?;
        let devices_online = conn.query_row(
            "SELECT COUNT(*) FROM devices WHERE online = 1 AND last_seen >= ?1",
            params![now - 90],
            |r| r.get(0),
        )?;
        let active_tokens = conn.query_row(
            "SELECT COUNT(*) FROM tokens WHERE revoked_at IS NULL AND expires_at > ?1",
            params![now],
            |r| r.get(0),
        )?;
        let pairings_today = conn.query_row(
            "SELECT COUNT(*) FROM pairings WHERE success = 1 AND created_at >= ?1",
            params![day_start],
            |r| r.get(0),
        )?;
        Ok(StatsView {
            devices_total,
            devices_online,
            active_tokens,
            pairings_today,
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.lock().execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_setting_if_missing(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.lock().execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO NOTHING",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn setting_or(&self, key: &str, default: &str) -> Result<String, DbError> {
        Ok(self.get_setting(key)?.unwrap_or_else(|| default.to_owned()))
    }
}

fn map_device(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeviceRecord> {
    Ok(DeviceRecord {
        id: row.get(0)?,
        code: row.get(1)?,
        name: row.get(2)?,
        password_hash: row.get(3)?,
        agent_secret_hash: row.get(4)?,
        tailscale_ip: row.get(5)?,
        platform: row.get(6)?,
        online: row.get::<_, i64>(7)? != 0,
        last_seen: row.get(8)?,
        created_at: row.get(9)?,
        listen_port: row.get::<_, i64>(10)?.clamp(1, 65535) as u16,
    })
}

fn map_token(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenRecord> {
    Ok(TokenRecord {
        id: row.get(0)?,
        device_code: row.get(1)?,
        controller_id: row.get(2)?,
        controller_name: row.get(3)?,
        platform: row.get(4)?,
        token_hash: row.get(5)?,
        created_at: row.get(6)?,
        expires_at: row.get(7)?,
        revoked_at: row.get(8)?,
        last_used: row.get(9)?,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

impl DbError {
    pub fn is_unique_violation(&self) -> bool {
        matches!(
            self,
            DbError::Sqlite(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::ConstraintViolation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(code: &str) -> DeviceRecord {
        DeviceRecord {
            id: format!("id-{code}"),
            code: code.to_owned(),
            name: "desk".into(),
            password_hash: "hash".into(),
            agent_secret_hash: "secret".into(),
            tailscale_ip: None,
            platform: "windows".into(),
            online: false,
            last_seen: None,
            created_at: now_secs(),
            listen_port: 21416,
        }
    }

    #[test]
    fn device_and_heartbeat_roundtrip() {
        let db = Db::open_memory().unwrap();
        db.insert_device(&device("A2B3C4")).unwrap();
        let stored = db.device_by_code("A2B3C4").unwrap().unwrap();
        assert!(!stored.online);
        assert!(
            db.update_heartbeat("A2B3C4", Some("100.64.0.7"), 21416, now_secs())
                .unwrap()
        );
        let stored = db.device_by_code("A2B3C4").unwrap().unwrap();
        assert!(stored.online);
        assert_eq!(stored.tailscale_ip.as_deref(), Some("100.64.0.7"));
    }

    #[test]
    fn token_lifecycle() {
        let db = Db::open_memory().unwrap();
        db.insert_device(&device("A2B3C4")).unwrap();
        let now = now_secs();
        db.insert_token(&TokenRecord {
            id: "t1".into(),
            device_code: "A2B3C4".into(),
            controller_id: "c1".into(),
            controller_name: "phone".into(),
            platform: "android".into(),
            token_hash: "ab".into(),
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used: None,
        })
        .unwrap();
        assert!(db.valid_token("A2B3C4", "ab", now).unwrap().is_some());
        assert!(
            db.valid_token("A2B3C4", "ab", now + 7200)
                .unwrap()
                .is_none()
        );
        assert!(db.revoke_token("t1", "A2B3C4", now).unwrap());
        assert!(db.valid_token("A2B3C4", "ab", now).unwrap().is_none());
        let controllers = db.active_controllers("A2B3C4", now).unwrap();
        assert!(controllers.is_empty());
    }

    #[test]
    fn cascade_delete_removes_tokens() {
        let db = Db::open_memory().unwrap();
        db.insert_device(&device("A2B3C4")).unwrap();
        let now = now_secs();
        db.insert_token(&TokenRecord {
            id: "t1".into(),
            device_code: "A2B3C4".into(),
            controller_id: "c1".into(),
            controller_name: "phone".into(),
            platform: "android".into(),
            token_hash: "ab".into(),
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used: None,
        })
        .unwrap();
        assert!(db.delete_device("A2B3C4").unwrap());
        assert!(db.token_by_hash("ab").unwrap().is_none());
    }
}
