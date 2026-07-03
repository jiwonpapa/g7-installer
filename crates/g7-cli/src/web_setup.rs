//! Web setup controller for `g7inst setup`.
//!
//! This module owns the human-facing setup UX. It runs a short-lived local web
//! controller, serves the bundled HTML/CSS/JS assets, and delegates all install
//! policy to `g7_core::commands::plan` and mutating actions to core commands.
//! The controller must not expose arbitrary shell execution or invent install
//! defaults that do not exist in `plan.rs`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use axum::extract::Query;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use g7_core::commands::{DoctorCheckStatus, doctor, install, plan, reset, status};
use getrandom::fill as fill_random;
use miette::{IntoDiagnostic, Result, miette};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::trace::TraceLayer;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub const DEFAULT_BIND: &str = "127.0.0.1:7717";

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const APP_CSS: &str = include_str!("../../../web/dist/app.css");
const REPORT_PATH: &str = "/var/log/g7-installer/report.json";
const SESSION_COOKIE: &str = "g7inst_session";
const CSRF_HEADER: &str = "x-g7-csrf";
const SESSION_TTL: Duration = Duration::from_secs(30 * 60);
const AUTH_TIMEOUT: Duration = Duration::from_secs(15);
const AUTH_MAX_FAILURES_BEFORE_LOCK: u32 = 3;
const AUTH_LOCKOUT: Duration = Duration::from_secs(60);
const NOBODY_UID: u32 = 65_534;
const NOBODY_GID: u32 = 65_534;

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
    loopback_bind: bool,
}

#[derive(Debug, Clone)]
struct Session {
    csrf_token: String,
    authenticated: bool,
    username: Option<String>,
    expires_at: Instant,
    failed_login_attempts: u32,
    login_blocked_until: Option<Instant>,
}

#[derive(Debug, Clone, Serialize)]
struct WebEvent {
    event_type: &'static str,
    message: String,
    stage: Option<&'static str>,
    status: Option<&'static str>,
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
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    authenticated: bool,
    username: String,
}

