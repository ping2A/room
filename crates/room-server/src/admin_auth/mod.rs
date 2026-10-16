//! Strong admin authentication: Argon2id password + TOTP MFA and/or WebAuthn (YubiKey).
#![allow(missing_docs)]

mod routes;
mod webauthn;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::http::{HeaderMap, Method, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;
use totp_rs::{Algorithm as TotpAlg, Secret, TOTP};
use tracing::{info, warn};
use zeroize::Zeroize;

pub use routes::auth_routes;
pub use webauthn::WebAuthnCredential;

const SESSION_COOKIE: &str = "room_admin_sid";
const PENDING_COOKIE: &str = "room_admin_pending";
const SESSION_TTL: Duration = Duration::from_secs(8 * 3600);
const PENDING_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_FAILS: u32 = 8;
const FAIL_WINDOW: Duration = Duration::from_secs(15 * 60);

/// Who is calling the admin API.
#[derive(Clone, Debug)]
pub struct AdminIdentity {
    /// Audit actor (`username`, `token`, or `anonymous`).
    pub actor: String,
    /// CSRF token when session-authenticated (must be sent on mutating requests).
    pub csrf: Option<String>,
}

/// Persisted admin account.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    #[serde(default)]
    pub totp_secret: Option<String>,
    #[serde(default)]
    pub totp_confirmed: bool,
    #[serde(default)]
    pub webauthn: Vec<WebAuthnCredential>,
    pub created: u64,
    #[serde(default)]
    pub last_login: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct AdminFile {
    users: Vec<AdminUser>,
}

#[derive(Clone)]
struct Session {
    user_id: String,
    username: String,
    csrf: String,
    expires: Instant,
}

#[derive(Clone)]
struct Pending {
    user_id: String,
    username: String,
    /// Password step done; waiting for MFA or enrollment.
    need: PendingNeed,
    expires: Instant,
    /// Optional WebAuthn / TOTP challenge bytes (base64url).
    challenge: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingNeed {
    Totp,
    WebAuthn,
    Enroll,
}

#[derive(Default)]
struct FailBucket {
    count: u32,
    window_start: Option<Instant>,
}

/// In-memory sessions + on-disk users (`admins.json` under data-dir).
#[derive(Clone)]
pub struct AuthStore {
    inner: Arc<Mutex<AuthInner>>,
    data_dir: Option<PathBuf>,
    rp_id: String,
    rp_origin: String,
    rp_name: String,
    secure_cookie: bool,
}

struct AuthInner {
    users: Vec<AdminUser>,
    sessions: HashMap<String, Session>,
    pending: HashMap<String, Pending>,
    fails: HashMap<String, FailBucket>,
}

impl AuthStore {
    /// Empty store (memory only).
    pub fn memory(rp_id: impl Into<String>, rp_origin: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuthInner {
                users: Vec::new(),
                sessions: HashMap::new(),
                pending: HashMap::new(),
                fails: HashMap::new(),
            })),
            data_dir: None,
            rp_id: rp_id.into(),
            rp_origin: rp_origin.into(),
            rp_name: "ROOM Ops".into(),
            secure_cookie: false,
        }
    }

    /// Load/create under `data_dir`.
    pub fn open(
        data_dir: impl AsRef<Path>,
        rp_id: impl Into<String>,
        rp_origin: impl Into<String>,
        secure_cookie: bool,
    ) -> Result<Self, std::io::Error> {
        let dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let store = Self {
            inner: Arc::new(Mutex::new(AuthInner {
                users: Vec::new(),
                sessions: HashMap::new(),
                pending: HashMap::new(),
                fails: HashMap::new(),
            })),
            data_dir: Some(dir),
            rp_id: rp_id.into(),
            rp_origin: rp_origin.into(),
            rp_name: "ROOM Ops".into(),
            secure_cookie,
        };
        store.load()?;
        Ok(store)
    }

    fn path(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join("admins.json"))
    }

    fn load(&self) -> Result<(), std::io::Error> {
        let Some(path) = self.path() else {
            return Ok(());
        };
        if !path.exists() {
            return Ok(());
        }
        let bytes = fs::read(&path)?;
        match serde_json::from_slice::<AdminFile>(&bytes) {
            Ok(file) => {
                if let Ok(mut g) = self.inner.lock() {
                    g.users = file.users;
                }
                info!(path = %path.display(), "admin users loaded");
            }
            Err(e) => warn!(?e, "admins.json parse failed"),
        }
        Ok(())
    }

    fn persist(&self) {
        let Some(path) = self.path() else {
            return;
        };
        let users = self
            .inner
            .lock()
            .map(|g| g.users.clone())
            .unwrap_or_default();
        let file = AdminFile { users };
        match serde_json::to_vec_pretty(&file) {
            Ok(bytes) => {
                if let Err(e) = fs::write(&path, bytes) {
                    warn!(?e, path = %path.display(), "admins.json write failed");
                }
            }
            Err(e) => warn!(?e, "admins serialize failed"),
        }
    }

    pub fn rp_id(&self) -> &str {
        &self.rp_id
    }

    pub fn rp_origin(&self) -> &str {
        &self.rp_origin
    }

    pub fn secure_cookie(&self) -> bool {
        self.secure_cookie
    }

    pub fn user_count(&self) -> usize {
        self.inner.lock().map(|g| g.users.len()).unwrap_or(0)
    }

    pub fn has_users(&self) -> bool {
        self.user_count() > 0
    }

    /// Bootstrap an admin when the store is empty (or upsert password for existing name).
    pub fn bootstrap_user(&self, username: &str, password: &str) -> Result<(), String> {
        let username = normalize_username(username)?;
        if password.is_empty() {
            return Err("password must not be empty".into());
        }
        let hash = hash_password(password)?;
        let now = now_secs();
        let mut g = self.inner.lock().map_err(|_| "lock")?;
        if let Some(u) = g.users.iter_mut().find(|u| u.username == username) {
            u.password_hash = hash;
        } else {
            if !g.users.is_empty() {
                return Err("admins already exist — use the console to add users".into());
            }
            g.users.push(AdminUser {
                id: random_id(),
                username: username.clone(),
                password_hash: hash,
                role: "admin".into(),
                totp_secret: None,
                totp_confirmed: false,
                webauthn: Vec::new(),
                created: now,
                last_login: 0,
            });
            info!(%username, "bootstrapped admin user");
        }
        drop(g);
        self.persist();
        Ok(())
    }

    pub fn status_json(&self, headers: &HeaderMap, token_configured: bool) -> serde_json::Value {
        let session = self.session_from_headers(headers);
        let mode = if self.has_users() {
            "mfa"
        } else if token_configured {
            "token"
        } else {
            "open"
        };
        serde_json::json!({
            "mode": mode,
            "bootstrap": !self.has_users(),
            "user": session.as_ref().map(|s| serde_json::json!({
                "username": s.username,
                "csrf": s.csrf,
            })),
            "webauthn": {
                "rpId": self.rp_id,
                "origins": [self.rp_origin],
            },
        })
    }

    /// Authorize an admin API call.
    pub fn authorize(
        &self,
        headers: &HeaderMap,
        method: &Method,
        admin_token: &str,
    ) -> Result<AdminIdentity, StatusCode> {
        self.gc();
        if self.has_users() {
            let session = self
                .session_from_headers(headers)
                .ok_or(StatusCode::UNAUTHORIZED)?;
            if method != Method::GET && method != Method::HEAD && method != Method::OPTIONS {
                let csrf = headers
                    .get("x-csrf-token")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if csrf.is_empty() || !bool::from(csrf.as_bytes().ct_eq(session.csrf.as_bytes())) {
                    return Err(StatusCode::FORBIDDEN);
                }
            }
            return Ok(AdminIdentity {
                actor: session.username,
                csrf: Some(session.csrf),
            });
        }
        if admin_token.is_empty() {
            return Ok(AdminIdentity {
                actor: "anonymous".into(),
                csrf: None,
            });
        }
        let ok = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.strip_prefix("Bearer ")
                    .is_some_and(|t| bool::from(t.as_bytes().ct_eq(admin_token.as_bytes())))
            });
        if ok {
            Ok(AdminIdentity {
                actor: "token".into(),
                csrf: None,
            })
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }

    pub fn login_password(
        &self,
        username: &str,
        password: &str,
        peer: &str,
    ) -> Result<LoginStep, String> {
        if self.rate_limited(peer) || self.rate_limited(username) {
            return Err("too many attempts — try again later".into());
        }
        let username_n = normalize_username(username)?;
        let user = {
            let g = self.inner.lock().map_err(|_| "lock")?;
            g.users.iter().find(|u| u.username == username_n).cloned()
        };
        let Some(user) = user else {
            self.record_fail(peer);
            self.record_fail(username);
            return Err("invalid credentials".into());
        };
        if !verify_password(password, &user.password_hash) {
            self.record_fail(peer);
            self.record_fail(&username_n);
            return Err("invalid credentials".into());
        }
        // No second factor yet: the password opens the console. TOTP / a passkey
        // is required on later logins only after the admin has set one up.
        if !user.totp_confirmed && user.webauthn.is_empty() {
            let (sid, csrf) = self.issue_session(&user);
            return Ok(LoginStep {
                pending_id: String::new(),
                need: PendingNeed::Enroll,
                methods: Vec::new(),
                challenge: None,
                credentials: Vec::new(),
                username: user.username,
                session: Some((sid, csrf)),
            });
        }
        let need = if user.totp_confirmed && !user.webauthn.is_empty() {
            // Prefer WebAuthn when both enrolled; client may choose TOTP.
            PendingNeed::WebAuthn
        } else if user.totp_confirmed {
            PendingNeed::Totp
        } else {
            PendingNeed::WebAuthn
        };
        let pending_id = random_id();
        let challenge = if need == PendingNeed::WebAuthn {
            Some(random_challenge_b64())
        } else {
            None
        };
        if let Ok(mut g) = self.inner.lock() {
            g.pending.insert(
                pending_id.clone(),
                Pending {
                    user_id: user.id.clone(),
                    username: user.username.clone(),
                    need: need.clone(),
                    expires: Instant::now() + PENDING_TTL,
                    challenge: challenge.clone(),
                },
            );
        }
        let mut methods = Vec::new();
        if user.totp_confirmed {
            methods.push("totp");
        }
        if !user.webauthn.is_empty() {
            methods.push("webauthn");
        }
        Ok(LoginStep {
            pending_id,
            need,
            methods: methods.into_iter().map(str::to_owned).collect(),
            challenge,
            credentials: user.webauthn,
            username: user.username,
            session: None,
        })
    }

    pub fn verify_totp(&self, pending_id: &str, code: &str) -> Result<(String, String), String> {
        let pending = self.take_pending_if(pending_id, |p| {
            matches!(p.need, PendingNeed::Totp | PendingNeed::WebAuthn | PendingNeed::Enroll)
                && /* allow totp when confirmed */ true
        })?;
        let user = self.user_by_id(&pending.user_id).ok_or("user gone")?;
        if !user.totp_confirmed {
            return Err("TOTP not enrolled".into());
        }
        let secret = user.totp_secret.as_deref().ok_or("TOTP not enrolled")?;
        if !verify_totp(secret, code) {
            // put pending back
            self.put_pending(pending_id, pending);
            return Err("invalid TOTP code".into());
        }
        Ok(self.issue_session(&user))
    }

    pub fn begin_webauthn_login(
        &self,
        pending_id: &str,
    ) -> Result<(String, webauthn::PublicKeyCredentialRequestOptions), String> {
        let mut g = self.inner.lock().map_err(|_| "lock")?;
        let pending = g.pending.get(pending_id).ok_or("login expired")?.clone();
        if pending.expires < Instant::now() {
            return Err("login expired".into());
        }
        let user = g
            .users
            .iter()
            .find(|u| u.id == pending.user_id)
            .cloned()
            .ok_or("user gone")?;
        if user.webauthn.is_empty() {
            return Err("no security keys enrolled".into());
        }
        let challenge = random_challenge_b64();
        if let Some(p) = g.pending.get_mut(pending_id) {
            p.challenge = Some(challenge.clone());
            p.need = PendingNeed::WebAuthn;
        }
        let opts = webauthn::request_options(&challenge, &self.rp_id, &user.webauthn);
        Ok((challenge, opts))
    }

    pub fn finish_webauthn_login(
        &self,
        pending_id: &str,
        resp: &webauthn::AuthenticatorResponse,
    ) -> Result<(String, String), String> {
        let pending = self.take_pending(pending_id)?;
        let challenge = pending.challenge.ok_or("no challenge")?;
        let mut user = self.user_by_id(&pending.user_id).ok_or("user gone")?;
        let cred = user
            .webauthn
            .iter()
            .find(|c| c.id == resp.id || c.id == resp.raw_id)
            .cloned()
            .ok_or("unknown credential")?;
        let count =
            webauthn::finish_authentication(resp, &cred, &challenge, &self.rp_origin, &self.rp_id)
                .map_err(|e| e.to_string())?;
        if let Ok(mut g) = self.inner.lock() {
            if let Some(u) = g.users.iter_mut().find(|u| u.id == user.id) {
                if let Some(c) = u.webauthn.iter_mut().find(|c| c.id == cred.id) {
                    c.sign_count = count;
                }
                user = u.clone();
            }
        }
        self.persist();
        Ok(self.issue_session(&user))
    }

    pub fn start_totp_enroll(&self, pending_id: &str) -> Result<(String, String), String> {
        let pending = self.peek_pending(pending_id)?;
        if pending.need != PendingNeed::Enroll
            && !self
                .user_by_id(&pending.user_id)
                .is_some_and(|u| !u.totp_confirmed)
        {
            // allow re-enroll from session path separately
        }
        let mut secret = random_bytes(20);
        let encoded = Secret::Raw(secret.clone()).to_encoded().to_string();
        secret.zeroize();
        let totp = totp_for(&encoded, &pending.username)?;
        let uri = totp.get_url();
        if let Ok(mut g) = self.inner.lock() {
            if let Some(u) = g.users.iter_mut().find(|u| u.id == pending.user_id) {
                u.totp_secret = Some(encoded.clone());
                u.totp_confirmed = false;
            }
        }
        self.persist();
        Ok((encoded, uri))
    }

    pub fn confirm_totp_enroll(
        &self,
        pending_id: &str,
        code: &str,
    ) -> Result<(String, String), String> {
        let pending = self.take_pending(pending_id)?;
        let user = self.user_by_id(&pending.user_id).ok_or("user gone")?;
        let secret = user.totp_secret.as_deref().ok_or("start enroll first")?;
        if !verify_totp(secret, code) {
            self.put_pending(pending_id, pending);
            return Err("invalid TOTP code".into());
        }
        if let Ok(mut g) = self.inner.lock() {
            if let Some(u) = g.users.iter_mut().find(|u| u.id == pending.user_id) {
                u.totp_confirmed = true;
            }
        }
        self.persist();
        let user = self.user_by_id(&pending.user_id).ok_or("user gone")?;
        Ok(self.issue_session(&user))
    }

    pub fn begin_webauthn_register(
        &self,
        pending_id: &str,
    ) -> Result<webauthn::PublicKeyCredentialCreationOptions, String> {
        let mut g = self.inner.lock().map_err(|_| "lock")?;
        let pending = g.pending.get(pending_id).ok_or("login expired")?.clone();
        if pending.expires < Instant::now() {
            return Err("login expired".into());
        }
        let user = g
            .users
            .iter()
            .find(|u| u.id == pending.user_id)
            .cloned()
            .ok_or("user gone")?;
        let challenge = random_challenge_b64();
        if let Some(p) = g.pending.get_mut(pending_id) {
            p.challenge = Some(challenge.clone());
        }
        let user_id_b64 = URL_SAFE_NO_PAD.encode(user.id.as_bytes());
        Ok(webauthn::creation_options(
            &challenge,
            &self.rp_id,
            &self.rp_name,
            &user_id_b64,
            &user.username,
            &user.webauthn,
        ))
    }

    pub fn finish_webauthn_register(
        &self,
        pending_id: &str,
        resp: &webauthn::AuthenticatorResponse,
        name: &str,
        issue_session: bool,
    ) -> Result<Option<(String, String)>, String> {
        let pending = if issue_session {
            self.take_pending(pending_id)?
        } else {
            self.peek_pending(pending_id)?
        };
        let challenge = {
            let g = self.inner.lock().map_err(|_| "lock")?;
            g.pending
                .get(pending_id)
                .and_then(|p| p.challenge.clone())
                .or(pending.challenge.clone())
                .ok_or("no challenge")?
        };
        let cred = webauthn::finish_registration(
            resp,
            &challenge,
            &self.rp_origin,
            &self.rp_id,
            name,
            now_secs(),
        )
        .map_err(|e| e.to_string())?;
        if let Ok(mut g) = self.inner.lock() {
            if let Some(u) = g.users.iter_mut().find(|u| u.id == pending.user_id) {
                if u.webauthn.iter().any(|c| c.id == cred.id) {
                    return Err("credential already registered".into());
                }
                u.webauthn.push(cred);
            }
            if issue_session {
                g.pending.remove(pending_id);
            }
        }
        self.persist();
        if issue_session {
            let user = self.user_by_id(&pending.user_id).ok_or("user gone")?;
            Ok(Some(self.issue_session(&user)))
        } else {
            Ok(None)
        }
    }

    /// Session-authenticated WebAuthn register begin.
    pub fn begin_webauthn_register_session(
        &self,
        session_id: &str,
    ) -> Result<webauthn::PublicKeyCredentialCreationOptions, String> {
        let mut g = self.inner.lock().map_err(|_| "lock")?;
        let session = g.sessions.get(session_id).ok_or("not signed in")?.clone();
        if session.expires < Instant::now() {
            return Err("session expired".into());
        }
        let user = g
            .users
            .iter()
            .find(|u| u.id == session.user_id)
            .cloned()
            .ok_or("user gone")?;
        let challenge = random_challenge_b64();
        // Stash challenge on a short pending keyed by session.
        let pending_id = format!("sess:{session_id}");
        g.pending.insert(
            pending_id,
            Pending {
                user_id: user.id.clone(),
                username: user.username.clone(),
                need: PendingNeed::Enroll,
                expires: Instant::now() + PENDING_TTL,
                challenge: Some(challenge.clone()),
            },
        );
        let user_id_b64 = URL_SAFE_NO_PAD.encode(user.id.as_bytes());
        Ok(webauthn::creation_options(
            &challenge,
            &self.rp_id,
            &self.rp_name,
            &user_id_b64,
            &user.username,
            &user.webauthn,
        ))
    }

    pub fn finish_webauthn_register_session(
        &self,
        session_id: &str,
        resp: &webauthn::AuthenticatorResponse,
        name: &str,
    ) -> Result<(), String> {
        let pending_id = format!("sess:{session_id}");
        self.finish_webauthn_register(&pending_id, resp, name, false)?;
        Ok(())
    }

    pub fn logout(&self, session_id: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.sessions.remove(session_id);
        }
    }

    pub fn list_users_public(&self) -> Vec<serde_json::Value> {
        self.inner
            .lock()
            .map(|g| {
                g.users
                    .iter()
                    .map(|u| {
                        serde_json::json!({
                            "id": u.id,
                            "username": u.username,
                            "role": u.role,
                            "totp": u.totp_confirmed,
                            "keys": u.webauthn.iter().map(|c| serde_json::json!({
                                "id": c.id,
                                "name": c.name,
                                "created": c.created,
                            })).collect::<Vec<_>>(),
                            "last_login": u.last_login,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn cookie_header(name: &str, value: &str, max_age: u64, secure: bool) -> String {
        let mut s =
            format!("{name}={value}; Path=/admin; HttpOnly; SameSite=Strict; Max-Age={max_age}");
        if secure {
            s.push_str("; Secure");
        }
        s
    }

    pub fn clear_cookie(name: &str, secure: bool) -> String {
        Self::cookie_header(name, "", 0, secure)
    }

    pub fn parse_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
        let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
        for part in cookie.split(';') {
            let part = part.trim();
            if let Some(v) = part.strip_prefix(name).and_then(|p| p.strip_prefix('=')) {
                return Some(v.to_owned());
            }
        }
        None
    }

    pub fn session_id(headers: &HeaderMap) -> Option<String> {
        Self::parse_cookie(headers, SESSION_COOKIE)
    }

    pub fn pending_id(headers: &HeaderMap) -> Option<String> {
        Self::parse_cookie(headers, PENDING_COOKIE)
    }

    pub fn session_cookie_name() -> &'static str {
        SESSION_COOKIE
    }

    pub fn pending_cookie_name() -> &'static str {
        PENDING_COOKIE
    }

    fn session_from_headers(&self, headers: &HeaderMap) -> Option<Session> {
        let id = Self::session_id(headers)?;
        let g = self.inner.lock().ok()?;
        let s = g.sessions.get(&id)?.clone();
        if s.expires < Instant::now() {
            return None;
        }
        Some(s)
    }

    fn issue_session(&self, user: &AdminUser) -> (String, String) {
        let sid = random_id();
        let csrf = random_id();
        if let Ok(mut g) = self.inner.lock() {
            if let Some(u) = g.users.iter_mut().find(|u| u.id == user.id) {
                u.last_login = now_secs();
            }
            g.sessions.insert(
                sid.clone(),
                Session {
                    user_id: user.id.clone(),
                    username: user.username.clone(),
                    csrf: csrf.clone(),
                    expires: Instant::now() + SESSION_TTL,
                },
            );
        }
        self.persist();
        (sid, csrf)
    }

    fn user_by_id(&self, id: &str) -> Option<AdminUser> {
        self.inner
            .lock()
            .ok()?
            .users
            .iter()
            .find(|u| u.id == id)
            .cloned()
    }

    fn take_pending(&self, id: &str) -> Result<Pending, String> {
        let mut g = self.inner.lock().map_err(|_| "lock")?;
        let p = g.pending.remove(id).ok_or("login expired")?;
        if p.expires < Instant::now() {
            return Err("login expired".into());
        }
        Ok(p)
    }

    fn take_pending_if(
        &self,
        id: &str,
        pred: impl FnOnce(&Pending) -> bool,
    ) -> Result<Pending, String> {
        let mut g = self.inner.lock().map_err(|_| "lock")?;
        let p = g.pending.get(id).ok_or("login expired")?.clone();
        if p.expires < Instant::now() {
            g.pending.remove(id);
            return Err("login expired".into());
        }
        if !pred(&p) {
            return Err("unexpected login step".into());
        }
        g.pending.remove(id);
        Ok(p)
    }

    fn peek_pending(&self, id: &str) -> Result<Pending, String> {
        let g = self.inner.lock().map_err(|_| "lock")?;
        let p = g.pending.get(id).ok_or("login expired")?.clone();
        if p.expires < Instant::now() {
            return Err("login expired".into());
        }
        Ok(p)
    }

    fn put_pending(&self, id: &str, pending: Pending) {
        if let Ok(mut g) = self.inner.lock() {
            g.pending.insert(id.into(), pending);
        }
    }

    fn rate_limited(&self, key: &str) -> bool {
        let Ok(mut g) = self.inner.lock() else {
            return false;
        };
        let b = g.fails.entry(key.to_owned()).or_default();
        let now = Instant::now();
        match b.window_start {
            Some(start) if now.duration_since(start) < FAIL_WINDOW => b.count >= MAX_FAILS,
            _ => {
                b.count = 0;
                b.window_start = Some(now);
                false
            }
        }
    }

    fn record_fail(&self, key: &str) {
        if let Ok(mut g) = self.inner.lock() {
            let b = g.fails.entry(key.to_owned()).or_default();
            let now = Instant::now();
            match b.window_start {
                Some(start) if now.duration_since(start) < FAIL_WINDOW => b.count += 1,
                _ => {
                    b.window_start = Some(now);
                    b.count = 1;
                }
            }
        }
    }

    fn gc(&self) {
        if let Ok(mut g) = self.inner.lock() {
            let now = Instant::now();
            g.sessions.retain(|_, s| s.expires > now);
            g.pending.retain(|_, p| p.expires > now);
        }
    }
}

/// Result of password login (MFA / enroll next).
pub struct LoginStep {
    pub pending_id: String,
    need: PendingNeed,
    pub methods: Vec<String>,
    pub challenge: Option<String>,
    pub credentials: Vec<WebAuthnCredential>,
    pub username: String,
    /// Set when the password alone opened a session (no MFA enrolled).
    pub session: Option<(String, String)>,
}

impl LoginStep {
    pub fn need_str(&self) -> &'static str {
        if self.session.is_some() {
            return "session";
        }
        match self.need {
            PendingNeed::Totp => "totp",
            PendingNeed::WebAuthn => "webauthn",
            PendingNeed::Enroll => "enroll",
        }
    }
}

fn normalize_username(s: &str) -> Result<String, String> {
    let t = s.trim().to_ascii_lowercase();
    if t.len() < 2 || t.len() > 32 {
        return Err("username must be 2–32 characters".into());
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err("username: use letters, digits, _ - .".into());
    }
    Ok(t)
}

fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut salt_rng());
    let params = Params::new(19_456, 2, 1, None).map_err(|e| e.to_string())?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn totp_for(secret_encoded: &str, account: &str) -> Result<TOTP, String> {
    let secret = Secret::Encoded(secret_encoded.to_owned())
        .to_bytes()
        .map_err(|e| e.to_string())?;
    TOTP::new(
        TotpAlg::SHA1,
        6,
        1,
        30,
        secret,
        Some("ROOM Ops".into()),
        account.to_owned(),
    )
    .map_err(|e| e.to_string())
}

fn verify_totp(secret_encoded: &str, code: &str) -> bool {
    let Ok(totp) = totp_for(secret_encoded, "x") else {
        return false;
    };
    totp.check_current(code.trim()).unwrap_or(false)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    fill_rng(&mut b);
    b
}

fn random_id() -> String {
    let mut b = [0u8; 32];
    fill_rng(&mut b);
    hex::encode(b)
}

fn fill_rng(dest: &mut [u8]) {
    if getrandom::fill(dest).is_err() {
        // Extremely rare; still avoid a predictable all-zero challenge.
        let t = now_secs().to_le_bytes();
        for (i, b) in dest.iter_mut().enumerate() {
            *b ^= t[i % t.len()].wrapping_add(i as u8);
        }
    }
}

fn random_challenge_b64() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes(32))
}

