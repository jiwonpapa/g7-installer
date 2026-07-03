//! Web setup controller for `g7inst setup`.
//!
//! This module owns the human-facing setup UX. It runs a short-lived local web
//! controller, serves the bundled HTML/CSS/JS assets, and delegates all install
//! policy to `g7_core::commands::plan` and mutating actions to core commands.
//! The controller must not expose arbitrary shell execution or invent install
//! defaults that do not exist in `plan.rs`.

use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{StatusCode, header};
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

pub const DEFAULT_BIND: &str = "127.0.0.1:7717";

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const APP_CSS: &str = include_str!("../../../web/dist/app.css");
const REPORT_PATH: &str = "/var/log/g7-installer/report.json";

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
}

#[derive(Debug, Serialize)]
struct BootstrapAuth {
    mode: &'static str,
    status: &'static str,
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
    println!("Stop: Ctrl+C");
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_err() {
        return;
    }
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
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
) -> impl IntoResponse {
    let payload = BootstrapPayload {
        domain: state.domain,
        local_test: state.local_test,
        auth: BootstrapAuth {
            mode: "token-pam-planned",
            status: "bootstrap-only",
        },
    };

    (StatusCode::OK, Json(payload))
}

async fn api_events(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<WebState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| event_socket(socket, state.events.subscribe()))
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
) -> impl IntoResponse {
    emit_log(&state, "running server check");
    let report = doctor_to_api(doctor::run());
    emit_log(
        &state,
        format!(
            "server check completed: install_allowed={}",
            report.install_allowed
        ),
    );

    Json(report)
}

async fn api_plan(
    axum::extract::State(state): axum::extract::State<WebState>,
    Json(request): Json<SetupRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    emit_log(&state, "building install plan");
    let domain = request.domain.clone();
    let options = options_from_request(request);
    let install_plan = plan::build_with_options(domain, options).map_err(ApiError::bad_request)?;
    emit_log(&state, "install plan ready");

    Ok(Json(plan_to_api(install_plan)))
}

async fn api_install_prepare(
    axum::extract::State(state): axum::extract::State<WebState>,
    Json(request): Json<SetupRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
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
    Json(request): Json<ResetRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
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

async fn api_status() -> impl IntoResponse {
    let current = status::read();

    Json(StatusApiReport {
        installed: current.installed,
        components: current
            .components
            .into_iter()
            .map(|component| ComponentApiStatus {
                name: component.name,
                state: component.state,
            })
            .collect(),
    })
}

async fn api_report() -> impl IntoResponse {
    match fs::read_to_string(REPORT_PATH) {
        Ok(content) => Json(ReportApiPayload {
            exists: true,
            path: REPORT_PATH,
            content,
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Json(ReportApiPayload {
            exists: false,
            path: REPORT_PATH,
            content: "report file does not exist yet".to_string(),
        }),
        Err(err) => Json(ReportApiPayload {
            exists: false,
            path: REPORT_PATH,
            content: format!("failed to read report: {err}"),
        }),
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

impl ApiError {
    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn conflict(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
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
    use super::{ensure_remote_binding_is_explicit, parse_bind, secure_token};

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
}
