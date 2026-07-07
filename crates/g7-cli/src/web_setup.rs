//! Web setup controller for `g7inst setup`.
//!
//! This module owns the human-facing setup UX. It runs a short-lived local web
//! controller, serves the bundled HTML/CSS/JS assets, and delegates all install
//! policy to `g7_core::commands::plan` and mutating actions to core commands.
//! The controller must not expose arbitrary shell execution or invent install
//! defaults that do not exist in `plan.rs`.

use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use g7_core::commands::{DoctorCheckStatus, doctor, install, plan, reset, rollback, status};
use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::STATE_PATH;
use getrandom::fill as fill_random;
use miette::{IntoDiagnostic, Result, miette};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::trace::TraceLayer;

pub const DEFAULT_BIND: &str = "127.0.0.1:7717";

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const APP_CSS: &str = include_str!("../../../web/dist/app.css");
const ASSET_VERSION: &str = match option_env!("G7_ASSET_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
const REPORT_PATH: &str = "/var/log/g7-installer/report.json";
const CONFIG_PATH: &str = "/etc/g7-installer/config.toml";
const LOCAL_HOSTS_PATH: &str = "/etc/g7-installer/local-hosts.txt";
const ROLLBACK_PATH: &str = "/var/lib/g7-installer/rollback.json";
const SESSION_COOKIE: &str = "g7inst_session";
const CSRF_HEADER: &str = "x-g7-csrf";
const SESSION_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSetupConfig {
    pub domain: Option<String>,
    pub local_test: bool,
    pub bind: String,
    pub allow_remote: bool,
}

#[derive(Debug, Clone)]
struct WebState {
    access_token: String,
    domain: Option<String>,
    local_test: bool,
    events: broadcast::Sender<WebEvent>,
    install_running: Arc<AtomicBool>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    allowed_client_ip: Arc<Mutex<Option<IpAddr>>>,
}

#[derive(Debug, Clone)]
struct Session {
    csrf_token: String,
    authenticated: bool,
    username: Option<String>,
    client_ip: IpAddr,
    expires_at: Instant,
}

#[derive(Debug, Clone, Serialize)]
struct WebEvent {
    event_type: &'static str,
    message: String,
    stage: Option<&'static str>,
    status: Option<&'static str>,
    operation: Option<&'static str>,
    percent: Option<u8>,
}

#[derive(Debug, Serialize)]
struct BootstrapPayload {
    domain: Option<String>,
    local_test: bool,
    auth: BootstrapAuth,
    csrf_token: String,
}

#[derive(Debug, Serialize)]
struct BootstrapAuth {
    mode: &'static str,
    status: &'static str,
    username: Option<String>,
    authenticated: bool,
    client_ip: String,
}

#[derive(Debug, Deserialize)]
struct SetupRequest {
    domain: String,
    #[serde(default)]
    local_test: bool,
    web_server: String,
    php_version: String,
    database: String,
    database_version: String,
    app_package: String,
    site_user: String,
    web_root_mode: String,
    web_root: Option<String>,
    www_mode: String,
    redis: String,
    mail_mode: String,
    smtp_host: Option<String>,
    smtp_port: u16,
    smtp_from: Option<String>,
    smtp_encryption: String,
    security_profile: String,
    ssh_policy: String,
    rollback: bool,
    preserve_config: bool,
    dns_check: bool,
}

#[derive(Debug, Deserialize)]
struct ResetRequest {
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct RollbackRequest {
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
    hint: Option<String>,
    details: Vec<String>,
    retryable: bool,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
    hint: Option<String>,
    details: Vec<String>,
    retryable: bool,
}

#[derive(Debug, Serialize)]
struct DoctorApiReport {
    install_allowed: bool,
    checks: Vec<DoctorApiCheck>,
}

#[derive(Debug, Serialize)]
struct DoctorApiCheck {
    name: &'static str,
    status: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct PlanApiReport {
    text: String,
    domain: String,
    deployment_mode: String,
    app_profile: String,
    app_profile_label: &'static str,
    app_document_root: String,
    web_server: String,
    php_version: String,
    database: String,
    database_version: String,
    app_package: String,
    site_user: String,
    web_root: String,
    packages: Vec<NameDescription>,
    files: Vec<FilePlan>,
    services: Vec<ServicePlan>,
    ports: Vec<PortPlan>,
    security_checks: Vec<SecurityCheckPlan>,
    app_requirements: Vec<RequirementPlan>,
    app_followup_steps: Vec<FollowupStepPlan>,
    provisioning: Vec<ProvisioningSectionPlan>,
    stop_conditions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct NameDescription {
    name: String,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct FilePlan {
    path: String,
    action: &'static str,
}

#[derive(Debug, Serialize)]
struct ServicePlan {
    name: String,
    action: &'static str,
}

#[derive(Debug, Serialize)]
struct PortPlan {
    port: u16,
    protocol: &'static str,
    purpose: &'static str,
}

#[derive(Debug, Serialize)]
struct SecurityCheckPlan {
    name: &'static str,
    level: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct RequirementPlan {
    name: String,
    status: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct FollowupStepPlan {
    name: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct ProvisioningSectionPlan {
    name: &'static str,
    title: &'static str,
    summary: String,
    settings: Vec<ProvisioningSettingPlan>,
}

#[derive(Debug, Serialize)]
struct ProvisioningSettingPlan {
    key: &'static str,
    value: String,
}

#[derive(Debug, Serialize)]
struct InstallApiReport {
    domain: String,
    deployment_mode: String,
    app_profile: String,
    app_profile_label: &'static str,
    app_document_root: String,
    web_server: String,
    php_version: String,
    database: String,
    database_version: String,
    app_package: String,
    site_user: String,
    web_root: String,
    mail_mode: String,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    smtp_from: Option<String>,
    smtp_encryption: Option<String>,
    dns_check: bool,
    phase: String,
    state_path: String,
    owned_files_path: String,
    completed_steps: Vec<String>,
    safety_checks: Vec<InstallApiCheck>,
    preinstall_package_checks: Vec<InstallApiCheck>,
    package_checks: Vec<InstallApiCheck>,
    service_checks: Vec<InstallApiCheck>,
    port_checks: Vec<InstallApiCheck>,
    network_checks: Vec<InstallApiCheck>,
    mail_checks: Vec<InstallApiCheck>,
    certbot_checks: Vec<InstallApiCheck>,
    vhost_checks: Vec<InstallApiCheck>,
    app_requirements: Vec<InstallApiCheck>,
}

#[derive(Debug, Serialize)]
struct InstallApiCheck {
    name: String,
    status: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct ResetApiReport {
    dry_run: bool,
    removed: Vec<String>,
    missing: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RollbackApiReport {
    dry_run: bool,
    phase: String,
    package_actions: Vec<RollbackApiAction>,
    service_actions: Vec<RollbackApiAction>,
    metadata_reset: ResetApiReport,
}

#[derive(Debug, Serialize)]
struct RollbackApiAction {
    name: String,
    status: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct StatusApiReport {
    installed: bool,
    components: Vec<ComponentApiStatus>,
}

#[derive(Debug, Serialize)]
struct RecoveryApiStatus {
    can_reset: bool,
    can_rollback: bool,
    recommended_action: &'static str,
    message: String,
    metadata_paths: Vec<String>,
    rollback_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ComponentApiStatus {
    name: &'static str,
    state: &'static str,
}

#[derive(Debug, Serialize)]
struct ReportApiPayload {
    exists: bool,
    path: &'static str,
    content: String,
}

pub async fn run(config: WebSetupConfig) -> Result<()> {
    ensure_setup_runs_as_root()?;

    let bind = parse_bind(&config.bind)?;
    ensure_remote_binding_is_explicit(bind, config.allow_remote)?;

    let state = WebState {
        access_token: secure_token()?,
        domain: config.domain,
        local_test: config.local_test,
        events: broadcast::channel(128).0,
        install_running: Arc::new(AtomicBool::new(false)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        allowed_client_ip: Arc::new(Mutex::new(None)),
    };

    let listener = TcpListener::bind(bind)
        .await
        .map_err(|err| miette!("failed to bind web setup controller on {bind}: {err}"))?;
    let local_addr = listener.local_addr().into_diagnostic()?;
    print_startup(local_addr, &state.access_token);

    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/auth/logout", post(api_logout))
        .route("/api/events", get(api_events))
        .route("/api/doctor", get(api_doctor))
        .route("/api/plan", post(api_plan))
        .route("/api/install/prepare", post(api_install_prepare))
        .route("/api/reset", post(api_reset))
        .route("/api/rollback", post(api_rollback))
        .route("/api/status", get(api_status))
        .route("/api/recovery", get(api_recovery))
        .route("/api/report", get(api_report))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(|err| miette!("web setup controller failed: {err}"))
}

fn parse_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .map_err(|err| miette!("invalid --bind value `{bind}`: {err}"))
}

fn ensure_remote_binding_is_explicit(bind: SocketAddr, allow_remote: bool) -> Result<()> {
    if is_loopback(bind.ip()) || allow_remote {
        return Ok(());
    }

    Err(miette!(
        "--allow-remote is required when binding setup controller to {bind}"
    ))
}

fn ensure_setup_runs_as_root() -> Result<()> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .into_diagnostic()
        .map_err(|err| miette!("failed to check current user id: {err}"))?;

    if !output.status.success() {
        return Err(miette!(
            "failed to check current user id: id -u exited with status {}",
            output.status
        ));
    }

    let uid = String::from_utf8_lossy(&output.stdout);
    if uid.trim() == "0" {
        return Ok(());
    }

    Err(miette!(
        "g7inst setup must be started with sudo/root.\nRun: sudo g7inst setup --domain example.com\nServer account password input is not used in the web UI."
    ))
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback(),
    }
}

fn secure_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    fill_random(&mut bytes).map_err(|err| miette!("failed to generate setup token: {err}"))?;
    Ok(hex_encode(&bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }

    out
}

fn print_startup(addr: SocketAddr, token: &str) {
    let browser_addr = browser_addr_for(addr);
    let port = addr.port();

    println!("G7inst Web Controller");
    println!("Open: http://{browser_addr}/?token={token}");
    println!("Access lock: first valid token client IP only");
    println!("Remote access:");
    println!("ssh -L {port}:127.0.0.1:{port} ubuntu@SERVER_IP");
    if !is_loopback(addr.ip()) {
        println!("Remote bind is enabled; keep this port firewalled.");
    }
    println!("Server password: not required; this controller already runs with sudo/root.");
    println!("Stop: Ctrl+C");
}

fn browser_addr_for(addr: SocketAddr) -> SocketAddr {
    if addr.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
    } else {
        addr
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn index(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mut response = Html(index_html()).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, max-age=0"),
    );
    let client_ip = peer.ip();

    if query
        .get("token")
        .is_some_and(|token| secure_eq(token, &state.access_token))
    {
        match create_session(&state, client_ip) {
            Ok(session_id) => {
                if let Ok(value) = HeaderValue::from_str(&session_cookie(&session_id)) {
                    response.headers_mut().insert(header::SET_COOKIE, value);
                }
            }
            Err(error) => return error.into_response(),
        }
    } else if let Err(error) = require_allowed_client_ip(&state, client_ip) {
        return error.into_response();
    }

    response
}

fn index_html() -> String {
    INDEX_HTML.replace("__G7INST_ASSET_VERSION__", ASSET_VERSION)
}

async fn app_js(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        APP_JS,
    ))
}

async fn app_css(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        APP_CSS,
    ))
}

async fn bootstrap(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers, peer.ip())?;
    let payload = BootstrapPayload {
        domain: state.domain,
        local_test: state.local_test,
        auth: BootstrapAuth {
            mode: "sudo-token",
            status: if session.authenticated {
                "authenticated"
            } else {
                "token-accepted"
            },
            username: session.username.clone(),
            authenticated: session.authenticated,
            client_ip: session.client_ip.to_string(),
        },
        csrf_token: session.csrf_token,
    };

    Ok((StatusCode::OK, Json(payload)))
}

async fn api_logout(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;
    let session_id = require_session_id(&headers)?;
    remove_session(&state, &session_id)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn api_events(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Err(error) = require_session(&state, &headers, peer.ip()) {
        return error.into_response();
    }

    ws.on_upgrade(move |socket| event_socket(socket, state.events.subscribe()))
        .into_response()
}

async fn event_socket(mut socket: WebSocket, mut events: broadcast::Receiver<WebEvent>) {
    let connected = WebEvent {
        event_type: "log",
        message: "event stream connected".to_string(),
        stage: None,
        status: None,
        operation: None,
        percent: None,
    };

    if send_event(&mut socket, &connected).await.is_err() {
        return;
    }

    while let Ok(event) = events.recv().await {
        if send_event(&mut socket, &event).await.is_err() {
            break;
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: &WebEvent) -> std::result::Result<(), ()> {
    let Ok(text) = serde_json::to_string(event) else {
        return Err(());
    };

    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}

async fn api_doctor(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_session(&state, &headers, peer.ip())?;
    emit_log(&state, "running server check");
    let report = doctor_to_api(doctor::run());
    emit_log(
        &state,
        format!(
            "server check completed: install_allowed={}",
            report.install_allowed
        ),
    );

    Ok(Json(report))
}

async fn api_plan(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;
    emit_log(&state, "building install plan");
    let domain = request.domain.clone();
    let database_version = normalize_database_version(&request.database_version);
    let options = options_from_request(request);
    let install_plan = match plan::build_with_options(domain, options) {
        Ok(install_plan) => install_plan,
        Err(error) => {
            emit_log(&state, format!("plan failed: {error}"));
            return Err(ApiError::bad_request(error)
                .with_hint("설치 옵션 값을 확인한 뒤 다시 계획을 생성하세요."));
        }
    };
    emit_log(&state, "install plan ready");

    Ok(Json(plan_to_api(install_plan, database_version)))
}

async fn api_install_prepare(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;

    if state.install_running.swap(true, Ordering::SeqCst) {
        emit_log(&state, "install request rejected: already running");
        return Err(ApiError::conflict("install is already running"));
    }

    emit_progress(&state, "install", 5, "install progress: starting preflight");
    emit_stage(&state, "preflight", "진행", "preflight started");
    let domain = request.domain.clone();
    let database_version = normalize_database_version(&request.database_version);
    let options = options_from_request(request);
    emit_progress(
        &state,
        "install",
        15,
        "install progress: running server install",
    );
    let result = install::run(domain, options);
    state.install_running.store(false, Ordering::SeqCst);

    match result {
        Ok(report) => {
            emit_stage(&state, "preflight", "성공", "preflight passed");
            emit_progress(&state, "install", 25, "install progress: preflight passed");
            emit_stage(&state, "packages", "성공", "packages installed");
            emit_progress(
                &state,
                "install",
                45,
                "install progress: packages installed",
            );
            emit_stage(&state, "config", "성공", "configuration prepared");
            emit_progress(
                &state,
                "install",
                60,
                "install progress: configuration prepared",
            );
            emit_stage(&state, "services", "성공", "services enabled");
            emit_progress(&state, "install", 75, "install progress: services verified");
            emit_stage(&state, "ports", "성공", "ports verified");
            emit_progress(&state, "install", 88, "install progress: ports verified");
            emit_stage(&state, "http", "성공", "HTTP vhost verification completed");
            emit_stage(&state, "report", "성공", "problem report prepared");
            emit_progress(&state, "install", 100, "install progress: report ready");
            emit_log(&state, "server install completed");
            Ok(Json(install_to_api(report, database_version)))
        }
        Err(error) => {
            let details = failed_doctor_details(doctor::run());
            emit_progress(&state, "install", 100, "install progress: failed");
            emit_stage(
                &state,
                "packages",
                "실패",
                format!("install failed: {error}"),
            );
            Err(ApiError::bad_request(error)
                .with_hint(
                    "리포트의 실패 항목을 확인하세요. 패키지 버전 문제면 PHP 8.3 같은 Ubuntu 기본 패키지 조합으로 다시 시도하세요.",
                )
                .with_details(details))
        }
    }
}

async fn api_reset(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<ResetRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;

    if state.install_running.load(Ordering::SeqCst) {
        return Err(ApiError::conflict(
            "reset is blocked while install is running",
        ));
    }

    emit_log(&state, "running reset");
    emit_progress(
        &state,
        "reset",
        10,
        "reset progress: starting metadata cleanup",
    );
    let report = match reset::run(true, request.dry_run) {
        Ok(report) => report,
        Err(error) => {
            emit_progress(&state, "reset", 100, "reset progress: failed");
            emit_log(&state, format!("reset failed: {error}"));
            return Err(ApiError::bad_request(error)
                .with_hint("root 권한과 installer owned-files 상태를 확인하세요."));
        }
    };
    emit_progress(
        &state,
        "reset",
        100,
        "reset progress: metadata cleanup completed",
    );
    emit_log(&state, "reset completed");

    Ok(Json(ResetApiReport {
        dry_run: report.dry_run,
        removed: report.removed,
        missing: report.missing,
    }))
}

async fn api_rollback(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<RollbackRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;

    if state.install_running.swap(true, Ordering::SeqCst) {
        return Err(ApiError::conflict(
            "rollback is blocked while another install action is running",
        ));
    }

    emit_log(&state, "running package rollback");
    emit_progress(
        &state,
        "rollback",
        10,
        "rollback progress: starting rollback",
    );
    let report = rollback::run(true, request.dry_run);
    state.install_running.store(false, Ordering::SeqCst);

    let report = match report {
        Ok(report) => report,
        Err(error) => {
            emit_progress(&state, "rollback", 100, "rollback progress: failed");
            emit_log(&state, format!("rollback failed: {error}"));
            return Err(ApiError::bad_request(error).with_hint(
                "운영 웹루트가 비어 있고, 설치 직후 패키지 기준 정보가 남아 있는지 확인하세요.",
            ));
        }
    };
    emit_progress(
        &state,
        "rollback",
        100,
        "rollback progress: rollback completed",
    );
    emit_log(&state, "package rollback completed");

    Ok(Json(rollback_to_api(report)))
}

async fn api_status(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers, peer.ip())?;
    let current = status::read();

    Ok(Json(StatusApiReport {
        installed: current.installed,
        components: current
            .components
            .into_iter()
            .map(|component| ComponentApiStatus {
                name: component.name,
                state: component.state,
            })
            .collect(),
    }))
}

async fn api_recovery(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers, peer.ip())?;

    Ok(Json(recovery_status()))
}

async fn api_report(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers, peer.ip())?;

    match fs::read_to_string(REPORT_PATH) {
        Ok(content) => Ok(Json(ReportApiPayload {
            exists: true,
            path: REPORT_PATH,
            content,
        })),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Json(ReportApiPayload {
            exists: false,
            path: REPORT_PATH,
            content: "report file does not exist yet".to_string(),
        })),
        Err(err) => Ok(Json(ReportApiPayload {
            exists: false,
            path: REPORT_PATH,
            content: format!("failed to read report: {err}"),
        })),
    }
}

fn recovery_status() -> RecoveryApiStatus {
    let metadata_paths = installer_metadata_paths()
        .into_iter()
        .filter(|path| fs::metadata(path).is_ok())
        .map(str::to_string)
        .collect::<Vec<_>>();

    let rollback_check = rollback::run(true, true);
    let (can_rollback, rollback_reason) = match rollback_check {
        Ok(_) => (true, None),
        Err(error) => (false, Some(error.to_string())),
    };

    let has_installer_metadata = !metadata_paths.is_empty();
    let can_reset = has_installer_metadata && !can_rollback;
    let recommended_action = if can_rollback {
        "rollback"
    } else if can_reset {
        "reset"
    } else {
        "manual"
    };
    let message = match recommended_action {
        "rollback" => {
            "설치 직후 패키지 되돌리기가 가능합니다. 설치기가 새로 넣은 패키지만 제거하고 메타데이터를 정리합니다."
        }
        "reset" => {
            "설치기 메타데이터만 정리할 수 있습니다. apt 패키지나 기존 웹서비스는 제거하지 않습니다."
        }
        _ => {
            "설치기 소유 흔적이 확인되지 않았습니다. 기존 운영 서버일 수 있으므로 자동 초기화 버튼을 제공하지 않습니다."
        }
    }
    .to_string();

    RecoveryApiStatus {
        can_reset,
        can_rollback,
        recommended_action,
        message,
        metadata_paths,
        rollback_reason,
    }
}

fn installer_metadata_paths() -> [&'static str; 6] {
    [
        STATE_PATH,
        OWNED_FILES_PATH,
        REPORT_PATH,
        CONFIG_PATH,
        LOCAL_HOSTS_PATH,
        ROLLBACK_PATH,
    ]
}

fn options_from_request(request: SetupRequest) -> plan::PlanOptions {
    crate::plan_options(
        request.local_test,
        request.app_package,
        request.web_server,
        request.php_version,
        request.database,
        request.site_user,
        request.web_root_mode,
        request.web_root.filter(|value| !value.trim().is_empty()),
        request.www_mode,
        request.redis,
        request.mail_mode,
        request.smtp_host.filter(|value| !value.trim().is_empty()),
        request.smtp_port,
        request.smtp_from.filter(|value| !value.trim().is_empty()),
        request.smtp_encryption,
        request.security_profile,
        request.ssh_policy,
        request.rollback,
        request.preserve_config,
        request.dns_check,
    )
}

fn doctor_to_api(report: doctor::DoctorReport) -> DoctorApiReport {
    DoctorApiReport {
        install_allowed: report.install_allowed,
        checks: report
            .checks
            .into_iter()
            .map(|check| DoctorApiCheck {
                name: check.name,
                status: doctor_status_label(check.status),
                message: check.message,
            })
            .collect(),
    }
}

fn doctor_status_label(status: DoctorCheckStatus) -> &'static str {
    match status {
        DoctorCheckStatus::Pass => "pass",
        DoctorCheckStatus::Warn => "warn",
        DoctorCheckStatus::Fail => "fail",
        DoctorCheckStatus::Pending => "pending",
    }
}

fn failed_doctor_details(report: doctor::DoctorReport) -> Vec<String> {
    report
        .checks
        .into_iter()
        .filter(|check| {
            matches!(
                check.status,
                DoctorCheckStatus::Fail | DoctorCheckStatus::Pending
            )
        })
        .map(|check| {
            format!(
                "[{}] {} - {}",
                doctor_status_label(check.status),
                check.name,
                check.message
            )
        })
        .collect()
}

fn plan_to_api(install_plan: plan::InstallPlan, database_version: String) -> PlanApiReport {
    let text = crate::format_plan(&install_plan);

    PlanApiReport {
        text,
        domain: install_plan.domain,
        deployment_mode: install_plan.deployment_mode,
        app_profile: install_plan.app_profile.clone(),
        app_profile_label: install_plan.app_profile_label,
        app_document_root: install_plan.app_document_root.clone(),
        web_server: install_plan.web_server,
        php_version: install_plan.php_version,
        database: install_plan.database_engine,
        database_version,
        app_package: install_plan.app_profile,
        site_user: install_plan.site_user,
        web_root: install_plan.web_root,
        packages: install_plan
            .packages
            .into_iter()
            .map(|package| NameDescription {
                name: package.name,
                description: package.description,
            })
            .collect(),
        files: install_plan
            .files
            .into_iter()
            .map(|file| FilePlan {
                path: file.path,
                action: file.action,
            })
            .collect(),
        services: install_plan
            .services
            .into_iter()
            .map(|service| ServicePlan {
                name: service.name,
                action: service.action,
            })
            .collect(),
        ports: install_plan
            .ports
            .into_iter()
            .map(|port| PortPlan {
                port: port.port,
                protocol: port.protocol,
                purpose: port.purpose,
            })
            .collect(),
        security_checks: install_plan
            .security_checks
            .into_iter()
            .map(|check| SecurityCheckPlan {
                name: check.name,
                level: check.level,
                description: check.description,
            })
            .collect(),
        app_requirements: install_plan
            .app_requirements
            .into_iter()
            .map(|requirement| RequirementPlan {
                name: requirement.name,
                status: requirement.status,
                message: requirement.message,
            })
            .collect(),
        app_followup_steps: install_plan
            .app_followup_steps
            .into_iter()
            .map(|step| FollowupStepPlan {
                name: step.name,
                description: step.description,
            })
            .collect(),
        provisioning: install_plan
            .provisioning
            .into_iter()
            .map(|section| ProvisioningSectionPlan {
                name: section.name,
                title: section.title,
                summary: section.summary,
                settings: section
                    .settings
                    .into_iter()
                    .map(|setting| ProvisioningSettingPlan {
                        key: setting.key,
                        value: setting.value,
                    })
                    .collect(),
            })
            .collect(),
        stop_conditions: install_plan
            .stop_conditions
            .into_iter()
            .map(|condition| condition.reason)
            .collect(),
    }
}

fn install_to_api(report: install::InstallReport, database_version: String) -> InstallApiReport {
    InstallApiReport {
        domain: report.domain,
        deployment_mode: report.deployment_mode,
        app_profile: report.app_profile.clone(),
        app_profile_label: report.app_profile_label,
        app_document_root: report.app_document_root.clone(),
        web_server: report.web_server,
        php_version: report.php_version,
        database: report.database_engine,
        database_version,
        app_package: report.app_profile,
        site_user: report.site_user,
        web_root: report.web_root,
        mail_mode: report.mail_mode,
        smtp_host: report.smtp_host,
        smtp_port: report.smtp_port,
        smtp_from: report.smtp_from,
        smtp_encryption: report.smtp_encryption,
        dns_check: report.dns_check,
        phase: report.phase,
        state_path: report.state_path.display().to_string(),
        owned_files_path: report.owned_files_path.display().to_string(),
        completed_steps: report.completed_steps,
        safety_checks: install_checks_to_api(report.safety_checks),
        preinstall_package_checks: install_checks_to_api(report.preinstall_package_checks),
        package_checks: install_checks_to_api(report.package_checks),
        service_checks: install_checks_to_api(report.service_checks),
        port_checks: install_checks_to_api(report.port_checks),
        network_checks: install_checks_to_api(report.network_checks),
        mail_checks: install_checks_to_api(report.mail_checks),
        certbot_checks: install_checks_to_api(report.certbot_checks),
        vhost_checks: install_checks_to_api(report.vhost_checks),
        app_requirements: install_checks_to_api(report.app_requirements),
    }
}

fn normalize_database_version(value: &str) -> String {
    match value {
        "mysql-8.0" | "mysql-8.4" => value.to_string(),
        _ => "apt-default".to_string(),
    }
}

fn install_checks_to_api(checks: Vec<install::InstallCheck>) -> Vec<InstallApiCheck> {
    checks
        .into_iter()
        .map(|check| InstallApiCheck {
            name: check.name,
            status: check.status,
            message: check.message,
        })
        .collect()
}

fn rollback_to_api(report: rollback::RollbackReport) -> RollbackApiReport {
    RollbackApiReport {
        dry_run: report.dry_run,
        phase: report.phase,
        package_actions: rollback_actions_to_api(report.package_actions),
        service_actions: rollback_actions_to_api(report.service_actions),
        metadata_reset: ResetApiReport {
            dry_run: report.metadata_reset.dry_run,
            removed: report.metadata_reset.removed,
            missing: report.metadata_reset.missing,
        },
    }
}

fn rollback_actions_to_api(actions: Vec<rollback::RollbackAction>) -> Vec<RollbackApiAction> {
    actions
        .into_iter()
        .map(|action| RollbackApiAction {
            name: action.name,
            status: action.status,
            message: action.message,
        })
        .collect()
}

fn create_session(state: &WebState, client_ip: IpAddr) -> std::result::Result<String, ApiError> {
    lock_client_ip(state, client_ip)?;
    let session_id = secure_token().map_err(ApiError::bad_request)?;
    let csrf_token = secure_token().map_err(ApiError::bad_request)?;
    let session = Session {
        csrf_token,
        authenticated: true,
        username: Some("root".to_string()),
        client_ip,
        expires_at: Instant::now() + SESSION_TTL,
    };

    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;
    sessions.insert(session_id.clone(), session);

    Ok(session_id)
}

fn lock_client_ip(state: &WebState, client_ip: IpAddr) -> std::result::Result<(), ApiError> {
    let mut allowed = state
        .allowed_client_ip
        .lock()
        .map_err(|_| ApiError::bad_request("client IP lock is unavailable"))?;

    match *allowed {
        Some(allowed_ip) if allowed_ip == client_ip => Ok(()),
        Some(allowed_ip) => Err(client_ip_forbidden(allowed_ip, client_ip)),
        None => {
            *allowed = Some(client_ip);
            emit_log(
                state,
                format!("setup access locked to client IP: {client_ip}"),
            );
            Ok(())
        }
    }
}

fn require_allowed_client_ip(
    state: &WebState,
    client_ip: IpAddr,
) -> std::result::Result<(), ApiError> {
    let allowed = state
        .allowed_client_ip
        .lock()
        .map_err(|_| ApiError::bad_request("client IP lock is unavailable"))?;

    match *allowed {
        Some(allowed_ip) if allowed_ip != client_ip => {
            Err(client_ip_forbidden(allowed_ip, client_ip))
        }
        _ => Ok(()),
    }
}

fn client_ip_forbidden(allowed_ip: IpAddr, client_ip: IpAddr) -> ApiError {
    ApiError::forbidden("setup controller is locked to the first valid token client IP")
        .with_hint(
            "터미널의 token URL을 처음 연 같은 SSH 터널 또는 같은 클라이언트에서 접속하세요.",
        )
        .with_details(vec![
            format!("allowed_client_ip: {allowed_ip}"),
            format!("request_client_ip: {client_ip}"),
        ])
}

fn require_session_id(headers: &HeaderMap) -> std::result::Result<String, ApiError> {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing setup session cookie"))?;

    cookie
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == SESSION_COOKIE).then(|| value.to_string()))
        .ok_or_else(|| ApiError::unauthorized("missing setup session cookie"))
}

fn require_session(
    state: &WebState,
    headers: &HeaderMap,
    client_ip: IpAddr,
) -> std::result::Result<Session, ApiError> {
    require_allowed_client_ip(state, client_ip)?;
    let session_id = require_session_id(headers)?;
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;

    let now = Instant::now();
    sessions.retain(|_, session| session.expires_at > now);

    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| ApiError::unauthorized("setup session expired or invalid"))?;
    if session.client_ip != client_ip {
        return Err(client_ip_forbidden(session.client_ip, client_ip));
    }
    session.expires_at = now + SESSION_TTL;

    Ok(session.clone())
}

