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
const PROMO_JSON: &str = include_str!("../../../web/promo.sample.json");
const DEFAULT_PROMO_MANIFEST_URL: &str = "/promo.json";
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
    #[serde(default)]
    install_template: Option<String>,
    web_server: String,
    php_version: String,
    #[serde(default = "default_php_source")]
    php_source: String,
    database: String,
    database_version: String,
    #[serde(default)]
    database_name: Option<String>,
    #[serde(default)]
    database_user: Option<String>,
    #[serde(default)]
    database_password: Option<String>,
    #[serde(default)]
    database_password_confirm: Option<String>,
    app_package: String,
    site_user: String,
    #[serde(default)]
    site_password: Option<String>,
    #[serde(default)]
    site_password_confirm: Option<String>,
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

#[derive(Debug, Deserialize)]
struct ProvisionActionRequest {
    action: String,
}

#[derive(Debug, Serialize)]
struct ProvisionActionReport {
    action: String,
    status: String,
    message: String,
    checks: Vec<InstallApiCheck>,
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
    php_source: String,
    database: String,
    database_version: String,
    database_name: String,
    database_user: String,
    database_password_policy: &'static str,
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
    php_source: String,
    database: String,
    database_version: String,
    database_name: String,
    database_user: String,
    database_password_policy: &'static str,
    app_package: String,
    site_user: String,
    web_root_mode: String,
    web_root: String,
    app_url: String,
    www_mode: String,
    redis: String,
    mail_mode: String,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    smtp_from: Option<String>,
    smtp_encryption: Option<String>,
    dns_check: bool,
    security_profile: String,
    ssh_policy: String,
    phase: String,
    state_path: String,
    owned_files_path: String,
    owned_files: Vec<String>,
    completed_steps: Vec<String>,
    safety_checks: Vec<InstallApiCheck>,
    preinstall_package_checks: Vec<InstallApiCheck>,
    package_checks: Vec<InstallApiCheck>,
    service_checks: Vec<InstallApiCheck>,
    port_checks: Vec<InstallApiCheck>,
    network_checks: Vec<InstallApiCheck>,
    runtime_checks: Vec<InstallApiCheck>,
    database_checks: Vec<InstallApiCheck>,
    firewall_checks: Vec<InstallApiCheck>,
    mail_checks: Vec<InstallApiCheck>,
    certbot_checks: Vec<InstallApiCheck>,
    vhost_checks: Vec<InstallApiCheck>,
    app_checks: Vec<InstallApiCheck>,
    setup_guide_path: String,
    backup_manifest_path: String,
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
    actions: Vec<ResetApiAction>,
    removed: Vec<String>,
    missing: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ResetApiAction {
    name: String,
    status: String,
    message: String,
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
    install_running: bool,
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
        .route("/setup/connect", get(index))
        .route("/setup/doctor", get(index))
        .route("/setup/options", get(index))
        .route("/setup/plan", get(index))
        .route("/setup/install", get(index))
        .route("/setup/result", get(index))
        .route("/setup/provision", get(index))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/promo.json", get(promo_json))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/auth/logout", post(api_logout))
        .route("/api/events", get(api_events))
        .route("/api/doctor", get(api_doctor))
        .route("/api/plan", post(api_plan))
        .route("/api/install/prepare", post(api_install_prepare))
        .route("/api/provision/action", post(api_provision_action))
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
    INDEX_HTML
        .replace("__G7INST_ASSET_VERSION__", ASSET_VERSION)
        .replace(
            "__G7INST_PROMO_MANIFEST_URL__",
            &html_attr_escape(&promo_manifest_url()),
        )
}