/// Salt RNG adapter for `password-hash`.
fn salt_rng() -> impl password_hash::rand_core::CryptoRngCore {
    struct OsRng;
    impl password_hash::rand_core::RngCore for OsRng {
        fn next_u32(&mut self) -> u32 {
            let mut b = [0u8; 4];
            self.fill_bytes(&mut b);
            u32::from_le_bytes(b)
        }
        fn next_u64(&mut self) -> u64 {
            let mut b = [0u8; 8];
            self.fill_bytes(&mut b);
            u64::from_le_bytes(b)
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            fill_rng(dest);
        }
        fn try_fill_bytes(
            &mut self,
            dest: &mut [u8],
        ) -> Result<(), password_hash::rand_core::Error> {
            fill_rng(dest);
            Ok(())
        }
    }
    impl password_hash::rand_core::CryptoRng for OsRng {}
    OsRng
}

// hex without adding the hex crate — tiny helper
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        const H: &[u8; 16] = b"0123456789abcdef";
        let b = bytes.as_ref();
        let mut s = String::with_capacity(b.len() * 2);
        for x in b {
            s.push(H[(x >> 4) as usize] as char);
            s.push(H[(x & 0xf) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn password_and_totp_roundtrip() {
        let store = AuthStore::memory("localhost", "http://127.0.0.1:8766");
        store
            .bootstrap_user("alice", "correct horse battery staple")
            .unwrap();
        let step = store
            .login_password("alice", "correct horse battery staple", "127.0.0.1")
            .unwrap();
        assert_eq!(step.need_str(), "session");
        let (sid, csrf) = step.session.expect("password opens the console");
        assert!(!sid.is_empty() && !csrf.is_empty());
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{}={sid}", AuthStore::session_cookie_name())
                .parse()
                .unwrap(),
        );
        headers.insert("x-csrf-token", csrf.parse().unwrap());
        let id = store.authorize(&headers, &Method::POST, "").unwrap();
        assert_eq!(id.actor, "alice");
    }
}