fn require_authenticated_session(
    state: &WebState,
    headers: &HeaderMap,
    client_ip: IpAddr,
) -> std::result::Result<Session, ApiError> {
    let session = require_session(state, headers, client_ip)?;
    if session.authenticated {
        Ok(session)
    } else {
        Err(ApiError::unauthorized("setup token session is required"))
    }
}

fn require_csrf(headers: &HeaderMap, session: &Session) -> std::result::Result<(), ApiError> {
    let token = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::forbidden("missing CSRF token"))?;

    if secure_eq(token, &session.csrf_token) {
        Ok(())
    } else {
        Err(ApiError::forbidden("invalid CSRF token"))
    }
}

fn remove_session(state: &WebState, session_id: &str) -> std::result::Result<(), ApiError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;
    sessions.remove(session_id);

    Ok(())
}

fn session_cookie(session_id: &str) -> String {
    format!("{SESSION_COOKIE}={session_id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=1800")
}

fn secure_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut diff = left.len() ^ right.len();

    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }

    diff == 0
}

impl ApiError {
    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
            hint: None,
            details: Vec::new(),
            retryable: true,
        }
    }

    fn unauthorized(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: error.into(),
            hint: Some(
                "터미널에 출력된 token URL로 다시 접속하세요. 서버 비밀번호 입력은 사용하지 않습니다."
                    .to_string(),
            ),
            details: Vec::new(),
            retryable: true,
        }
    }

    fn forbidden(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: error.into(),
            hint: Some(
                "권한 또는 접속 방식을 확인하세요. 원격 VPS는 SSH 터널 접속을 권장합니다."
                    .to_string(),
            ),
            details: Vec::new(),
            retryable: true,
        }
    }

    fn conflict(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: error.into(),
            hint: Some("현재 작업이 끝난 뒤 다시 시도하세요.".to_string()),
            details: Vec::new(),
            retryable: true,
        }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
                hint: self.hint,
                details: self.details,
                retryable: self.retryable,
            }),
        )
            .into_response()
    }
}