#[derive(Debug, Deserialize)]
struct SetupRequest {
    domain: String,
    local_test: bool,
    web_server: String,
    php_version: String,
    database: String,
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

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
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
    web_server: String,
    php_version: String,
    database: String,
    site_user: String,
    web_root: String,
    packages: Vec<NameDescription>,
    files: Vec<FilePlan>,
    services: Vec<ServicePlan>,
    ports: Vec<PortPlan>,
    security_checks: Vec<SecurityCheckPlan>,
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
struct InstallApiReport {
    domain: String,
    deployment_mode: String,
    web_server: String,
    php_version: String,
    database: String,
    site_user: String,
    web_root: String,
    phase: String,
    state_path: String,
    owned_files_path: String,
    completed_steps: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ResetApiReport {
    dry_run: bool,
    removed: Vec<String>,
    missing: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StatusApiReport {
    installed: bool,
    components: Vec<ComponentApiStatus>,
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
    let bind = parse_bind(&config.bind)?;
    ensure_remote_binding_is_explicit(bind, config.allow_remote)?;

    let state = WebState {
        access_token: secure_token()?,
        domain: config.domain,
        local_test: config.local_test,
        events: broadcast::channel(128).0,
        install_running: Arc::new(AtomicBool::new(false)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        loopback_bind: is_loopback(bind.ip()),
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
        .route("/api/auth/login", post(api_login))
        .route("/api/auth/logout", post(api_logout))
        .route("/api/events", get(api_events))
        .route("/api/doctor", get(api_doctor))
        .route("/api/plan", post(api_plan))
        .route("/api/install/prepare", post(api_install_prepare))
        .route("/api/reset", post(api_reset))
        .route("/api/status", get(api_status))
        .route("/api/report", get(api_report))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    axum::serve(listener, app)
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
    println!("G7inst Web Controller");
    println!("Open: http://{addr}/?token={token}");
    println!("Remote access:");
    println!("ssh -L 7717:127.0.0.1:7717 root@SERVER_IP");
    println!("If server account password is not set:");
    println!("sudo passwd root");
    println!("Stop: Ctrl+C");
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn index(
    axum::extract::State(state): axum::extract::State<WebState>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mut response = Html(INDEX_HTML).into_response();

    if query
        .get("token")
        .is_some_and(|token| secure_eq(token, &state.access_token))
    {
        match create_session(&state) {
            Ok(session_id) => {
                if let Ok(value) = HeaderValue::from_str(&session_cookie(&session_id)) {
                    response.headers_mut().insert(header::SET_COOKIE, value);
                }
            }
            Err(error) => return error.into_response(),
        }
    }

    response
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        APP_JS,
    )
}

async fn app_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS)
}

async fn bootstrap(
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers)?;
    let payload = BootstrapPayload {
        domain: state.domain,
        local_test: state.local_test,
        auth: BootstrapAuth {
            mode: "server-account",
            status: if session.authenticated {
                "authenticated"
            } else {
                "token-accepted"
            },
            username: session.username.clone(),
            authenticated: session.authenticated,
        },
        csrf_token: session.csrf_token,
    };

    Ok((StatusCode::OK, Json(payload)))
}

async fn api_login(
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session_id = require_session_id(&headers)?;
    let session = require_session(&state, &headers)?;
    require_csrf(&headers, &session)?;
    require_loopback_login(&state)?;
    require_login_allowed(&session)?;

    let username = normalize_login_username(&request.username)?;
    let password = request.password;
    let auth_username = username.clone();
    let auth_result = tokio::task::spawn_blocking(move || {
        verify_server_account_password(&auth_username, &password)
    })
    .await
    .map_err(|err| ApiError::bad_request(format!("authentication worker failed: {err}")))?;
    if let Err(message) = auth_result {
        record_login_failure(&state, &session_id)?;
        return Err(ApiError::unauthorized(message));
    }

    if !account_can_install(&username) {
        record_login_failure(&state, &session_id)?;
        return Err(ApiError::forbidden(
            "root or sudo-capable server account is required",
        ));
    }

    mark_session_authenticated(&state, &session_id, &username)?;
    emit_log(&state, format!("server account authenticated: {username}"));

    Ok(Json(LoginResponse {
        authenticated: true,
        username,
    }))
}

async fn api_logout(
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers)?;
    require_csrf(&headers, &session)?;
    let session_id = require_session_id(&headers)?;
    remove_session(&state, &session_id)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn api_events(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Err(error) = require_session(&state, &headers) {
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
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_session(&state, &headers)?;
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
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers)?;
    require_csrf(&headers, &session)?;
    emit_log(&state, "building install plan");
    let domain = request.domain.clone();
    let options = options_from_request(request);
    let install_plan = plan::build_with_options(domain, options).map_err(ApiError::bad_request)?;
    emit_log(&state, "install plan ready");

    Ok(Json(plan_to_api(install_plan)))
}

async fn api_install_prepare(
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers)?;
    require_csrf(&headers, &session)?;

    if state.install_running.swap(true, Ordering::SeqCst) {
        emit_log(&state, "install request rejected: already running");
        return Err(ApiError::conflict("install is already running"));
    }

    emit_stage(&state, "preflight", "진행", "preflight started");
    let domain = request.domain.clone();
    let options = options_from_request(request);
    let result = install::run(domain, options);
    state.install_running.store(false, Ordering::SeqCst);

    match result {
        Ok(report) => {
            emit_stage(&state, "preflight", "성공", "preflight passed");
            emit_stage(&state, "config", "성공", "configuration prepared");
            emit_stage(&state, "report", "성공", "problem report prepared");
            emit_log(&state, "install preparation completed");
            Ok(Json(install_to_api(report)))
        }
        Err(error) => {
            emit_stage(
                &state,
                "preflight",
                "실패",
                format!("install failed: {error}"),
            );
            Err(ApiError::bad_request(error))
        }
    }
}

async fn api_reset(
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
    Json(request): Json<ResetRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers)?;
    require_csrf(&headers, &session)?;

