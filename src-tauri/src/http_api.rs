//! Module Overview
//! HTTP API for management UI and external clients.
//! Provides JSON endpoints parallel to IPC commands.

use crate::app_state::{sync_runtime_config, SharedState};
use crate::auth::{
    build_clear_session_cookie, build_session_cookie, extract_cookie_value,
    is_remote_request_with_headers, SESSION_COOKIE_NAME,
};
use crate::backup::{backup_default_file_name, create_groups_backup_payload};
use crate::models::{
    AgentConfig, AgentConfigFile, AppInfo, AuthSessionStatus, ClipboardTextResult,
    GroupBackupExportResult, GroupBackupImportResult, GroupsExportJsonResult,
    IntegrationClientKind, IntegrationTarget, IntegrationWriteResult, LogEntry,
    ProviderModelTestResult, ProxyConfig, ProxyStatus, RemoteRulesPullResult,
    RemoteRulesUploadResult, RuleCardStatsItem, RuleQuotaConfig, RuleQuotaSnapshot,
    RuleQuotaTestResult, SaveConfigResult, StatsSummaryResult, WriteAgentConfigResult,
};
use crate::proxy::ServiceState;
use crate::services::{
    config_service, group_backup_service, integration_service, provider_service, quota_service,
    remote_rules_service, AppError,
};
use axum::extract::{ConnectInfo, Path as AxumPath, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::Json;
use axum::Router;
use mime_guess::MimeGuess;
#[cfg(not(debug_assertions))]
use rust_embed::RustEmbed;
use serde::de::Deserializer;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
}

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        match value {
            AppError::Validation { message } => {
                ApiError::new(StatusCode::BAD_REQUEST, "validation_error", message)
            }
            AppError::NotFound { message } => {
                ApiError::new(StatusCode::NOT_FOUND, "not_found", message)
            }
            AppError::External { message } => {
                ApiError::new(StatusCode::BAD_GATEWAY, "external_error", message)
            }
            AppError::Internal { message } => {
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let payload = json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        });
        (self.status, Json(payload)).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

fn unauthorized_remote_admin() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "authentication_required",
        "Remote management password required",
    )
}

fn require_shared_state(state: &ServiceState) -> Result<SharedState, ApiError> {
    state
        .shared_state
        .clone()
        .ok_or_else(|| ApiError::internal("shared state unavailable"))
}

fn require_app_handle(state: &ServiceState) -> Result<tauri::AppHandle, ApiError> {
    state.app_handle.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "unsupported",
            "app handle unavailable",
        )
    })
}

fn ensure_not_headless(state: &ServiceState, message: &'static str) -> Result<(), ApiError> {
    if state.app_handle.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "unsupported",
            message,
        ));
    }
    Ok(())
}

fn resolve_app_data_dir(
    state: &SharedState,
    app: Option<&tauri::AppHandle>,
) -> Result<PathBuf, ApiError> {
    if let Some(app) = app {
        return app
            .path()
            .app_data_dir()
            .map_err(|e| ApiError::internal(format!("resolve app_data_dir failed: {e}")));
    }
    let config_path = state.config_store.path();
    config_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| ApiError::internal("resolve app_data_dir failed"))
}

#[cfg(not(debug_assertions))]
#[derive(RustEmbed)]
#[folder = "../out/renderer"]
struct ManagementAssets;