fn emit_log(state: &WebState, message: impl Into<String>) {
    let _ = state.events.send(WebEvent {
        event_type: "log",
        message: message.into(),
        stage: None,
        status: None,
        operation: None,
        percent: None,
    });
}

fn emit_stage(
    state: &WebState,
    stage: &'static str,
    status: &'static str,
    message: impl Into<String>,
) {
    let _ = state.events.send(WebEvent {
        event_type: "stage",
        message: message.into(),
        stage: Some(stage),
        status: Some(status),
        operation: None,
        percent: None,
    });
}

fn emit_progress(
    state: &WebState,
    operation: &'static str,
    percent: u8,
    message: impl Into<String>,
) {
    let _ = state.events.send(WebEvent {
        event_type: "progress",
        message: message.into(),
        stage: None,
        status: None,
        operation: Some(operation),
        percent: Some(percent.min(100)),
    });
}

#[cfg(test)]
mod tests {
    use super::{
        CSRF_HEADER, DoctorCheckStatus, REPORT_PATH, SESSION_COOKIE, SESSION_TTL, Session,
        SetupRequest, WebState, api_install_prepare, api_plan, api_recovery, api_report, api_reset,
        api_rollback, api_status, app_css, app_js, bootstrap, browser_addr_for, create_session,
        doctor_status_label, doctor_to_api, ensure_remote_binding_is_explicit,
        failed_doctor_details, index, install_checks_to_api, install_to_api, lock_client_ip,
        options_from_request, parse_bind, remove_session, require_allowed_client_ip,
        require_authenticated_session, require_csrf, require_session, require_session_id,
        rollback_to_api, secure_eq, secure_token, session_cookie,
    };
    use axum::Json;
    use axum::body::to_bytes;
    use axum::extract::ConnectInfo;
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
    use axum::response::IntoResponse;
    use g7_core::commands::doctor::{DoctorCheck, DoctorReport};
    use g7_core::commands::{install, plan, reset, rollback};
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tokio::sync::broadcast;