    if state.install_running.load(Ordering::SeqCst) {
        return Err(ApiError::conflict(
            "reset is blocked while install is running",
        ));
    }

    emit_log(&state, "running reset");
    let report = reset::run(true, request.dry_run).map_err(ApiError::bad_request)?;
    emit_log(&state, "reset completed");

    Ok(Json(ResetApiReport {
        dry_run: report.dry_run,
        removed: report.removed,
        missing: report.missing,
    }))
}

async fn api_status(
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers)?;
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

async fn api_report(
    axum::extract::State(state): axum::extract::State<WebState>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers)?;

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

fn options_from_request(request: SetupRequest) -> plan::PlanOptions {
    crate::plan_options(
        request.local_test,
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

fn plan_to_api(install_plan: plan::InstallPlan) -> PlanApiReport {
    let text = crate::format_plan(&install_plan);

    PlanApiReport {
        text,
        domain: install_plan.domain,
        deployment_mode: install_plan.deployment_mode,
        web_server: install_plan.web_server,
        php_version: install_plan.php_version,
        database: install_plan.database_engine,
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
        stop_conditions: install_plan
            .stop_conditions
            .into_iter()
            .map(|condition| condition.reason)
            .collect(),
    }
}

fn install_to_api(report: install::InstallReport) -> InstallApiReport {
    InstallApiReport {
        domain: report.domain,
        deployment_mode: report.deployment_mode,
        web_server: report.web_server,
        php_version: report.php_version,
        database: report.database_engine,
        site_user: report.site_user,
        web_root: report.web_root,
        phase: report.phase,
        state_path: report.state_path.display().to_string(),
        owned_files_path: report.owned_files_path.display().to_string(),
        completed_steps: report.completed_steps,
    }
}

fn create_session(state: &WebState) -> std::result::Result<String, ApiError> {
    let session_id = secure_token().map_err(ApiError::bad_request)?;
    let csrf_token = secure_token().map_err(ApiError::bad_request)?;
    let session = Session {
        csrf_token,
        authenticated: false,
        username: None,
        expires_at: Instant::now() + SESSION_TTL,
        failed_login_attempts: 0,
        login_blocked_until: None,
    };

    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;
    sessions.insert(session_id.clone(), session);

    Ok(session_id)
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
) -> std::result::Result<Session, ApiError> {
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
    session.expires_at = now + SESSION_TTL;

    Ok(session.clone())
}

fn require_authenticated_session(
    state: &WebState,
    headers: &HeaderMap,
) -> std::result::Result<Session, ApiError> {
    let session = require_session(state, headers)?;
    if session.authenticated {
        Ok(session)
    } else {
        Err(ApiError::unauthorized("server account login is required"))
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

fn mark_session_authenticated(
    state: &WebState,
    session_id: &str,
    username: &str,
) -> std::result::Result<(), ApiError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;
    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| ApiError::unauthorized("setup session expired or invalid"))?;
    session.authenticated = true;
    session.username = Some(username.to_string());
    session.expires_at = Instant::now() + SESSION_TTL;
    session.failed_login_attempts = 0;
    session.login_blocked_until = None;

    Ok(())
}

fn record_login_failure(state: &WebState, session_id: &str) -> std::result::Result<(), ApiError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;
    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| ApiError::unauthorized("setup session expired or invalid"))?;
    session.failed_login_attempts = session.failed_login_attempts.saturating_add(1);

    if session.failed_login_attempts >= AUTH_MAX_FAILURES_BEFORE_LOCK {
        session.login_blocked_until = Some(Instant::now() + AUTH_LOCKOUT);
    }

    Ok(())
}