#[cfg(debug_assertions)]
fn management_root() -> PathBuf {
    if let Ok(value) = std::env::var("AOR_MANAGEMENT_ROOT") {
        if !value.trim().is_empty() {
            return PathBuf::from(value.trim());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join("out/renderer");
    }
    PathBuf::from("out/renderer")
}

#[cfg(debug_assertions)]
fn management_response_fs(path: &str) -> Response {
    use std::fs;
    let root = management_root();
    let normalized = if path.is_empty() { "index.html" } else { path };
    let target = root.join(normalized);
    let fallback = root.join("index.html");
    let file_path = if target.exists() { target } else { fallback };
    let body = fs::read(&file_path).unwrap_or_default();
    let mime = MimeGuess::from_path(&file_path).first_or_octet_stream();
    let mut response = Response::new(body.into());
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
}

#[cfg(not(debug_assertions))]
fn management_response_embed(path: &str) -> Response {
    let normalized = if path.is_empty() { "index.html" } else { path };
    if let Some(content) = ManagementAssets::get(normalized) {
        let mime = MimeGuess::from_path(normalized).first_or_octet_stream();
        let mut response = Response::new(content.data.into());
        if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
        return response;
    }

    if let Some(content) = ManagementAssets::get("index.html") {
        let mut response = Response::new(content.data.into());
        if let Ok(value) = HeaderValue::from_str("text/html; charset=utf-8") {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
        return response;
    }

    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

fn strip_management_prefix(path: &str) -> &str {
    path.strip_prefix("/management").unwrap_or(path)
}

async fn management_handler(uri: Uri) -> Response {
    let raw = strip_management_prefix(uri.path());
    let trimmed = raw.trim_start_matches('/');
    #[cfg(debug_assertions)]
    {
        return management_response_fs(trimmed);
    }
    #[cfg(not(debug_assertions))]
    {
        return management_response_embed(trimmed);
    }
}

#[cfg(debug_assertions)]
fn asset_response_fs(path: &str) -> Response {
    use std::fs;
    let root = management_root().join("assets");
    let target = root.join(path);
    if !target.exists() {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }
    let body = fs::read(&target).unwrap_or_default();
    let mime = MimeGuess::from_path(&target).first_or_octet_stream();
    let mut response = Response::new(body.into());
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
}

#[cfg(not(debug_assertions))]
fn asset_response_embed(path: &str) -> Response {
    let asset_path = format!("assets/{path}");
    match ManagementAssets::get(asset_path.as_str()) {
        Some(content) => {
            let mime = MimeGuess::from_path(&asset_path).first_or_octet_stream();
            let mut response = Response::new(content.data.into());
            if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
                response.headers_mut().insert(header::CONTENT_TYPE, value);
            }
            response
        }
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

async fn asset_handler(AxumPath(path): AxumPath<String>) -> Response {
    let trimmed = path.trim_start_matches('/');
    #[cfg(debug_assertions)]
    {
        return asset_response_fs(trimmed);
    }
    #[cfg(not(debug_assertions))]
    {
        return asset_response_embed(trimmed);
    }
}

fn management_router() -> Router<ServiceState> {
    Router::new()
        .route("/management", get(management_handler))
        .route("/management/*path", get(management_handler))
        .route("/assets/*path", get(asset_handler))
}

fn public_api_router() -> Router<ServiceState> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/app/info", get(app_info))
        .route("/api/app/renderer-ready", post(app_renderer_ready))
        .route("/api/app/renderer-error", post(app_renderer_error))
        .route("/api/auth/session", get(auth_session))
        .route("/api/auth/login", post(auth_login))
        .route("/api/auth/logout", post(auth_logout))
}

fn protected_api_router() -> Router<ServiceState> {
    Router::new()
        .route("/api/app/status", get(app_status))
        .route("/api/app/server/start", post(app_start))
        .route("/api/app/server/stop", post(app_stop))
        .route("/api/app/clipboard-text", get(app_clipboard_text))
        .route("/api/config", get(config_get))
        .route("/api/config", put(config_save))
        .route(
            "/api/config/remote-admin-password",
            put(config_set_remote_admin_password),
        )
        .route(
            "/api/config/remote-admin-password",
            delete(config_clear_remote_admin_password),
        )
        .route("/api/config/groups/export", post(config_export_groups))
        .route(
            "/api/config/groups/export-folder",
            post(config_export_groups_folder),
        )
        .route(
            "/api/config/groups/export-clipboard",
            post(config_export_groups_clipboard),
        )
        .route(
            "/api/config/groups/export-json",
            get(config_export_groups_json),
        )
        .route("/api/config/groups/import", post(config_import_groups))
        .route(
            "/api/config/groups/import-json",
            post(config_import_groups_json),
        )
        .route(
            "/api/config/remote-rules/upload",
            post(config_remote_rules_upload),
        )
        .route(
            "/api/config/remote-rules/pull",
            post(config_remote_rules_pull),
        )
        .route("/api/logs", get(logs_list))
        .route("/api/logs", delete(logs_clear))
        .route("/api/logs/stats/summary", get(logs_stats_summary))
        .route("/api/logs/stats/rule-cards", get(logs_stats_rule_cards))
        .route("/api/logs/stats", delete(logs_stats_clear))
        .route("/api/quota/rule", get(quota_get_rule))
        .route("/api/quota/group", get(quota_get_group))
        .route("/api/quota/test-draft", post(quota_test_draft))
        .route("/api/provider/test-model", post(provider_test_model))
        .route("/api/integration/targets", get(integration_list_targets))
        .route(
            "/api/integration/pick-directory",
            post(integration_pick_directory),
        )
        .route("/api/integration/targets", post(integration_add_target))
        .route("/api/integration/targets", put(integration_update_target))
        .route(
            "/api/integration/targets",
            delete(integration_remove_target),
        )
        .route(
            "/api/integration/write-group-entry",
            post(integration_write_group_entry),
        )
        .route(
            "/api/integration/agent-config",
            get(integration_read_agent_config),
        )
        .route(
            "/api/integration/agent-config",
            put(integration_write_agent_config),
        )
        .route(
            "/api/integration/agent-config/source",
            put(integration_write_agent_config_source),
        )
}

pub(crate) fn router(service_state: ServiceState) -> Router<ServiceState> {
    public_api_router()
        .merge(protected_api_router().layer(middleware::from_fn_with_state(
            service_state.clone(),
            require_remote_admin_auth,
        )))
        .merge(management_router())
}

fn request_socket_addr(request: &Request<axum::body::Body>) -> Option<SocketAddr> {
    request
        .extensions()
        .get::<axum::extract::connect_info::ConnectInfo<SocketAddr>>()
        .map(|value| value.0)
        .or_else(|| request.extensions().get::<SocketAddr>().copied())
}

fn request_is_remote(request: &Request<axum::body::Body>) -> bool {
    is_remote_request_with_headers(request_socket_addr(request), request.headers())
}

fn resolve_remote_request(socket_addr: SocketAddr, headers: &HeaderMap) -> bool {
    is_remote_request_with_headers(Some(socket_addr), headers)
}

fn resolve_request_base_url(headers: &HeaderMap) -> Option<String> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(origin) = origin {
        return Some(origin.trim_end_matches('/').to_string());
    }

    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| *value == "http" || *value == "https")
        .unwrap_or("http");

    Some(format!("{scheme}://{}", host.trim_end_matches('/')))
}

fn set_cookie_header(response: &mut Response, cookie_value: &str) -> Result<(), ApiError> {
    let header_value = HeaderValue::from_str(cookie_value)
        .map_err(|e| ApiError::internal(format!("build Set-Cookie header failed: {e}")))?;
    response
        .headers_mut()
        .append(header::SET_COOKIE, header_value);
    Ok(())
}

async fn require_remote_admin_auth(
    State(state): State<ServiceState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Ok(shared) = require_shared_state(&state) else {
        return ApiError::internal("shared state unavailable").into_response();
    };

    if !request_is_remote(&request) || !shared.remote_admin_auth.password_configured() {
        return next.run(request).await;
    }

    match shared
        .remote_admin_auth
        .authenticate_request(request.headers())
    {
        Ok(true) => next.run(request).await,
        Ok(false) => unauthorized_remote_admin().into_response(),
        Err(message) => ApiError::internal(message).into_response(),
    }
}

async fn health() -> ApiResult<serde_json::Value> {
    Ok(Json(json!({ "ok": true })))
}

async fn auth_session(
    State(state): State<ServiceState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult<AuthSessionStatus> {
    let shared = require_shared_state(&state)?;
    let remote_request = resolve_remote_request(socket_addr, &headers);
    let authenticated = if remote_request && shared.remote_admin_auth.password_configured() {
        shared
            .remote_admin_auth
            .authenticate_request(&headers)
            .map_err(ApiError::internal)?
    } else {
        true
    };
    Ok(Json(config_service::auth_session_status(
        &shared,
        remote_request,
        authenticated,
    )))
}

#[derive(Debug, Deserialize)]
struct AuthLoginRequest {
    password: String,
}

async fn auth_login(
    State(state): State<ServiceState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<AuthLoginRequest>,
) -> Result<Response, ApiError> {
    let shared = require_shared_state(&state)?;
    let remote_request = resolve_remote_request(socket_addr, &headers);
    if !remote_request || !shared.remote_admin_auth.password_configured() {
        return Ok(Json(config_service::auth_session_status(
            &shared,
            remote_request,
            true,
        ))
        .into_response());
    }

    let valid = shared
        .remote_admin_auth
        .verify_password(&payload.password)
        .map_err(ApiError::internal)?;
    if !valid {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Invalid remote management password",
        ));
    }

    let session_token = shared
        .remote_admin_auth
        .issue_session()
        .map_err(ApiError::internal)?;
    let mut response =
        Json(config_service::auth_session_status(&shared, true, true)).into_response();
    set_cookie_header(&mut response, &build_session_cookie(&session_token))?;
    Ok(response)
}