    fn test_state() -> WebState {
        WebState {
            access_token: "token".to_string(),
            domain: None,
            local_test: true,
            events: broadcast::channel(16).0,
            install_running: Arc::new(AtomicBool::new(false)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            allowed_client_ip: Arc::new(Mutex::new(None)),
        }
    }

    fn peer() -> ConnectInfo<SocketAddr> {
        ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 49152))
    }

    fn setup_request(domain: &str) -> SetupRequest {
        SetupRequest {
            domain: domain.to_string(),
            local_test: true,
            web_server: "nginx".to_string(),
            php_version: "8.3".to_string(),
            database: "mysql".to_string(),
            database_version: "apt-default".to_string(),
            app_package: "gnuboard7".to_string(),
            site_user: "g7".to_string(),
            web_root_mode: "public-html".to_string(),
            web_root: Some("  ".to_string()),
            www_mode: "redirect-to-root".to_string(),
            redis: "enable".to_string(),
            mail_mode: "none".to_string(),
            smtp_host: Some("  ".to_string()),
            smtp_port: 587,
            smtp_from: Some("  ".to_string()),
            smtp_encryption: "starttls".to_string(),
            security_profile: "standard".to_string(),
            ssh_policy: "audit-only".to_string(),
            rollback: true,
            preserve_config: true,
            dns_check: false,
        }
    }

    fn authenticated_headers(
        state: &WebState,
    ) -> std::result::Result<HeaderMap, Box<dyn std::error::Error>> {
        let session_id = create_session(state, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .expect("session should be created");
        let csrf = state
            .sessions
            .lock()
            .expect("session lock")
            .get(&session_id)
            .expect("session exists")
            .csrf_token
            .clone();

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&session_cookie(&session_id))?,
        );
        headers.insert(CSRF_HEADER, HeaderValue::from_str(&csrf)?);
        Ok(headers)
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        serde_json::from_slice(&bytes).expect("json response")
    }

    #[test]
    fn loopback_bind_is_allowed_without_remote_flag()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let bind = parse_bind("127.0.0.1:7717")?;
        ensure_remote_binding_is_explicit(bind, false)?;
        Ok(())
    }

    #[test]
    fn remote_bind_requires_explicit_flag() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let bind = parse_bind("0.0.0.0:7717")?;
        assert!(ensure_remote_binding_is_explicit(bind, false).is_err());
        ensure_remote_binding_is_explicit(bind, true)?;
        Ok(())
    }

    #[test]
    fn setup_token_is_hex_encoded_32_bytes() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let token = secure_token()?;
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|char| char.is_ascii_hexdigit()));
        Ok(())
    }

    #[test]
    fn secure_compare_checks_length_and_content() {
        assert!(secure_eq("abc", "abc"));
        assert!(!secure_eq("abc", "abcd"));
        assert!(!secure_eq("abc", "abd"));
    }

    #[test]
    fn browser_url_uses_loopback_for_unspecified_bind() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7717);
        let browser = browser_addr_for(addr);

        assert_eq!(browser.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(browser.port(), 7717);
    }

    #[test]
    fn failed_doctor_details_lists_blocking_checks_only() {
        let report = DoctorReport {
            install_allowed: false,
            checks: vec![
                DoctorCheck {
                    name: "ok",
                    status: DoctorCheckStatus::Pass,
                    message: "ready".to_string(),
                },
                DoctorCheck {
                    name: "warn",
                    status: DoctorCheckStatus::Warn,
                    message: "inspect manually".to_string(),
                },
                DoctorCheck {
                    name: "fail",
                    status: DoctorCheckStatus::Fail,
                    message: "blocked".to_string(),
                },
                DoctorCheck {
                    name: "pending",
                    status: DoctorCheckStatus::Pending,
                    message: "waiting".to_string(),
                },
            ],
        };

        assert_eq!(
            failed_doctor_details(report),
            vec!["[fail] fail - blocked", "[pending] pending - waiting"]
        );
    }

    #[test]
    fn client_ip_lock_allows_first_token_ip() {
        let state = test_state();
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        lock_client_ip(&state, client_ip).expect("first token IP should lock access");
        require_allowed_client_ip(&state, client_ip).expect("same client IP should be allowed");
    }

    #[test]
    fn client_ip_lock_rejects_different_ip() {
        let state = test_state();
        let allowed_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let other_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));

        lock_client_ip(&state, allowed_ip).expect("first token IP should lock access");
        let error = require_allowed_client_ip(&state, other_ip)
            .expect_err("different client IP should be rejected");

        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert_eq!(
            error.details,
            vec![
                "allowed_client_ip: 127.0.0.1",
                "request_client_ip: 10.0.0.5"
            ]
        );
    }

    #[test]
    fn session_cookie_uses_http_only_same_site() {
        let cookie = session_cookie("abc123");

        assert!(cookie.contains("g7inst_session=abc123"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Max-Age=1800"));
    }

    #[test]
    fn session_lifecycle_authentication_and_csrf()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = test_state();
        let session_id = create_session(&state, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .expect("session should be created");
        let cookie = session_cookie(&session_id);
        let csrf = state
            .sessions
            .lock()
            .expect("session lock")
            .get(&session_id)
            .expect("session exists")
            .csrf_token
            .clone();

        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(&cookie)?);
        headers.insert(CSRF_HEADER, HeaderValue::from_str(&csrf)?);

        let session = require_session(&state, &headers, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .expect("session should be valid");
        require_csrf(&headers, &session).expect("csrf should match");
        let session =
            require_authenticated_session(&state, &headers, IpAddr::V4(Ipv4Addr::LOCALHOST))
                .expect("token session should already be authenticated");
        assert_eq!(session.username.as_deref(), Some("root"));

        remove_session(&state, &session_id).expect("session should be removed");
        assert_eq!(
            require_session(&state, &headers, IpAddr::V4(Ipv4Addr::LOCALHOST))
                .expect_err("removed session should be invalid")
                .status,
            StatusCode::UNAUTHORIZED
        );
        Ok(())
    }

    #[test]
    fn session_header_parser_accepts_multiple_cookies()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=light; g7inst_session=session-1; other=yes"),
        );

        assert_eq!(
            require_session_id(&headers).expect("session cookie should parse"),
            "session-1"
        );
        Ok(())
    }

    #[test]
    fn csrf_rejects_missing_or_invalid_token() {
        let session = Session {
            csrf_token: "csrf".to_string(),
            authenticated: true,
            username: Some("root".to_string()),
            client_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            expires_at: Instant::now() + SESSION_TTL,
        };
        let headers = HeaderMap::new();
        assert_eq!(
            require_csrf(&headers, &session)
                .expect_err("missing csrf")
                .status,
            StatusCode::FORBIDDEN
        );

        let mut headers = HeaderMap::new();
        headers.insert(CSRF_HEADER, HeaderValue::from_static("wrong"));
        assert_eq!(
            require_csrf(&headers, &session)
                .expect_err("invalid csrf")
                .status,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn setup_request_trims_empty_optional_fields() {
        let options = options_from_request(setup_request("g7-test.local"));

        assert!(options.local_test);
        assert_eq!(options.custom_web_root, None);
        assert_eq!(options.smtp_host, None);
        assert_eq!(options.smtp_from, None);
        assert_eq!(options.web_server, "nginx");
        assert_eq!(options.database_engine, "mysql");
    }

    #[test]
    fn doctor_conversion_preserves_status_labels() {
        let report = doctor_to_api(DoctorReport {
            install_allowed: false,
            checks: vec![
                DoctorCheck {
                    name: "pass",
                    status: DoctorCheckStatus::Pass,
                    message: "ok".to_string(),
                },
                DoctorCheck {
                    name: "warn",
                    status: DoctorCheckStatus::Warn,
                    message: "watch".to_string(),
                },
                DoctorCheck {
                    name: "fail",
                    status: DoctorCheckStatus::Fail,
                    message: "blocked".to_string(),
                },
                DoctorCheck {
                    name: "pending",
                    status: DoctorCheckStatus::Pending,
                    message: "unknown".to_string(),
                },
            ],
        });

        assert!(!report.install_allowed);
        assert_eq!(
            report
                .checks
                .into_iter()
                .map(|check| check.status)
                .collect::<Vec<_>>(),
            vec!["pass", "warn", "fail", "pending"]
        );
        assert_eq!(doctor_status_label(DoctorCheckStatus::Pending), "pending");
    }

    #[test]
    fn install_and_rollback_reports_map_to_api_shapes() {
        let install_api = install_to_api(
            install::InstallReport {
                domain: "g7-test.local".to_string(),
                deployment_mode: "local-test".to_string(),
                app_profile: "gnuboard7".to_string(),
                app_profile_label: "Gnuboard 7",
                app_document_root: "/home/g7/public_html/public".to_string(),
                web_server: "nginx".to_string(),
                php_version: "8.3".to_string(),
                database_engine: "mysql".to_string(),
                site_user: "g7".to_string(),
                web_root_mode: "public-html".to_string(),
                web_root: "/home/g7/public_html".to_string(),
                www_mode: "redirect-to-root".to_string(),
                redis_mode: "enable".to_string(),
                mail_mode: "none".to_string(),
                smtp_host: None,
                smtp_port: None,
                smtp_from: None,
                smtp_encryption: None,
                dns_check: false,
                security_profile: "standard".to_string(),
                ssh_policy: "audit-only".to_string(),
                phase: "packages-installed".to_string(),
                state_path: PathBuf::from("/var/lib/g7-installer/state.json"),
                owned_files_path: PathBuf::from("/var/lib/g7-installer/owned-files.json"),
                owned_files: vec!["/etc/g7-installer/config.toml".to_string()],
                completed_steps: vec!["preflight-passed".to_string()],
                safety_checks: Vec::new(),
                preinstall_package_checks: vec![install::InstallCheck {
                    name: "nginx".to_string(),
                    status: "not-installed".to_string(),
                    message: "package was absent before G7 installer ran".to_string(),
                }],
                package_checks: vec![install::InstallCheck {
                    name: "nginx".to_string(),
                    status: "pass".to_string(),
                    message: "installed".to_string(),
                }],
                service_checks: Vec::new(),
                port_checks: Vec::new(),
                network_checks: Vec::new(),
                mail_checks: Vec::new(),
                certbot_checks: Vec::new(),
                vhost_checks: Vec::new(),
                app_requirements: vec![install::InstallCheck {
                    name: "php-version".to_string(),
                    status: "pass".to_string(),
                    message: "PHP 8.3 satisfies app minimum PHP 8.2.".to_string(),
                }],
            },
            "apt-default".to_string(),
        );
        assert_eq!(install_api.phase, "packages-installed");
        assert_eq!(install_api.database_version, "apt-default");
        assert_eq!(install_api.app_package, "gnuboard7");
        assert_eq!(install_api.app_document_root, "/home/g7/public_html/public");
        assert_eq!(install_api.mail_mode, "none");
        assert!(!install_api.dns_check);
        assert_eq!(
            install_api.preinstall_package_checks[0].status,
            "not-installed"
        );
        assert_eq!(install_api.package_checks[0].name, "nginx");
        assert_eq!(install_api.state_path, "/var/lib/g7-installer/state.json");

        let checks = install_checks_to_api(vec![install::InstallCheck {
            name: "80".to_string(),
            status: "pass".to_string(),
            message: "free".to_string(),
        }]);
        assert_eq!(checks[0].message, "free");

        let rollback_api = rollback_to_api(rollback::RollbackReport {
            dry_run: false,
            phase: "packages-installed".to_string(),
            package_actions: vec![rollback::RollbackAction {
                name: "nginx".to_string(),
                status: "removed".to_string(),
                message: "package removed".to_string(),
            }],
            service_actions: vec![rollback::RollbackAction {
                name: "nginx".to_string(),
                status: "disabled".to_string(),
                message: "service disabled".to_string(),
            }],
            metadata_reset: reset::ResetReport {
                dry_run: false,
                removed: vec!["/etc/g7-installer".to_string()],
                missing: vec!["/tmp/g7".to_string()],
            },
        });
        assert_eq!(rollback_api.package_actions[0].status, "removed");
        assert_eq!(rollback_api.metadata_reset.missing, vec!["/tmp/g7"]);
    }

    #[tokio::test]
    async fn index_with_token_creates_session_cookie()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = test_state();
        let response = index(
            axum::extract::State(state.clone()),
            peer(),
            axum::extract::Query(HashMap::from([("token".to_string(), "token".to_string())])),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("set-cookie")
            .to_str()?;
        assert!(cookie.contains(SESSION_COOKIE));
        assert_eq!(state.sessions.lock().expect("session lock").len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn static_assets_require_first_token_ip_lock()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = test_state();
        lock_client_ip(&state, IpAddr::V4(Ipv4Addr::LOCALHOST)).expect("client IP should lock");

        let js = app_js(axum::extract::State(state.clone()), peer())
            .await
            .expect("js should be served")
            .into_response();
        assert_eq!(js.status(), StatusCode::OK);
        assert_eq!(
            js.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static(
                "application/javascript; charset=utf-8"
            ))
        );
        assert_eq!(
            js.headers().get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store, no-cache, max-age=0"))
        );

        let css = app_css(axum::extract::State(state), peer())
            .await
            .expect("css should be served")
            .into_response();
        assert_eq!(css.status(), StatusCode::OK);
        assert_eq!(
            css.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/css; charset=utf-8"))
        );
        assert_eq!(
            css.headers().get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store, no-cache, max-age=0"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn bootstrap_reports_token_session_authenticated_state()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = test_state();
        let session_id = create_session(&state, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .expect("session should be created");
        let cookie = session_cookie(&session_id);
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(&cookie)?);

        let response = bootstrap(axum::extract::State(state.clone()), peer(), headers.clone())
            .await
            .expect("bootstrap should respond")
            .into_response();
        let payload = response_json(response).await;
        assert_eq!(payload["auth"]["mode"], "sudo-token");
        assert_eq!(payload["auth"]["status"], "authenticated");
        assert_eq!(payload["auth"]["authenticated"], true);
        assert_eq!(payload["auth"]["username"], "root");
        Ok(())
    }

    #[tokio::test]
    async fn plan_api_requires_authentication_and_returns_plan()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = test_state();
        let headers = authenticated_headers(&state)?;
        let response = api_plan(
            axum::extract::State(state),
            peer(),
            headers,
            Json(setup_request("g7-test.local")),
        )
        .await
        .expect("plan should succeed")
        .into_response();
        let payload = response_json(response).await;

        assert_eq!(payload["domain"], "g7-test.local");
        assert_eq!(payload["deployment_mode"], "local-test");
        assert_eq!(payload["web_server"], "nginx");
        assert_eq!(payload["database_version"], "apt-default");
        assert_eq!(payload["app_package"], "gnuboard7");
        assert!(payload["packages"].as_array().expect("packages").len() > 5);
        Ok(())
    }

    #[tokio::test]
    async fn status_report_reset_and_rollback_error_paths_are_json()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = test_state();
        let headers = authenticated_headers(&state)?;

        let status_response =
            api_status(axum::extract::State(state.clone()), peer(), headers.clone())
                .await
                .expect("status should respond")
                .into_response();
        let status_payload = response_json(status_response).await;
        assert_eq!(status_payload["installed"], false);

        let report_response =
            api_report(axum::extract::State(state.clone()), peer(), headers.clone())
                .await
                .expect("report should respond")
                .into_response();
        let report_payload = response_json(report_response).await;
        assert_eq!(report_payload["exists"], false);
        assert_eq!(report_payload["path"], REPORT_PATH);

        let recovery_response =
            api_recovery(axum::extract::State(state.clone()), peer(), headers.clone())
                .await
                .expect("recovery should respond")
                .into_response();
        let recovery_payload = response_json(recovery_response).await;
        assert_eq!(recovery_payload["can_reset"], false);
        assert_eq!(recovery_payload["can_rollback"], false);
        assert_eq!(recovery_payload["recommended_action"], "manual");

        let reset_error = match api_reset(
            axum::extract::State(state.clone()),
            peer(),
            headers.clone(),
            Json(super::ResetRequest { dry_run: true }),
        )
        .await
        {
            Ok(_) => panic!("non-root reset should fail in unit test"),
            Err(error) => error,
        };
        assert_eq!(reset_error.status, StatusCode::BAD_REQUEST);
        assert!(reset_error.hint.expect("reset hint").contains("root"));

        let rollback_error = match api_rollback(
            axum::extract::State(state),
            peer(),
            headers,
            Json(super::RollbackRequest { dry_run: true }),
        )
        .await
        {
            Ok(_) => panic!("non-root rollback should fail in unit test"),
            Err(error) => error,
        };
        assert_eq!(rollback_error.status, StatusCode::BAD_REQUEST);
        assert!(
            rollback_error
                .hint
                .expect("rollback hint")
                .contains("웹루트")
        );
        Ok(())
    }

    #[tokio::test]
    async fn install_prepare_rejects_concurrent_actions()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let state = test_state();
        state.install_running.store(true, Ordering::SeqCst);
        let headers = authenticated_headers(&state)?;

        let error = match api_install_prepare(
            axum::extract::State(state),
            peer(),
            headers,
            Json(setup_request("g7-test.local")),
        )
        .await
        {
            Ok(_) => panic!("busy install should be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.message, "install is already running");
        Ok(())
    }

    #[test]
    fn public_plan_api_mapping_exposes_user_visible_fields()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let install_plan = plan::build_with_options(
            "example.com".to_string(),
            options_from_request(setup_request("example.com")),
        )?;
        let api = super::plan_to_api(install_plan, "apt-default".to_string());

        assert_eq!(api.domain, "example.com");
        assert_eq!(api.database_version, "apt-default");
        assert_eq!(api.app_package, "gnuboard7");
        assert_eq!(api.app_document_root, "/home/g7/public_html/public");
        assert_eq!(api.web_root, "/home/g7/public_html");
        assert!(api.text.contains("G7 Installer Plan"));
        assert!(
            api.files
                .iter()
                .any(|file| file.path == "/home/g7/public_html")
        );
        assert!(api.ports.iter().any(|port| port.port == 80));
        assert!(
            api.security_checks
                .iter()
                .any(|check| check.name == "redis-local-only")
        );
        assert!(
            api.app_requirements
                .iter()
                .any(|requirement| requirement.name == "php-version")
        );
        assert!(
            api.provisioning
                .iter()
                .any(|section| section.name == "php-runtime")
        );
        assert!(api.provisioning.iter().any(|section| {
            section.name == "server-sizing"
                && section
                    .settings
                    .iter()
                    .any(|setting| setting.key == "tier_gt32gb")
        }));
        assert!(api.provisioning.iter().any(|section| {
            section
                .settings
                .iter()
                .any(|setting| setting.key == "password_policy")
        }));
        assert!(
            api.stop_conditions
                .iter()
                .any(|condition| condition.contains("Apache is running"))
        );
        Ok(())
    }
}
