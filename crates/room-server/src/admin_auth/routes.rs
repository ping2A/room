//! Admin auth HTTP API.

use super::webauthn::AuthenticatorResponse;
use super::{AuthStore, LoginStep};
use crate::state::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::net::SocketAddr;

/// Auth routes (merged into admin router). Static assets stay public.
pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/api/v1/auth/status", get(status))
        .route("/admin/api/v1/auth/bootstrap", post(bootstrap))
        .route("/admin/api/v1/auth/login", post(login))
        .route("/admin/api/v1/auth/totp", post(totp_verify))
        .route("/admin/api/v1/auth/totp/enroll", post(totp_enroll_start))
        .route("/admin/api/v1/auth/totp/confirm", post(totp_enroll_confirm))
        .route(
            "/admin/api/v1/auth/webauthn/login/begin",
            post(webauthn_login_begin),
        )
        .route(
            "/admin/api/v1/auth/webauthn/login/finish",
            post(webauthn_login_finish),
        )
        .route(
            "/admin/api/v1/auth/webauthn/register/begin",
            post(webauthn_register_begin),
        )
        .route(
            "/admin/api/v1/auth/webauthn/register/finish",
            post(webauthn_register_finish),
        )
        .route("/admin/api/v1/auth/logout", post(logout))
        .route("/admin/api/v1/auth/me", get(me))
        .route("/admin/api/v1/auth/users", get(list_users))
}

async fn status(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let token_configured = !state.registry.config().admin_token.is_empty();
    Json(state.auth.status_json(&headers, token_configured))
}

#[derive(Deserialize)]
struct BootstrapBody {
    username: String,
    password: String,
}

async fn bootstrap(
    State(state): State<AppState>,
    Json(body): Json<BootstrapBody>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if state.auth.has_users() {
        return Err((StatusCode::CONFLICT, "admins already configured".into()));
    }
    state
        .auth
        .bootstrap_user(&body.username, &body.password)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    state.admin.audit(
        &body.username.to_ascii_lowercase(),
        "auth_bootstrap",
        "admins",
        "first admin created",
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginBody>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    if !state.auth.has_users() {
        return Err((
            StatusCode::BAD_REQUEST,
            "no admin users — bootstrap first or use bearer token".into(),
        ));
    }
    let step = state
        .auth
        .login_password(&body.username, &body.password, &addr.ip().to_string())
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;
    if let Some((sid, csrf)) = step.session.clone() {
        state
            .admin
            .audit(&step.username, "auth_login", "session", "password");
        return Ok(
            session_response(state.auth.secure_cookie(), sid, csrf, "password").into_response(),
        );
    }
    Ok(login_step_response(
        state.auth.rp_id().to_owned(),
        state.auth.secure_cookie(),
        step,
    )
    .into_response())
}

fn login_step_response(rp_id: String, secure: bool, step: LoginStep) -> impl IntoResponse {
    let mut public_key: Option<serde_json::Value> = None;
    if let Some(ch) = &step.challenge {
        public_key = Some(
            serde_json::to_value(super::webauthn::request_options(
                ch,
                &rp_id,
                &step.credentials,
            ))
            .unwrap_or_default(),
        );
    }
    let body = serde_json::json!({
        "ok": true,
        "need": step.need_str(),
        "methods": step.methods,
        "username": step.username,
        "publicKey": public_key,
    });
    let cookie = AuthStore::cookie_header(
        AuthStore::pending_cookie_name(),
        &step.pending_id,
        600,
        secure,
    );
    ([(header::SET_COOKIE, cookie)], Json(body))
}

#[derive(Deserialize)]
struct TotpBody {
    code: String,
}

async fn totp_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TotpBody>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let pending = AuthStore::pending_id(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "password step required".into()))?;
    let (sid, csrf) = state
        .auth
        .verify_totp(&pending, &body.code)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;
    state.admin.audit("admin", "auth_login", "session", "totp");
    Ok(session_response(
        state.auth.secure_cookie(),
        sid,
        csrf,
        "totp",
    ))
}