async fn auth_logout(
    State(state): State<ServiceState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let shared = require_shared_state(&state)?;
    if let Some(session_token) = extract_cookie_value(&headers, SESSION_COOKIE_NAME) {
        shared.remote_admin_auth.revoke_session(&session_token);
    }
    let remote_request = resolve_remote_request(socket_addr, &headers);
    let mut response = Json(config_service::auth_session_status(
        &shared,
        remote_request,
        false,
    ))
    .into_response();
    set_cookie_header(&mut response, &build_clear_session_cookie())?;
    Ok(response)
}

async fn app_info(State(state): State<ServiceState>) -> ApiResult<AppInfo> {
    let shared = require_shared_state(&state)?;
    Ok(Json(shared.app_info.clone()))
}

async fn app_status(State(state): State<ServiceState>) -> ApiResult<ProxyStatus> {
    let shared = require_shared_state(&state)?;
    Ok(Json(shared.runtime.get_status()))
}

async fn app_start(State(state): State<ServiceState>) -> ApiResult<ProxyStatus> {
    if state.app_handle.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "Headless mode does not support start/stop from management UI",
        ));
    }
    let shared = require_shared_state(&state)?;
    let status = shared.runtime.start().await.map_err(ApiError::internal)?;
    Ok(Json(status))
}