fn promo_manifest_url() -> String {
    std::env::var("G7_PROMO_MANIFEST_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PROMO_MANIFEST_URL.to_string())
}

fn html_attr_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

async fn promo_json(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        PROMO_JSON,
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
        message: "실시간 로그 연결됨".to_string(),
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
    emit_log(&state, "서버 점검 실행 중");
    let report = doctor_to_api(doctor::run());
    emit_log(
        &state,
        format!(
            "서버 점검 완료: {}",
            if report.install_allowed {
                "설치 가능"
            } else {
                "설치 차단"
            }
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
    validate_template_app_request(&request)?;
    validate_site_password_request(&request)?;
    validate_database_request(&request)?;
    emit_log(&state, "설치 계획 계산 중");
    let domain = request.domain.clone();
    let database_version = normalize_database_version(&request.database_version);
    let options = options_from_request(request);
    let install_plan = match plan::build_with_options(domain, options) {
        Ok(install_plan) => install_plan,
        Err(error) => {
            emit_log(&state, format!("설치 계획 생성 실패: {error}"));
            return Err(ApiError::bad_request(error)
                .with_hint("설치 옵션 값을 확인한 뒤 다시 계획을 생성하세요."));
        }
    };
    emit_log(&state, "설치 계획 계산 완료");

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
    validate_template_app_request(&request)?;
    validate_site_password_request(&request)?;
    validate_database_request(&request)?;

    if state.install_running.swap(true, Ordering::SeqCst) {
        emit_log(&state, "설치 요청 거부: 이미 다른 설치 작업이 진행 중");
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
            emit_progress(&state, "install", 15, "install progress: preflight passed");
            emit_stage(&state, "packages", "성공", "packages installed");
            emit_progress(
                &state,
                "install",
                30,
                "install progress: packages installed",
            );
            emit_stage(
                &state,
                "site",
                "성공",
                "site account and web root configured",
            );
            emit_progress(
                &state,
                "install",
                42,
                "install progress: site account and web root configured",
            );
            emit_stage(
                &state,
                "vhost",
                "성공",
                "web server vhost and HTTP smoke verified",
            );
            emit_progress(&state, "install", 54, "install progress: vhost verified");
            emit_stage(&state, "runtime", "성공", "PHP runtime configured");
            emit_progress(
                &state,
                "install",
                66,
                "install progress: runtime configured",
            );
            emit_stage(&state, "database", "성공", "database configured");
            emit_progress(
                &state,
                "install",
                76,
                "install progress: database configured",
            );
            emit_stage(
                &state,
                "ssl",
                "성공",
                "TLS certificate and HTTPS vhost verified",
            );
            emit_progress(&state, "install", 88, "install progress: TLS configured");
            emit_stage(&state, "app", "성공", "web app files prepared");
            emit_stage(&state, "report", "성공", "setup guide and report prepared");
            emit_progress(&state, "install", 100, "install progress: report ready");
            emit_log(&state, "서버 설치 완료");
            Ok(Json(install_to_api(report, database_version)))
        }
        Err(error) => {
            let details = failed_report_details();
            emit_progress(&state, "install", 100, "install progress: failed");
            emit_stage(&state, "report", "실패", format!("install failed: {error}"));
            Err(ApiError::bad_request(error)
                .with_hint(
                    "리포트의 실패 항목을 확인하세요. 중단된 서버 세팅 단계 이후 작업은 실행하지 않습니다.",
                )
                .with_details(details))
        }
    }
}

async fn api_provision_action(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<ProvisionActionRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;

    if state.install_running.load(Ordering::SeqCst) {
        return Err(ApiError::conflict(
            "provision action is blocked while install is running",
        ));
    }

    let report = read_saved_report_json()?;
    emit_log(&state, format!("후속 작업 실행 중: {}", request.action));
    let action_report = run_provision_action(&request.action, &report)?;
    emit_log(
        &state,
        format!(
            "후속 작업 완료: {} {}",
            action_report.action, action_report.status
        ),
    );

    Ok(Json(action_report))
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

    emit_log(&state, "재설치 초기화 실행 중");
    emit_progress(
        &state,
        "reset",
        10,
        "reset progress: starting full installer reset",
    );
    let report = match reset::run(true, request.dry_run) {
        Ok(report) => report,
        Err(error) => {
            emit_progress(&state, "reset", 100, "reset progress: failed");
            emit_log(&state, format!("재설치 초기화 실패: {error}"));
            return Err(ApiError::bad_request(error)
                .with_hint("root 권한과 installer report/owned-files 상태를 확인하세요."));
        }
    };
    emit_progress(
        &state,
        "reset",
        100,
        "reset progress: full installer reset completed",
    );
    emit_log(&state, "재설치 초기화 완료");

    Ok(Json(ResetApiReport {
        dry_run: report.dry_run,
        actions: reset_actions_to_api(report.actions),
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

    emit_log(&state, "패키지 되돌리기 실행 중");
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
            emit_log(&state, format!("패키지 되돌리기 실패: {error}"));
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
    emit_log(&state, "패키지 되돌리기 완료");

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
        install_running: state.install_running.load(Ordering::SeqCst),
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

fn read_saved_report_json() -> std::result::Result<serde_json::Value, ApiError> {
    let content = fs::read_to_string(REPORT_PATH).map_err(|error| {
        ApiError::bad_request(format!("failed to read report: {error}"))
            .with_hint("먼저 기본 서버 구성을 실행해 리포트를 생성하세요.")
    })?;
    serde_json::from_str(&content).map_err(|error| {
        ApiError::bad_request(format!("failed to parse report: {error}"))
            .with_hint("리포트 파일이 손상되었습니다. 재설치 초기화 후 다시 진행하세요.")
    })
}

fn run_provision_action(
    action: &str,
    report: &serde_json::Value,
) -> std::result::Result<ProvisionActionReport, ApiError> {
    let action = action.trim();
    let checks = match action {
        "webserver" => provision_webserver(report),
        "php" => provision_php(report),
        "database" => provision_database(report),
        "ssl" => provision_ssl(report),
        "mail" => provision_mail(report),
        "security" => provision_security(report),
        "app" => provision_app(report),
        _ => {
            return Err(
                ApiError::bad_request(format!("unsupported provision action: {action}")).with_hint(
                    "지원 작업은 webserver, php, database, ssl, mail, security, app 입니다.",
                ),
            );
        }
    };
    let status = if checks.iter().any(|check| check.status == "fail") {
        "fail"
    } else if checks.iter().any(|check| check.status == "manual") {
        "manual"
    } else {
        "pass"
    }
    .to_string();
    let message = match status.as_str() {
        "pass" => "재시작/점검이 완료되었습니다.",
        "manual" => "자동 실행 대신 후속 수동 확인이 필요합니다.",
        _ => "실패 항목을 확인하세요.",
    }
    .to_string();

    Ok(ProvisionActionReport {
        action: action.to_string(),
        status,
        message,
        checks,
    })
}

fn provision_webserver(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let web_server = report_string(report, "web_server").unwrap_or_else(|| "nginx".to_string());
    if web_server == "apache" {
        vec![
            run_command_check("apache-configtest", "apache2ctl", &["configtest"], None),
            run_command_check("apache-reload", "systemctl", &["reload", "apache2"], None),
        ]
    } else if web_server == "frankenphp" {
        vec![
            run_command_check("nginx-configtest", "nginx", &["-t"], None),
            run_command_check("nginx-reload", "systemctl", &["reload", "nginx"], None),
            run_command_check(
                "frankenphp-active",
                "systemctl",
                &["is-active", "--quiet", "g7-frankenphp"],
                None,
            ),
        ]
    } else {
        vec![
            run_command_check("nginx-configtest", "nginx", &["-t"], None),
            run_command_check("nginx-reload", "systemctl", &["reload", "nginx"], None),
        ]
    }
}

fn provision_php(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    if report_string(report, "web_server").as_deref() == Some("frankenphp") {
        return vec![
            run_command_check(
                "frankenphp-restart",
                "systemctl",
                &["restart", "g7-frankenphp"],
                None,
            ),
            run_command_check(
                "frankenphp-active",
                "systemctl",
                &["is-active", "--quiet", "g7-frankenphp"],
                None,
            ),
        ];
    }
    let version = report_string(report, "php_version").unwrap_or_else(|| "8.5".to_string());
    let service = format!("php{version}-fpm");
    vec![
        run_command_check("php-fpm-restart", "systemctl", &["restart", &service], None),
        run_command_check(
            "php-fpm-active",
            "systemctl",
            &["is-active", "--quiet", &service],
            None,
        ),
    ]
}

fn provision_database(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let service = if report_string(report, "database").as_deref() == Some("mariadb") {
        "mariadb"
    } else {
        "mysql"
    };
    vec![
        run_command_check("database-restart", "systemctl", &["restart", service], None),
        run_command_check(
            "database-active",
            "systemctl",
            &["is-active", "--quiet", service],
            None,
        ),
    ]
}

fn provision_ssl(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let cert_name = report_string(report, "domain").unwrap_or_default();
    if cert_name.is_empty() {
        return vec![InstallApiCheck {
            name: "certbot-certificate".to_string(),
            status: "fail".to_string(),
            message: "리포트에서 인증서 이름으로 사용할 도메인을 찾지 못했습니다.".to_string(),
        }];
    }

    let fullchain_path = format!("/etc/letsencrypt/live/{cert_name}/fullchain.pem");
    let privkey_path = format!("/etc/letsencrypt/live/{cert_name}/privkey.pem");
    if !path_is_file(&fullchain_path) || !path_is_file(&privkey_path) {
        return vec![
            file_check("certbot-fullchain", &fullchain_path),
            file_check("certbot-privkey", &privkey_path),
            InstallApiCheck {
                name: "certbot-rate-limit-guard".to_string(),
                status: "manual".to_string(),
                message:
                    "기존 인증서가 없어 7단계 점검에서는 새 발급을 실행하지 않았습니다. 중복 발급 제한을 피하려면 기본 구성/TLS 단계를 한 번만 완료하세요."
                        .to_string(),
            },
        ];
    }

    vec![
        file_check("certbot-fullchain", &fullchain_path),
        file_check("certbot-privkey", &privkey_path),
        run_command_check(
            "certbot-certificate-list",
            "certbot",
            &["certificates", "--cert-name", &cert_name],
            None,
        ),
        run_command_check(
            "certbot-renew-dry-run",
            "certbot",
            &[
                "renew",
                "--dry-run",
                "--no-random-sleep-on-renew",
                "--cert-name",
                &cert_name,
            ],
            None,
        ),
    ]
}

fn provision_mail(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    if report_string(report, "mail_mode").as_deref() != Some("local-postfix") {
        return vec![InstallApiCheck {
            name: "mail".to_string(),
            status: "manual".to_string(),
            message: "로컬 Postfix 모드가 아니라 자동 재시작 대상이 없습니다.".to_string(),
        }];
    }

    vec![
        run_command_check(
            "postfix-restart",
            "systemctl",
            &["restart", "postfix"],
            None,
        ),
        run_command_check(
            "postfix-active",
            "systemctl",
            &["is-active", "--quiet", "postfix"],
            None,
        ),
    ]
}

fn provision_security(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let security_profile =
        report_string(report, "security_profile").unwrap_or_else(|| "standard".to_string());
    let ssh_policy =
        report_string(report, "ssh_policy").unwrap_or_else(|| "audit-only".to_string());
    let mut checks = vec![InstallApiCheck {
        name: "security-policy".to_string(),
        status: "manual".to_string(),
        message: format!(
            "보안 수준은 `{security_profile}`, SSH 정책은 `{ssh_policy}`입니다. SSH 차단을 피하기 위해 자동 변경 대신 점검 결과를 확인하세요."
        ),
    }];

    checks.push(run_command_check(
        "ssh-service-active",
        "systemctl",
        &["is-active", "--quiet", "ssh"],
        None,
    ));
    checks.push(run_command_check(
        "ufw-status",
        "ufw",
        &["status", "verbose"],
        None,
    ));
    checks
}

fn provision_app(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let app_profile = report_string(report, "app_profile")
        .or_else(|| report_string(report, "app_package"))
        .unwrap_or_default();
    let web_root = report_string(report, "web_root").unwrap_or_default();
    let site_user = report_string(report, "site_user").unwrap_or_default();
    let app_document_root = report_string(report, "app_document_root").unwrap_or(web_root.clone());

    if app_profile == "gnuboard7" {
        let mut checks = vec![
            file_check("app-artisan", &format!("{web_root}/artisan")),
            dir_check("app-storage", &format!("{web_root}/storage")),
            file_check(
                "g7-core-template-engine",
                &format!("{web_root}/public/build/core/template-engine.min.js"),
            ),
        ];
        checks.extend(app_permission_checks(
            &web_root,
            &site_user,
            &["storage", "bootstrap/cache"],
        ));
        let install_lock = format!("{web_root}/storage/app/g7_installed");
        checks.push(InstallApiCheck {
            name: "g7-install-lock".to_string(),
            status: if path_is_file(&install_lock) {
                "pass".to_string()
            } else {
                "manual".to_string()
            },
            message: if path_is_file(&install_lock) {
                format!("{install_lock} 파일을 확인했습니다. 브라우저 설치가 완료된 상태입니다.")
            } else {
                format!("{install_lock} 파일이 없습니다. 브라우저 /install 완료 전이면 정상이고, 완료 후라면 설치 잠금을 확인하세요.")
            },
        });
        if !site_user.is_empty() {
            checks.push(run_command_check(
                "g7-artisan-about",
                "runuser",
                &["-u", &site_user, "--", "php", "artisan", "about"],
                Some(&web_root),
            ));
        }
        checks.push(g7_ckeditor_upload_limit_check(&format!(
            "{web_root}/storage/app/plugins/sirsoft-ckeditor5/settings/setting.json"
        )));
        checks.push(InstallApiCheck {
            name: "app-browser-install".to_string(),
            status: "manual".to_string(),
            message: "그누보드7은 브라우저 /install 완료 후 /, /login, /admin 접속을 확인하세요."
                .to_string(),
        });
        return checks;
    }

    if app_profile == "wordpress" {
        let mut checks = vec![
            file_check(
                "wordpress-install-screen",
                &format!("{app_document_root}/wp-admin/install.php"),
            ),
            dir_check(
                "wordpress-uploads",
                &format!("{app_document_root}/wp-content"),
            ),
        ];
        checks.extend(app_permission_checks(
            &web_root,
            &site_user,
            &["wp-content/uploads"],
        ));
        return checks;
    }

    let mut checks = vec![file_check(
        "laravel-artisan",
        &format!("{web_root}/artisan"),
    )];
    checks.extend(app_permission_checks(
        &web_root,
        &site_user,
        &["storage", "bootstrap/cache"],
    ));
    checks.push(run_command_check(
        "artisan-about",
        "php",
        &["artisan", "about"],
        Some(&web_root),
    ));
    checks
}

fn app_permission_checks(
    web_root: &str,
    site_user: &str,
    writable_paths: &[&str],
) -> Vec<InstallApiCheck> {
    if web_root.is_empty() {
        return vec![InstallApiCheck {
            name: "app-web-root".to_string(),
            status: "fail".to_string(),
            message: "리포트에서 앱 경로를 찾지 못했습니다.".to_string(),
        }];
    }

    let mut checks = Vec::new();
    if site_user.is_empty() {
        checks.push(InstallApiCheck {
            name: "app-site-user".to_string(),
            status: "fail".to_string(),
            message: "리포트에서 사이트 계정을 찾지 못해 소유권을 적용할 수 없습니다.".to_string(),
        });
    } else {
        let owner_group = format!("{site_user}:www-data");
        checks.push(run_command_check(
            "app-web-root-owner",
            "chown",
            &["-R", &owner_group, web_root],
            None,
        ));
    }

    checks.push(run_command_check(
        "app-web-root-mode",
        "chmod",
        &["-R", "0755", web_root],
        None,
    ));

    for writable_path in writable_paths {
        let target = format!("{web_root}/{writable_path}");
        checks.push(dir_check(&format!("app-dir:{writable_path}"), &target));
        if path_is_dir(&target) {
            checks.push(run_command_check(
                &format!("app-writable-mode:{writable_path}"),
                "chmod",
                &["-R", "0775", &target],
                None,
            ));
            if !site_user.is_empty() {
                checks.push(run_command_check(
                    &format!("app-writable-test:{writable_path}"),
                    "runuser",
                    &["-u", site_user, "--", "test", "-w", &target],
                    None,
                ));
            }
        }
    }

    let env_path = format!("{web_root}/.env");
    checks.push(file_check("app-env", &env_path));
    if path_is_file(&env_path) {
        checks.push(run_command_check(
            "app-env-mode",
            "chmod",
            &["0640", &env_path],
            None,
        ));
    }

    checks
}

fn g7_ckeditor_upload_limit_check(path: &str) -> InstallApiCheck {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            return InstallApiCheck {
                name: "g7-ckeditor-upload-limit".to_string(),
                status: "manual".to_string(),
                message: format!(
                    "{path} 파일이 없습니다. CKEditor5 플러그인을 설치한 뒤 imageMaxSizeMb 값을 확인하세요."
                ),
            };
        }
    };

    let limit = serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|value| value.get("imageMaxSizeMb").cloned())
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        });

    match limit {
        Some(value) if value > 2 => InstallApiCheck {
            name: "g7-ckeditor-upload-limit".to_string(),
            status: "pass".to_string(),
            message: format!(
                "CKEditor5 imageMaxSizeMb={value}MB 입니다. PHP/Nginx 한도와 함께 확인하세요."
            ),
        },
        Some(value) => InstallApiCheck {
            name: "g7-ckeditor-upload-limit".to_string(),
            status: "manual".to_string(),
            message: format!(
                "CKEditor5 imageMaxSizeMb={value}MB 입니다. 큰 이미지 업로드가 필요하면 플러그인 설정을 조정하세요."
            ),
        },
        None => InstallApiCheck {
            name: "g7-ckeditor-upload-limit".to_string(),
            status: "manual".to_string(),
            message: "CKEditor5 설정 파일은 있으나 imageMaxSizeMb 값을 읽지 못했습니다."
                .to_string(),
        },
    }
}

fn file_check(name: &str, path: &str) -> InstallApiCheck {
    let exists = path_is_file(path);
    InstallApiCheck {
        name: name.to_string(),
        status: if exists { "pass" } else { "fail" }.to_string(),
        message: if exists {
            format!("{path} 파일을 확인했습니다.")
        } else {
            format!("{path} 파일이 없습니다.")
        },
    }
}

fn dir_check(name: &str, path: &str) -> InstallApiCheck {
    let exists = path_is_dir(path);
    InstallApiCheck {
        name: name.to_string(),
        status: if exists { "pass" } else { "fail" }.to_string(),
        message: if exists {
            format!("{path} 디렉터리를 확인했습니다.")
        } else {
            format!("{path} 디렉터리가 없습니다.")
        },
    }
}

fn path_is_file(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn path_is_dir(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn run_command_check(
    name: &str,
    program: &str,
    args: &[&str],
    cwd: Option<&str>,
) -> InstallApiCheck {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let display = std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");

    match command.output() {
        Ok(output) if output.status.success() => InstallApiCheck {
            name: name.to_string(),
            status: "pass".to_string(),
            message: format!("{display} 성공"),
        },
        Ok(output) => InstallApiCheck {
            name: name.to_string(),
            status: "fail".to_string(),
            message: format!(
                "{display} 실패: status={} stdout={} stderr={}",
                output.status,
                trim_command_output(&output.stdout),
                trim_command_output(&output.stderr)
            ),
        },
        Err(error) => InstallApiCheck {
            name: name.to_string(),
            status: "fail".to_string(),
            message: format!("{display} 실행 실패: {error}"),
        },
    }
}

fn trim_command_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.chars().count() > 400 {
        let prefix = trimmed.chars().take(400).collect::<String>();
        format!("{prefix}...")
    } else {
        trimmed.to_string()
    }
}

fn report_string(report: &serde_json::Value, key: &str) -> Option<String> {
    report
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
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
            "설치기가 만든 계정, DB, 인증서, 서비스, 웹루트/설정 파일, 패키지, 메타데이터를 제거하고 재설치 상태로 되돌립니다."
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
        request.php_source,
        request.database,
        request
            .database_name
            .filter(|value| !value.trim().is_empty()),
        request
            .database_user
            .filter(|value| !value.trim().is_empty()),
        request
            .database_password
            .filter(|value| !value.trim().is_empty()),
        request.site_user,
        request
            .site_password
            .filter(|value| !value.trim().is_empty()),
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

fn default_php_source() -> String {
    plan::DEFAULT_PHP_SOURCE.to_string()
}

fn validate_template_app_request(request: &SetupRequest) -> std::result::Result<(), ApiError> {
    let install_template = request.install_template.as_deref().unwrap_or("recommended");
    let app_package = request.app_package.as_str();

    if !matches!(install_template, "recommended" | "apache") {
        return Err(ApiError::bad_request(
            "공개 설치기는 권장 설치 또는 Apache 호환 템플릿만 지원합니다.",
        )
        .with_hint("설치 템플릿을 권장 설치 또는 Apache 호환으로 바꾸세요."));
    }

    if !matches!(request.web_server.as_str(), "nginx" | "apache") {
        return Err(
            ApiError::bad_request("공개 설치기는 Nginx 또는 Apache 웹서버만 지원합니다.")
                .with_hint("웹서버를 Nginx 또는 Apache로 바꾸세요."),
        );
    }

    if !matches!(app_package, "gnuboard7" | "wordpress") {
        return Err(ApiError::bad_request(
            "공개 설치기는 그누보드7 또는 WordPress 앱만 지원합니다.",
        )
        .with_hint("설치할 앱을 그누보드7 또는 WordPress로 바꾸세요."));
    }

    Ok(())
}

fn validate_site_password_request(request: &SetupRequest) -> std::result::Result<(), ApiError> {
    let password = request.site_password.as_deref().unwrap_or("");
    let confirm = request.site_password_confirm.as_deref().unwrap_or("");

    if password.is_empty() {
        return Err(ApiError::bad_request("사이트 계정 비밀번호를 입력하세요.")
            .with_hint("이 비밀번호는 사이트 파일 SFTP 접속과 Linux 계정 로그인에 사용됩니다."));
    }

    if password != confirm {
        return Err(
            ApiError::bad_request("사이트 계정 비밀번호 확인이 일치하지 않습니다.")
                .with_hint("비밀번호와 확인 입력값을 다시 입력하세요."),
        );
    }

    if password.len() < 8 {
        return Err(
            ApiError::bad_request("사이트 계정 비밀번호는 8자 이상이어야 합니다.")
                .with_hint("콜론, 줄바꿈, 제어문자는 사용할 수 없습니다."),
        );
    }

    if password
        .chars()
        .any(|ch| ch == ':' || ch == '\n' || ch == '\r' || ch.is_control())
    {
        return Err(ApiError::bad_request(
            "사이트 계정 비밀번호에 사용할 수 없는 문자가 있습니다.",
        )
        .with_hint("콜론, 줄바꿈, 제어문자는 사용할 수 없습니다."));
    }

    Ok(())
}

fn validate_database_request(request: &SetupRequest) -> std::result::Result<(), ApiError> {
    let database_name = request.database_name.as_deref().unwrap_or("").trim();
    let database_user = request.database_user.as_deref().unwrap_or("").trim();
    let password = request.database_password.as_deref().unwrap_or("");
    let confirm = request.database_password_confirm.as_deref().unwrap_or("");

    if database_name.is_empty() {
        return Err(ApiError::bad_request("DB 이름을 입력하세요.")
            .with_hint("영문, 숫자, 밑줄만 사용할 수 있습니다."));
    }
    if !is_database_identifier(database_name, 64) {
        return Err(ApiError::bad_request("DB 이름 형식이 올바르지 않습니다.")
            .with_hint("영문 또는 밑줄로 시작하고 영문, 숫자, 밑줄만 사용하세요."));
    }
    if database_user.is_empty() {
        return Err(ApiError::bad_request("DB 계정을 입력하세요.")
            .with_hint("영문, 숫자, 밑줄만 사용할 수 있습니다."));
    }
    if !is_database_identifier(database_user, 32) {
        return Err(ApiError::bad_request("DB 계정 형식이 올바르지 않습니다.")
            .with_hint("영문 또는 밑줄로 시작하고 영문, 숫자, 밑줄만 사용하세요."));
    }
    if password.is_empty() {
        return Err(ApiError::bad_request("DB 비밀번호를 입력하세요.")
            .with_hint("앱이 DB에 접속할 때 사용할 비밀번호입니다."));
    }
    if password != confirm {
        return Err(
            ApiError::bad_request("DB 비밀번호 확인이 일치하지 않습니다.")
                .with_hint("DB 비밀번호와 확인 입력값을 다시 입력하세요."),
        );
    }
    if password.len() < 8 {
        return Err(
            ApiError::bad_request("DB 비밀번호는 8자 이상이어야 합니다.")
                .with_hint("작은따옴표, 백슬래시, 줄바꿈, 제어문자는 사용할 수 없습니다."),
        );
    }
    if password
        .chars()
        .any(|ch| ch == '\'' || ch == '\\' || ch == '\n' || ch == '\r' || ch.is_control())
    {
        return Err(
            ApiError::bad_request("DB 비밀번호에 사용할 수 없는 문자가 있습니다.")
                .with_hint("작은따옴표, 백슬래시, 줄바꿈, 제어문자는 사용할 수 없습니다."),
        );
    }

    Ok(())
}

fn is_database_identifier(value: &str, max_len: usize) -> bool {
    value.len() <= max_len
        && value
            .chars()
            .next()
            .map(|ch| ch.is_ascii_alphabetic() || ch == '_')
            .unwrap_or(false)
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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

#[cfg(test)]
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

fn failed_report_details() -> Vec<String> {
    let Ok(content) = fs::read_to_string(REPORT_PATH) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };

    let mut details = Vec::new();
    if let Some(problem) = value.get("problem").and_then(serde_json::Value::as_str) {
        details.push(format!("[problem] {problem}"));
    }

    for section in [
        "package_checks",
        "vhost_checks",
        "runtime_checks",
        "database_checks",
        "certbot_checks",
        "app_checks",
    ] {
        let Some(checks) = value.get(section).and_then(serde_json::Value::as_array) else {
            continue;
        };
        for check in checks {
            let status = check
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            if status != "fail" {
                continue;
            }
            let name = check
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let message = check
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            details.push(format!("[{status}] {section}.{name} - {message}"));
        }
    }

    details
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
        php_source: install_plan.php_source,
        database: install_plan.database_engine,
        database_version,
        database_name: install_plan.database_name,
        database_user: install_plan.database_user,
        database_password_policy: install_plan.database_password_policy,
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
        php_source: report.php_source,
        database: report.database_engine,
        database_version,
        database_name: report.database_name,
        database_user: report.database_user,
        database_password_policy: report.database_password_policy,
        app_package: report.app_profile,
        site_user: report.site_user,
        web_root_mode: report.web_root_mode,
        web_root: report.web_root,
        app_url: report.app_url,
        www_mode: report.www_mode,
        redis: report.redis_mode,
        mail_mode: report.mail_mode,
        smtp_host: report.smtp_host,
        smtp_port: report.smtp_port,
        smtp_from: report.smtp_from,
        smtp_encryption: report.smtp_encryption,
        dns_check: report.dns_check,
        security_profile: report.security_profile,
        ssh_policy: report.ssh_policy,
        phase: report.phase,
        state_path: report.state_path.display().to_string(),
        owned_files_path: report.owned_files_path.display().to_string(),
        owned_files: report.owned_files,
        completed_steps: report.completed_steps,
        safety_checks: install_checks_to_api(report.safety_checks),
        preinstall_package_checks: install_checks_to_api(report.preinstall_package_checks),
        package_checks: install_checks_to_api(report.package_checks),
        service_checks: install_checks_to_api(report.service_checks),
        port_checks: install_checks_to_api(report.port_checks),
        network_checks: install_checks_to_api(report.network_checks),
        runtime_checks: install_checks_to_api(report.runtime_checks),
        database_checks: install_checks_to_api(report.database_checks),
        firewall_checks: install_checks_to_api(report.firewall_checks),
        mail_checks: install_checks_to_api(report.mail_checks),
        certbot_checks: install_checks_to_api(report.certbot_checks),
        vhost_checks: install_checks_to_api(report.vhost_checks),
        app_checks: install_checks_to_api(report.app_checks),
        setup_guide_path: report.setup_guide_path.display().to_string(),
        backup_manifest_path: report.backup_manifest_path.display().to_string(),
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
            actions: reset_actions_to_api(report.metadata_reset.actions),
            removed: report.metadata_reset.removed,
            missing: report.metadata_reset.missing,
        },
    }
}

fn reset_actions_to_api(actions: Vec<reset::ResetAction>) -> Vec<ResetApiAction> {
    actions
        .into_iter()
        .map(|action| ResetApiAction {
            name: action.name,
            status: action.status,
            message: action.message,
        })
        .collect()
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
            emit_log(state, format!("접속 IP 잠금 완료: {client_ip}"));
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
        options_from_request, parse_bind, promo_json, remove_session, require_allowed_client_ip,
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
            install_template: Some("recommended".to_string()),
            web_server: "nginx".to_string(),
            php_version: "8.5".to_string(),
            php_source: "auto".to_string(),
            database: "mysql".to_string(),
            database_version: "mysql-8.4".to_string(),
            database_name: Some("g7_example_com".to_string()),
            database_user: Some("g7_app".to_string()),
            database_password: Some("0808dong!!".to_string()),
            database_password_confirm: Some("0808dong!!".to_string()),
            app_package: "gnuboard7".to_string(),
            site_user: "g7".to_string(),
            site_password: Some("0808dong!!".to_string()),
            site_password_confirm: Some("0808dong!!".to_string()),
            web_root_mode: "public-html".to_string(),
            web_root: Some("  ".to_string()),
            www_mode: "redirect-to-www".to_string(),
            redis: "enable".to_string(),
            mail_mode: "local-postfix".to_string(),
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
    fn public_wizard_rejects_unsupported_templates() {
        let mut request = setup_request("example.com");
        request.install_template = Some("frankenphp-octane".to_string());
        request.app_package = "gnuboard7".to_string();

        let error = super::validate_template_app_request(&request)
            .expect_err("must reject experimental template");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);

        request.install_template = Some("recommended".to_string());
        assert!(super::validate_template_app_request(&request).is_ok());
    }

    #[test]
    fn public_wizard_rejects_unsupported_runtime_and_apps() {
        let mut request = setup_request("example.com");
        request.web_server = "frankenphp".to_string();

        let error = super::validate_template_app_request(&request)
            .expect_err("must reject experimental runtime");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);

        request.web_server = "nginx".to_string();
        request.app_package = "laravel".to_string();
        let error =
            super::validate_template_app_request(&request).expect_err("must reject Laravel");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);

        request.app_package = "wordpress".to_string();
        assert!(super::validate_template_app_request(&request).is_ok());
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
                php_source: "ubuntu".to_string(),
                database_engine: "mysql".to_string(),
                database_name: "g7_test".to_string(),
                database_user: "g7_app".to_string(),
                database_password_policy: "user-provided-store-root-only",
                site_user: "g7".to_string(),
                web_root_mode: "public-html".to_string(),
                web_root: "/home/g7/public_html".to_string(),
                app_url: "http://g7-test.local/install".to_string(),
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
                    message: "설치 전에는 없던 패키지입니다. 이번 설치 대상입니다.".to_string(),
                }],
                package_checks: vec![install::InstallCheck {
                    name: "nginx".to_string(),
                    status: "pass".to_string(),
                    message: "installed".to_string(),
                }],
                service_checks: Vec::new(),
                port_checks: Vec::new(),
                network_checks: Vec::new(),
                runtime_checks: Vec::new(),
                database_checks: Vec::new(),
                firewall_checks: Vec::new(),
                mail_checks: Vec::new(),
                certbot_checks: Vec::new(),
                vhost_checks: Vec::new(),
                app_checks: Vec::new(),
                setup_guide_path: PathBuf::from("/var/log/g7-installer/setup-guide.md"),
                backup_manifest_path: PathBuf::from("/var/backups/g7-installer/manifest.json"),
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
        assert_eq!(install_api.app_url, "http://g7-test.local/install");
        assert_eq!(install_api.mail_mode, "none");
        assert!(!install_api.dns_check);
        assert_eq!(
            install_api.preinstall_package_checks[0].status,
            "not-installed"
        );
        assert_eq!(install_api.package_checks[0].name, "nginx");
        assert_eq!(install_api.state_path, "/var/lib/g7-installer/state.json");
        assert_eq!(
            install_api.backup_manifest_path,
            "/var/backups/g7-installer/manifest.json"
        );
        assert_eq!(
            install_api.owned_files,
            vec!["/etc/g7-installer/config.toml".to_string()]
        );

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
                actions: vec![reset::ResetAction {
                    name: "package:nginx".to_string(),
                    status: "purged".to_string(),
                    message: "package purged".to_string(),
                }],
                removed: vec!["/etc/g7-installer".to_string()],
                missing: vec!["/tmp/g7".to_string()],
            },
        });
        assert_eq!(rollback_api.package_actions[0].status, "removed");
        assert_eq!(rollback_api.metadata_reset.actions[0].status, "purged");
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

        let css = app_css(axum::extract::State(state.clone()), peer())
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

        let promo = promo_json(axum::extract::State(state), peer())
            .await
            .expect("promo json should be served")
            .into_response();
        assert_eq!(promo.status(), StatusCode::OK);
        assert_eq!(
            promo.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json; charset=utf-8"))
        );
        assert_eq!(
            promo.headers().get(header::CACHE_CONTROL),
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
        assert_eq!(payload["database_version"], "mysql-8.4");
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
        let api = super::plan_to_api(install_plan, "mysql-8.4".to_string());

        assert_eq!(api.domain, "example.com");
        assert_eq!(api.database_version, "mysql-8.4");
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
            section.name == "web-server"
                && section
                    .settings
                    .iter()
                    .any(|setting| setting.key == "apache_max_request_workers_by_ram")
        }));
        assert!(api.provisioning.iter().any(|section| {
            section.name == "web-server"
                && section
                    .settings
                    .iter()
                    .any(|setting| setting.key == "nginx_worker_processes_by_cpu_ram")
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
