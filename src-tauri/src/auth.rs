//! Remote admin authentication primitives.
//! Stores the remote management password hash on disk and keeps login sessions in memory.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::http::{header, HeaderMap};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const SESSION_TTL_HOURS: i64 = 12;
const MIN_PASSWORD_LEN: usize = 8;
pub const SESSION_COOKIE_NAME: &str = "aor_session";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRemoteAdminAuth {
    #[serde(default)]
    password_hash: String,
    updated_at: Option<String>,
}

#[derive(Debug, Clone)]
struct AuthSession {
    expires_at: DateTime<Utc>,
    password_updated_at: Option<String>,
}

#[derive(Clone)]
pub struct RemoteAdminAuthStore {
    file_path: PathBuf,
    persisted: Arc<Mutex<StoredRemoteAdminAuth>>,
    sessions: Arc<Mutex<HashMap<String, AuthSession>>>,
}

impl RemoteAdminAuthStore {
    /// Creates a new remote admin auth store.
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            persisted: Arc::new(Mutex::new(StoredRemoteAdminAuth::default())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Loads the persisted password hash if present and ensures the parent directory exists.
    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create remote admin auth dir failed: {e}"))?;
        }

        if !self.file_path.exists() {
            return self.write_persisted(&StoredRemoteAdminAuth::default());
        }

        let raw = std::fs::read_to_string(&self.file_path)
            .map_err(|e| format!("read remote admin auth failed: {e}"))?;
        let parsed = serde_json::from_str::<StoredRemoteAdminAuth>(&raw)
            .map_err(|e| format!("parse remote admin auth failed: {e}"))?;
        let mut guard = self
            .persisted
            .lock()
            .map_err(|_| "remote admin auth lock poisoned".to_string())?;
        *guard = parsed;
        Ok(())
    }

    /// Returns whether a remote management password is configured.
    pub fn password_configured(&self) -> bool {
        self.snapshot()
            .map(|value| !value.password_hash.trim().is_empty())
            .unwrap_or(false)
    }

    /// Sets or rotates the remote management password.
    pub fn set_password(&self, password: &str) -> Result<(), String> {
        let trimmed = password.trim();
        if trimmed.len() < MIN_PASSWORD_LEN {
            return Err(format!(
                "remote admin password must be at least {MIN_PASSWORD_LEN} characters"
            ));
        }

        let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
            .map_err(|e| format!("generate remote admin password salt failed: {e}"))?;
        let password_hash = Argon2::default()
            .hash_password(trimmed.as_bytes(), &salt)
            .map_err(|e| format!("hash remote admin password failed: {e}"))?
            .to_string();

        let next = StoredRemoteAdminAuth {
            password_hash,
            updated_at: Some(Utc::now().to_rfc3339()),
        };
        self.write_persisted(&next)?;
        self.revoke_all_sessions();
        Ok(())
    }

    /// Clears the configured remote management password and all active sessions.
    pub fn clear_password(&self) -> Result<(), String> {
        self.write_persisted(&StoredRemoteAdminAuth::default())?;
        self.revoke_all_sessions();
        Ok(())
    }

    /// Verifies a plaintext password against the configured password hash.
    pub fn verify_password(&self, password: &str) -> Result<bool, String> {
        let snapshot = self.snapshot()?;
        if snapshot.password_hash.trim().is_empty() {
            return Ok(false);
        }

        let parsed = PasswordHash::new(snapshot.password_hash.trim())
            .map_err(|e| format!("parse remote admin password hash failed: {e}"))?;
        Ok(Argon2::default()
            .verify_password(password.trim().as_bytes(), &parsed)
            .is_ok())
    }

    /// Issues a new browser session token bound to the current password revision.
    pub fn issue_session(&self) -> Result<String, String> {
        let snapshot = self.snapshot()?;
        if snapshot.password_hash.trim().is_empty() {
            return Err("remote admin password is not configured".to_string());
        }

        let token = Uuid::new_v4().to_string();
        let session = AuthSession {
            expires_at: Utc::now() + Duration::hours(SESSION_TTL_HOURS),
            password_updated_at: snapshot.updated_at.clone(),
        };

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote admin sessions lock poisoned".to_string())?;
        sessions.insert(token.clone(), session);
        Ok(token)
    }

    /// Verifies a browser session token and invalidates stale or expired sessions.
    pub fn verify_session(&self, token: &str) -> Result<bool, String> {
        if token.trim().is_empty() {
            return Ok(false);
        }

        let snapshot = self.snapshot()?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "remote admin sessions lock poisoned".to_string())?;
        let Some(session) = sessions.get(token).cloned() else {
            return Ok(false);
        };

        let expired = session.expires_at <= Utc::now();
        let password_rotated = session.password_updated_at != snapshot.updated_at;
        if expired || password_rotated || snapshot.password_hash.trim().is_empty() {
            sessions.remove(token);
            return Ok(false);
        }

        Ok(true)
    }

    /// Revokes one browser session token if present.
    pub fn revoke_session(&self, token: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(token);
        }
    }

    /// Revokes all browser sessions.
    pub fn revoke_all_sessions(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.clear();
        }
    }

    /// Authenticates a remote admin request using either bearer password or session cookie.
    pub fn authenticate_request(&self, headers: &HeaderMap) -> Result<bool, String> {
        if let Some(password) = extract_bearer_token(headers) {
            if self.verify_password(&password)? {
                return Ok(true);
            }
        }

        if let Some(session_token) = extract_cookie_value(headers, SESSION_COOKIE_NAME) {
            if self.verify_session(&session_token)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn snapshot(&self) -> Result<StoredRemoteAdminAuth, String> {
        self.persisted
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| "remote admin auth lock poisoned".to_string())
    }

    fn write_persisted(&self, next: &StoredRemoteAdminAuth) -> Result<(), String> {
        let text = serde_json::to_string_pretty(next)
            .map_err(|e| format!("serialize remote admin auth failed: {e}"))?;
        std::fs::write(&self.file_path, text)
            .map_err(|e| format!("write remote admin auth failed: {e}"))?;
        let mut guard = self
            .persisted
            .lock()
            .map_err(|_| "remote admin auth lock poisoned".to_string())?;
        *guard = next.clone();
        Ok(())
    }
}