async fn app_stop(State(state): State<ServiceState>) -> ApiResult<ProxyStatus> {
    if state.app_handle.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "Headless mode does not support start/stop from management UI",
        ));
    }
    let shared = require_shared_state(&state)?;
    let status = shared.runtime.stop().await.map_err(ApiError::internal)?;
    Ok(Json(status))
}

async fn app_renderer_ready(State(state): State<ServiceState>) -> ApiResult<serde_json::Value> {
    let shared = require_shared_state(&state)?;
    shared.set_renderer_ready(true);
    eprintln!("[renderer][info] event=renderer_ready window=http message=renderer boot completed");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct RendererErrorPayload {
    kind: String,
    message: String,
    stack: Option<String>,
    source: Option<String>,
}

async fn app_renderer_error(
    State(state): State<ServiceState>,
    Json(payload): Json<RendererErrorPayload>,
) -> ApiResult<serde_json::Value> {
    let _shared = require_shared_state(&state)?;
    let stack_preview = payload
        .stack
        .as_deref()
        .map(|value| value.chars().take(240).collect::<String>())
        .unwrap_or_else(|| "-".to_string());
    let source_text = payload.source.unwrap_or_else(|| "-".to_string());
    eprintln!(
        "[renderer][error] event=renderer_runtime_error window=http message=kind={} source={} message={} stack={}",
        payload.kind, source_text, payload.message, stack_preview
    );
    Ok(Json(json!({ "ok": true })))
}

async fn app_clipboard_text(State(state): State<ServiceState>) -> ApiResult<ClipboardTextResult> {
    let app = require_app_handle(&state)?;
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| ApiError::internal(format!("read clipboard failed: {e}")))?;
    Ok(Json(ClipboardTextResult { text }))
}

async fn config_get(State(state): State<ServiceState>) -> ApiResult<ProxyConfig> {
    let shared = require_shared_state(&state)?;
    Ok(Json(config_service::get_config(&shared)))
}

#[derive(Debug, Deserialize)]
struct SaveConfigRequest {
    #[serde(rename = "nextConfig")]
    next_config: Value,
}

#[derive(Debug, Deserialize)]
struct RemoteAdminPasswordRequest {
    password: String,
}