fn require_login_allowed(session: &Session) -> std::result::Result<(), ApiError> {
    if session
        .login_blocked_until
        .is_some_and(|blocked_until| blocked_until > Instant::now())
    {
        Err(ApiError::too_many_requests(
            "too many failed login attempts; wait 60 seconds and retry",
        ))
    } else {
        Ok(())
    }
}

fn require_loopback_login(state: &WebState) -> std::result::Result<(), ApiError> {
    if state.loopback_bind {
        return Ok(());
    }

    Err(ApiError::forbidden(
        "server account password login is disabled on remote bind; use the default 127.0.0.1 bind with an SSH tunnel",
    ))
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

fn normalize_login_username(username: &str) -> std::result::Result<String, ApiError> {
    let username = username.trim();
    let safe = !username.is_empty()
        && username.len() <= 32
        && username
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-'));

    if safe {
        Ok(username.to_string())
    } else {
        Err(ApiError::bad_request("invalid server account name"))
    }
}

#[cfg(unix)]
fn verify_server_account_password(
    username: &str,
    password: &str,
) -> std::result::Result<(), String> {
    if password.is_empty() {
        return Err("server account password is required".to_string());
    }

    let mut child = Command::new("su")
        .arg("--login")
        .arg(username)
        .arg("--command")
        .arg("true")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .uid(NOBODY_UID)
        .gid(NOBODY_GID)
        .spawn()
        .map_err(|err| format!("failed to start server account verifier: {err}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(format!("{password}\n").as_bytes())
            .map_err(|err| format!("failed to send password to verifier: {err}"))?;
    }

    let deadline = Instant::now() + AUTH_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("server account authentication failed".to_string()),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("server account authentication timed out".to_string());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(err) => return Err(format!("failed to wait for verifier: {err}")),
        }
    }
}

#[cfg(not(unix))]
fn verify_server_account_password(
    _username: &str,
    _password: &str,
) -> std::result::Result<(), String> {
    Err("server account authentication is supported on Unix-like VPS targets only".to_string())
}

fn account_can_install(username: &str) -> bool {
    username == "root" || user_in_admin_group(username)
}

fn user_in_admin_group(username: &str) -> bool {
    let Ok(content) = fs::read_to_string("/etc/group") else {
        return false;
    };

    content.lines().any(|line| {
        let mut parts = line.split(':');
        let group = parts.next();
        let _password = parts.next();
        let _gid = parts.next();
        let members = parts.next();

        matches!(group, Some("sudo" | "admin"))
            && members
                .map(|members| members.split(',').any(|member| member == username))
                .unwrap_or(false)
    })
}

impl ApiError {
    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn unauthorized(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: error.into(),
        }
    }

    fn forbidden(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: error.into(),
        }
    }

    fn conflict(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: error.into(),
        }
    }

    fn too_many_requests(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: error.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
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
    });
}

#[cfg(test)]
mod tests {
    use super::{
        AUTH_LOCKOUT, SESSION_TTL, Session, ensure_remote_binding_is_explicit,
        normalize_login_username, parse_bind, require_login_allowed, secure_eq, secure_token,
    };
    use axum::http::StatusCode;
    use std::time::Instant;

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
    fn login_username_accepts_safe_server_account_names() {
        assert!(matches!(
            normalize_login_username("root").as_deref(),
            Ok("root")
        ));
        assert!(matches!(
            normalize_login_username("ubuntu-admin").as_deref(),
            Ok("ubuntu-admin")
        ));
        assert!(normalize_login_username("../root").is_err());
        assert!(normalize_login_username("").is_err());
    }

    #[test]
    fn login_lockout_returns_rate_limit_error() {
        let session = Session {
            csrf_token: "csrf".to_string(),
            authenticated: false,
            username: None,
            expires_at: Instant::now() + SESSION_TTL,
            failed_login_attempts: 3,
            login_blocked_until: Some(Instant::now() + AUTH_LOCKOUT),
        };

        let error = require_login_allowed(&session).expect_err("lockout should reject login");
        assert_eq!(error.status, StatusCode::TOO_MANY_REQUESTS);
    }
}