/// Returns whether the incoming request should be treated as remote.
pub fn is_remote_request(socket_addr: Option<SocketAddr>) -> bool {
    socket_addr
        .map(|addr| !addr.ip().is_loopback())
        .unwrap_or(false)
}

/// Extracts a bearer token from the Authorization header.
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let token = raw.strip_prefix("Bearer ")?;
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

/// Extracts one named cookie value from the Cookie header.
pub fn extract_cookie_value(headers: &HeaderMap, key: &str) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    for cookie in cookies.split(';') {
        let trimmed = cookie.trim();
        let (name, value) = trimmed.split_once('=')?;
        if name.trim() == key {
            let cookie_value = value.trim();
            if !cookie_value.is_empty() {
                return Some(cookie_value.to_string());
            }
        }
    }
    None
}

/// Creates a Set-Cookie header value for a fresh authenticated session.
pub fn build_session_cookie(token: &str) -> String {
    format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        Duration::hours(SESSION_TTL_HOURS).num_seconds()
    )
}

/// Creates a Set-Cookie header value that clears the browser session.
pub fn build_clear_session_cookie() -> String {
    format!(
        "{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_clear_session_cookie, build_session_cookie, extract_bearer_token,
        extract_cookie_value, is_remote_request, RemoteAdminAuthStore, SESSION_COOKIE_NAME,
    };
    use axum::http::{header, HeaderMap, HeaderValue};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_store_path() -> PathBuf {
        std::env::temp_dir().join(format!("aor-remote-admin-auth-{}.json", Uuid::new_v4()))
    }

    #[test]
    fn remote_request_detection_uses_loopback_only() {
        assert!(!is_remote_request(Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            8899
        ))));
        assert!(is_remote_request(Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            8899
        ))));
        assert!(!is_remote_request(None));
    }

    #[test]
    fn password_hash_verification_round_trip_works() {
        let store = RemoteAdminAuthStore::new(temp_store_path());
        store.initialize().expect("store should init");
        store
            .set_password("correct horse battery staple")
            .expect("set password");

        assert!(store
            .verify_password("correct horse battery staple")
            .unwrap());
        assert!(!store.verify_password("wrong").unwrap());
        assert!(store.password_configured());
    }

    #[test]
    fn issuing_new_password_invalidates_existing_sessions() {
        let store = RemoteAdminAuthStore::new(temp_store_path());
        store.initialize().expect("store should init");
        store.set_password("password-one").expect("set password");
        let session = store.issue_session().expect("issue session");
        assert!(store.verify_session(&session).unwrap());

        store.set_password("password-two").expect("rotate password");
        assert!(!store.verify_session(&session).unwrap());
    }

    #[test]
    fn auth_headers_support_bearer_and_cookie() {
        let store = RemoteAdminAuthStore::new(temp_store_path());
        store.initialize().expect("store should init");
        store
            .set_password("very-secret-password")
            .expect("set password");
        let session = store.issue_session().expect("issue session");

        let mut bearer_headers = HeaderMap::new();
        bearer_headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer very-secret-password"),
        );
        assert!(store.authenticate_request(&bearer_headers).unwrap());

        let mut cookie_headers = HeaderMap::new();
        cookie_headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE_NAME}={session}"))
                .expect("cookie header"),
        );
        assert!(store.authenticate_request(&cookie_headers).unwrap());
    }

    #[test]
    fn cookie_helpers_extract_expected_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-123"),
        );
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("foo=bar; aor_session=session-xyz"),
        );

        assert_eq!(extract_bearer_token(&headers).as_deref(), Some("token-123"));
        assert_eq!(
            extract_cookie_value(&headers, SESSION_COOKIE_NAME).as_deref(),
            Some("session-xyz")
        );
        assert!(build_session_cookie("session-xyz").contains("HttpOnly"));
        assert!(build_clear_session_cookie().contains("Max-Age=0"));
    }
}