async fn config_save(
    State(state): State<ServiceState>,
    Json(payload): Json<SaveConfigRequest>,
) -> ApiResult<SaveConfigResult> {
    let shared = require_shared_state(&state)?;
    if state.app_handle.is_none() {
        let prev = shared.config_store.get();
        let next_server_port = payload
            .next_config
            .get("server")
            .and_then(|server| server.get("port"))
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok());
        if let Some(port) = next_server_port {
            if port != prev.server.port {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "validation_error",
                    "Headless mode does not allow changing server port from management UI",
                ));
            }
        }
    }

    if let Some(app) = state.app_handle.clone() {
        let saved = config_service::save_config(&shared, &app, payload.next_config)
            .await
            .map_err(ApiError::from)?;
        return Ok(Json(saved));
    }

    let prev = shared.config_store.get();
    let saved = shared
        .config_store
        .save(payload.next_config)
        .map_err(ApiError::internal)?;
    let (restarted, status) = sync_runtime_config(&shared, prev, saved.clone())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(SaveConfigResult {
        ok: true,
        config: saved,
        restarted,
        status,
    }))
}

async fn config_set_remote_admin_password(
    State(state): State<ServiceState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<RemoteAdminPasswordRequest>,
) -> Result<Response, ApiError> {
    let shared = require_shared_state(&state)?;
    let remote_request = resolve_remote_request(socket_addr, &headers);
    let status =
        config_service::set_remote_admin_password(&shared, payload.password, remote_request)
            .map_err(ApiError::from)?;
    let mut response = Json(status).into_response();
    if remote_request {
        let session_token = shared
            .remote_admin_auth
            .issue_session()
            .map_err(ApiError::internal)?;
        set_cookie_header(&mut response, &build_session_cookie(&session_token))?;
    }
    Ok(response)
}

async fn config_clear_remote_admin_password(
    State(state): State<ServiceState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let shared = require_shared_state(&state)?;
    let remote_request = resolve_remote_request(socket_addr, &headers);
    let status = config_service::clear_remote_admin_password(&shared, remote_request)
        .map_err(ApiError::from)?;
    let mut response = Json(status).into_response();
    set_cookie_header(&mut response, &build_clear_session_cookie())?;
    Ok(response)
}