async fn totp_enroll_start(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pending = AuthStore::pending_id(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "password step required".into()))?;
    let (secret, uri) = state
        .auth
        .start_totp_enroll(&pending)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::json!({
        "secret": secret,
        "otpauth": uri,
    })))
}

async fn totp_enroll_confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TotpBody>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let pending = AuthStore::pending_id(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "password step required".into()))?;
    let (sid, csrf) = state
        .auth
        .confirm_totp_enroll(&pending, &body.code)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;
    Ok(session_response(
        state.auth.secure_cookie(),
        sid,
        csrf,
        "totp_enroll",
    ))
}

async fn webauthn_login_begin(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let pending = AuthStore::pending_id(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "password step required".into()))?;
    let (_ch, opts) = state
        .auth
        .begin_webauthn_login(&pending)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::json!({ "publicKey": opts })))
}

#[derive(Deserialize)]
struct WebAuthnFinishBody {
    credential: AuthenticatorResponse,
    #[serde(default)]
    name: String,
}

async fn webauthn_login_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WebAuthnFinishBody>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let pending = AuthStore::pending_id(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "password step required".into()))?;
    let (sid, csrf) = state
        .auth
        .finish_webauthn_login(&pending, &body.credential)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;
    Ok(session_response(
        state.auth.secure_cookie(),
        sid,
        csrf,
        "webauthn",
    ))
}

async fn webauthn_register_begin(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if let Some(sid) = AuthStore::session_id(&headers) {
        let opts = state
            .auth
            .begin_webauthn_register_session(&sid)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        return Ok(Json(serde_json::json!({ "publicKey": opts })));
    }
    let pending = AuthStore::pending_id(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "password step required".into()))?;
    let opts = state
        .auth
        .begin_webauthn_register(&pending)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::json!({ "publicKey": opts })))
}

async fn webauthn_register_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WebAuthnFinishBody>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if let Some(sid) = AuthStore::session_id(&headers) {
        state
            .auth
            .finish_webauthn_register_session(&sid, &body.credential, &body.name)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        state
            .admin
            .audit("admin", "webauthn_register", "self", &body.name);
        return Ok((StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response());
    }
    let pending = AuthStore::pending_id(&headers)
        .ok_or((StatusCode::UNAUTHORIZED, "password step required".into()))?;
    let session = state
        .auth
        .finish_webauthn_register(&pending, &body.credential, &body.name, true)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let Some((sid, csrf)) = session else {
        return Ok(Json(serde_json::json!({ "ok": true })).into_response());
    };
    Ok(session_response(state.auth.secure_cookie(), sid, csrf, "webauthn_enroll").into_response())
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(sid) = AuthStore::session_id(&headers) {
        state.auth.logout(&sid);
    }
    let clear_s =
        AuthStore::clear_cookie(AuthStore::session_cookie_name(), state.auth.secure_cookie());
    let clear_p =
        AuthStore::clear_cookie(AuthStore::pending_cookie_name(), state.auth.secure_cookie());
    let mut res = Json(serde_json::json!({ "ok": true })).into_response();
    if let Ok(v) = HeaderValue::from_str(&clear_s) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&clear_p) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
    res
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let token_configured = !state.registry.config().admin_token.is_empty();
    Json(state.auth.status_json(&headers, token_configured))
}

async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    state.auth.authorize(
        &headers,
        &axum::http::Method::GET,
        &state.registry.config().admin_token,
    )?;
    Ok(Json(state.auth.list_users_public()))
}

fn session_response(secure: bool, sid: String, csrf: String, via: &str) -> impl IntoResponse {
    let session_cookie =
        AuthStore::cookie_header(AuthStore::session_cookie_name(), &sid, 8 * 3600, secure);
    let clear_pending = AuthStore::clear_cookie(AuthStore::pending_cookie_name(), secure);
    let body = serde_json::json!({ "ok": true, "csrf": csrf, "via": via });
    let mut res = Json(body).into_response();
    if let Ok(v) = HeaderValue::from_str(&session_cookie) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&clear_pending) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
    res
}