async fn config_export_groups(
    State(state): State<ServiceState>,
) -> ApiResult<GroupBackupExportResult> {
    let shared = require_shared_state(&state)?;
    let app = require_app_handle(&state)?;
    let result = group_backup_service::export_groups_to_file(&shared, &app)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn config_export_groups_folder(
    State(state): State<ServiceState>,
) -> ApiResult<GroupBackupExportResult> {
    let shared = require_shared_state(&state)?;
    let app = require_app_handle(&state)?;
    let result = group_backup_service::export_groups_to_folder(&shared, &app)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn config_export_groups_clipboard(
    State(state): State<ServiceState>,
) -> ApiResult<GroupBackupExportResult> {
    let shared = require_shared_state(&state)?;
    let app = require_app_handle(&state)?;
    let result = group_backup_service::export_groups_to_clipboard(&shared, &app)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn config_export_groups_json(
    State(state): State<ServiceState>,
) -> ApiResult<GroupsExportJsonResult> {
    let shared = require_shared_state(&state)?;
    let current = shared.config_store.get();
    let payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&payload)
        .map_err(|e| ApiError::internal(format!("serialize backup failed: {e}")))?;
    let char_count = json_text.len();
    Ok(Json(GroupsExportJsonResult {
        text: json_text,
        file_name: backup_default_file_name(),
        group_count: current.groups.len(),
        char_count,
    }))
}

async fn config_import_groups(
    State(state): State<ServiceState>,
) -> ApiResult<GroupBackupImportResult> {
    let shared = require_shared_state(&state)?;
    let app = require_app_handle(&state)?;
    let result = group_backup_service::import_groups_from_file(&shared, &app)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct ImportGroupsJsonRequest {
    #[serde(rename = "jsonText")]
    json_text: String,
}

async fn config_import_groups_json(
    State(state): State<ServiceState>,
    Json(payload): Json<ImportGroupsJsonRequest>,
) -> ApiResult<GroupBackupImportResult> {
    let shared = require_shared_state(&state)?;
    let result = group_backup_service::import_groups_from_json_text(&shared, payload.json_text)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct RemoteRulesRequest {
    force: Option<bool>,
}

async fn config_remote_rules_upload(
    State(state): State<ServiceState>,
    Json(payload): Json<RemoteRulesRequest>,
) -> ApiResult<RemoteRulesUploadResult> {
    let shared = require_shared_state(&state)?;
    let app_dir = resolve_app_data_dir(&shared, state.app_handle.as_ref())?;
    let result = remote_rules_service::upload_with_dir(&shared, &app_dir, payload.force)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn config_remote_rules_pull(
    State(state): State<ServiceState>,
    Json(payload): Json<RemoteRulesRequest>,
) -> ApiResult<RemoteRulesPullResult> {
    let shared = require_shared_state(&state)?;
    let app_dir = resolve_app_data_dir(&shared, state.app_handle.as_ref())?;
    let result = remote_rules_service::pull_with_dir(&shared, &app_dir, payload.force)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct LogsQuery {
    max: Option<usize>,
}

async fn logs_list(
    State(state): State<ServiceState>,
    Query(query): Query<LogsQuery>,
) -> ApiResult<Vec<LogEntry>> {
    let shared = require_shared_state(&state)?;
    Ok(Json(shared.runtime.list_logs(query.max.unwrap_or(100))))
}

async fn logs_clear(State(state): State<ServiceState>) -> ApiResult<serde_json::Value> {
    let shared = require_shared_state(&state)?;
    shared.runtime.clear_logs();
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct LogsStatsSummaryQuery {
    hours: Option<u32>,
    #[serde(rename = "ruleKeys")]
    #[serde(default, deserialize_with = "deserialize_rule_keys")]
    rule_keys: Option<Vec<String>>,
    #[serde(rename = "ruleKey")]
    rule_key: Option<String>,
    dimension: Option<String>,
    #[serde(rename = "enableComparison")]
    enable_comparison: Option<bool>,
}

fn deserialize_rule_keys<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RuleKeys {
        One(String),
        Many(Vec<String>),
    }

    let value = Option::<RuleKeys>::deserialize(deserializer)?;
    Ok(value.map(|keys| match keys {
        RuleKeys::One(text) => text
            .split(',')
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(|item| item.to_string())
            .collect(),
        RuleKeys::Many(items) => items
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
    }))
}

async fn logs_stats_summary(
    State(state): State<ServiceState>,
    Query(query): Query<LogsStatsSummaryQuery>,
) -> ApiResult<StatsSummaryResult> {
    let shared = require_shared_state(&state)?;
    let result = shared.runtime.stats_summary(
        query.hours,
        query.rule_keys,
        query.rule_key,
        query.dimension,
        query.enable_comparison,
    );
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct LogsStatsRuleCardsQuery {
    #[serde(rename = "groupId")]
    group_id: String,
    hours: Option<u32>,
}

async fn logs_stats_rule_cards(
    State(state): State<ServiceState>,
    Query(query): Query<LogsStatsRuleCardsQuery>,
) -> ApiResult<Vec<RuleCardStatsItem>> {
    let shared = require_shared_state(&state)?;
    Ok(Json(
        shared.runtime.stats_rule_cards(query.group_id, query.hours),
    ))
}

#[derive(Debug, Deserialize)]
struct LogsStatsClearQuery {
    #[serde(rename = "beforeEpochMs")]
    before_epoch_ms: Option<i64>,
}

async fn logs_stats_clear(
    State(state): State<ServiceState>,
    Query(query): Query<LogsStatsClearQuery>,
) -> ApiResult<serde_json::Value> {
    let shared = require_shared_state(&state)?;
    if let Some(value) = query.before_epoch_ms {
        shared
            .runtime
            .clear_stats_before(value)
            .map_err(ApiError::internal)?;
    } else {
        shared.runtime.clear_stats().map_err(ApiError::internal)?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct QuotaRuleQuery {
    #[serde(rename = "groupId")]
    group_id: String,
    #[serde(rename = "ruleId")]
    rule_id: String,
}

async fn quota_get_rule(
    State(state): State<ServiceState>,
    Query(query): Query<QuotaRuleQuery>,
) -> ApiResult<RuleQuotaSnapshot> {
    let shared = require_shared_state(&state)?;
    let result = quota_service::get_rule(&shared, query.group_id, query.rule_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct QuotaGroupQuery {
    #[serde(rename = "groupId")]
    group_id: String,
}

async fn quota_get_group(
    State(state): State<ServiceState>,
    Query(query): Query<QuotaGroupQuery>,
) -> ApiResult<Vec<RuleQuotaSnapshot>> {
    let shared = require_shared_state(&state)?;
    let result = quota_service::get_group(&shared, query.group_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct QuotaTestDraftRequest {
    #[serde(rename = "groupId")]
    group_id: String,
    #[serde(rename = "ruleName")]
    rule_name: String,
    #[serde(rename = "ruleToken")]
    rule_token: String,
    #[serde(rename = "ruleApiAddress")]
    rule_api_address: String,
    #[serde(rename = "ruleDefaultModel")]
    rule_default_model: String,
    quota: RuleQuotaConfig,
}

async fn quota_test_draft(
    State(state): State<ServiceState>,
    Json(payload): Json<QuotaTestDraftRequest>,
) -> ApiResult<RuleQuotaTestResult> {
    let shared = require_shared_state(&state)?;
    let result = quota_service::test_draft(
        &shared,
        payload.group_id,
        payload.rule_name,
        payload.rule_token,
        payload.rule_api_address,
        payload.rule_default_model,
        payload.quota,
    )
    .await
    .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct ProviderTestRequest {
    #[serde(default, rename = "groupId")]
    group_id: Option<String>,
    #[serde(rename = "providerId")]
    provider_id: String,
}

async fn provider_test_model(
    State(state): State<ServiceState>,
    Json(payload): Json<ProviderTestRequest>,
) -> ApiResult<ProviderModelTestResult> {
    let shared = require_shared_state(&state)?;
    let result = provider_service::test_model(&shared, payload.group_id, payload.provider_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn integration_list_targets(
    State(state): State<ServiceState>,
) -> ApiResult<Vec<IntegrationTarget>> {
    if state.app_handle.is_none() {
        return Ok(Json(integration_service::list_default_targets()));
    }
    let shared = require_shared_state(&state)?;
    Ok(Json(integration_service::list_targets(&shared)))
}

#[derive(Debug, Deserialize)]
struct IntegrationPickRequest {
    #[serde(rename = "initialDir")]
    initial_dir: Option<String>,
    kind: Option<IntegrationClientKind>,
}

async fn integration_pick_directory(
    State(state): State<ServiceState>,
    Json(payload): Json<IntegrationPickRequest>,
) -> ApiResult<Option<String>> {
    ensure_not_headless(
        &state,
        "Headless mode does not support client integration updates from management UI",
    )?;
    let app = require_app_handle(&state)?;
    let initial = payload.initial_dir;
    let kind = payload.kind;
    let picked = crate::commands::integration_pick_directory(app, initial, kind)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(picked))
}

#[derive(Debug, Deserialize)]
struct IntegrationAddRequest {
    kind: IntegrationClientKind,
    #[serde(rename = "configDir")]
    config_dir: String,
}

async fn integration_add_target(
    State(state): State<ServiceState>,
    Json(payload): Json<IntegrationAddRequest>,
) -> ApiResult<IntegrationTarget> {
    ensure_not_headless(
        &state,
        "Headless mode does not support client integration updates from management UI",
    )?;
    let shared = require_shared_state(&state)?;
    let result = integration_service::add_target(&shared, payload.kind, payload.config_dir)
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct IntegrationUpdateRequest {
    #[serde(rename = "targetId")]
    target_id: String,
    #[serde(rename = "configDir")]
    config_dir: String,
}

async fn integration_update_target(
    State(state): State<ServiceState>,
    Json(payload): Json<IntegrationUpdateRequest>,
) -> ApiResult<IntegrationTarget> {
    ensure_not_headless(
        &state,
        "Headless mode does not support client integration updates from management UI",
    )?;
    let shared = require_shared_state(&state)?;
    let result =
        integration_service::update_target(&shared, &payload.target_id, payload.config_dir)
            .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct IntegrationRemoveQuery {
    #[serde(rename = "targetId")]
    target_id: String,
}

async fn integration_remove_target(
    State(state): State<ServiceState>,
    Query(query): Query<IntegrationRemoveQuery>,
) -> ApiResult<serde_json::Value> {
    ensure_not_headless(
        &state,
        "Headless mode does not support client integration updates from management UI",
    )?;
    let shared = require_shared_state(&state)?;
    let removed =
        integration_service::remove_target(&shared, &query.target_id).map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true, "removed": removed })))
}

#[derive(Debug, Deserialize)]
struct IntegrationWriteEntryRequest {
    #[serde(rename = "groupId")]
    group_id: String,
    #[serde(rename = "targetIds")]
    target_ids: Vec<String>,
}

async fn integration_write_group_entry(
    State(state): State<ServiceState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<IntegrationWriteEntryRequest>,
) -> ApiResult<IntegrationWriteResult> {
    let shared = require_shared_state(&state)?;
    let targets = if state.app_handle.is_none() {
        integration_service::list_default_targets()
    } else {
        integration_service::list_targets(&shared)
    };
    let base_url_override = if resolve_remote_request(socket_addr, &headers) {
        resolve_request_base_url(&headers)
    } else {
        None
    };
    let result = integration_service::write_group_entry_with_targets_and_base_url(
        &shared,
        &payload.group_id,
        targets,
        payload.target_ids,
        base_url_override.as_deref(),
    )
    .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct IntegrationReadQuery {
    #[serde(rename = "targetId")]
    target_id: String,
}

async fn integration_read_agent_config(
    State(state): State<ServiceState>,
    Query(query): Query<IntegrationReadQuery>,
) -> ApiResult<AgentConfigFile> {
    let shared = require_shared_state(&state)?;
    let targets = if state.app_handle.is_none() {
        integration_service::list_default_targets()
    } else {
        integration_service::list_targets(&shared)
    };
    let result = integration_service::read_agent_config_with_targets(targets, &query.target_id)
        .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct IntegrationWriteConfigRequest {
    #[serde(rename = "targetId")]
    target_id: String,
    config: AgentConfig,
}

async fn integration_write_agent_config(
    State(state): State<ServiceState>,
    Json(payload): Json<IntegrationWriteConfigRequest>,
) -> ApiResult<WriteAgentConfigResult> {
    let shared = require_shared_state(&state)?;
    let targets = if state.app_handle.is_none() {
        integration_service::list_default_targets()
    } else {
        integration_service::list_targets(&shared)
    };
    let result = integration_service::write_agent_config_with_targets(
        if state.app_handle.is_none() {
            None
        } else {
            Some(&shared)
        },
        targets,
        &payload.target_id,
        payload.config,
    )
    .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct IntegrationWriteSourceRequest {
    #[serde(rename = "targetId")]
    target_id: String,
    content: String,
    #[serde(rename = "sourceId")]
    source_id: Option<String>,
}

async fn integration_write_agent_config_source(
    State(state): State<ServiceState>,
    Json(payload): Json<IntegrationWriteSourceRequest>,
) -> ApiResult<WriteAgentConfigResult> {
    let shared = require_shared_state(&state)?;
    let targets = if state.app_handle.is_none() {
        integration_service::list_default_targets()
    } else {
        integration_service::list_targets(&shared)
    };
    let result = integration_service::write_agent_config_source_with_targets(
        if state.app_handle.is_none() {
            None
        } else {
            Some(&shared)
        },
        targets,
        &payload.target_id,
        &payload.content,
        payload.source_id.as_deref(),
    )
    .map_err(ApiError::from)?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::resolve_request_base_url;
    use axum::http::{header, HeaderMap, HeaderValue};

    #[test]
    fn resolve_request_base_url_prefers_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://remote-aor.test:17777"),
        );
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("ignored.example:18888"),
        );

        assert_eq!(
            resolve_request_base_url(&headers).as_deref(),
            Some("https://remote-aor.test:17777")
        );
    }

    #[test]
    fn resolve_request_base_url_uses_forwarded_host_and_proto() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("remote-aor.test:17777"),
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));

        assert_eq!(
            resolve_request_base_url(&headers).as_deref(),
            Some("https://remote-aor.test:17777")
        );
    }

    #[test]
    fn resolve_request_base_url_falls_back_to_host_header() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:8899"));

        assert_eq!(
            resolve_request_base_url(&headers).as_deref(),
            Some("http://localhost:8899")
        );
    }
}
