use super::*;

pub struct WebSetupConfig {
    pub domain: Option<String>,
    pub local_test: bool,
    pub bind: String,
    pub allow_remote: bool,
}

#[derive(Debug, Clone)]
pub(super) struct WebState {
    pub(super) access_token: String,
    pub(super) domain: Option<String>,
    pub(super) local_test: bool,
    pub(super) events: broadcast::Sender<WebEvent>,
    pub(super) event_history: Arc<Mutex<VecDeque<WebEvent>>>,
    pub(super) event_sequence: Arc<AtomicU64>,
    pub(super) install_running: Arc<AtomicBool>,
    pub(super) sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub(super) allowed_client_ip: Arc<Mutex<Option<IpAddr>>>,
}

#[derive(Debug, Clone)]
pub(super) struct Session {
    pub(super) csrf_token: String,
    pub(super) authenticated: bool,
    pub(super) username: Option<String>,
    pub(super) client_ip: IpAddr,
    pub(super) expires_at: Instant,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct WebEvent {
    pub(super) seq: u64,
    pub(super) timestamp_unix_ms: u128,
    pub(super) event_type: &'static str,
    pub(super) message: String,
    pub(super) stage: Option<&'static str>,
    pub(super) status: Option<&'static str>,
    pub(super) operation: Option<&'static str>,
    pub(super) percent: Option<u8>,
    pub(super) operation_id: Option<String>,
    pub(super) stream: Option<&'static str>,
    pub(super) command: Option<String>,
    pub(super) elapsed_ms: Option<u128>,
}

struct InstallRunningGuard(Arc<AtomicBool>);

impl Drop for InstallRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Serialize)]
pub(super) struct BootstrapPayload {
    pub(super) domain: Option<String>,
    pub(super) local_test: bool,
    pub(super) auth: BootstrapAuth,
    pub(super) csrf_token: String,
}

#[derive(Debug, Serialize)]
pub(super) struct BootstrapAuth {
    pub(super) mode: &'static str,
    pub(super) status: &'static str,
    pub(super) username: Option<String>,
    pub(super) authenticated: bool,
    pub(super) client_ip: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct SetupRequest {
    pub(super) domain: String,
    #[serde(default)]
    pub(super) local_test: bool,
    #[serde(default)]
    pub(super) install_template: Option<String>,
    pub(super) web_server: String,
    pub(super) php_version: String,
    #[serde(default = "default_php_source")]
    pub(super) php_source: String,
    pub(super) database: String,
    pub(super) database_version: String,
    #[serde(default)]
    pub(super) database_name: Option<String>,
    #[serde(default)]
    pub(super) database_user: Option<String>,
    #[serde(default)]
    pub(super) database_password: Option<String>,
    #[serde(default)]
    pub(super) database_password_confirm: Option<String>,
    pub(super) app_package: String,
    pub(super) site_user: String,
    #[serde(default)]
    pub(super) site_password: Option<String>,
    #[serde(default)]
    pub(super) site_password_confirm: Option<String>,
    pub(super) web_root_mode: String,
    pub(super) web_root: Option<String>,
    pub(super) www_mode: String,
    pub(super) redis: String,
    pub(super) mail_mode: String,
    pub(super) smtp_host: Option<String>,
    pub(super) smtp_port: u16,
    pub(super) smtp_from: Option<String>,
    #[serde(default)]
    pub(super) smtp_username: Option<String>,
    #[serde(default)]
    pub(super) smtp_password: Option<String>,
    #[serde(default)]
    pub(super) smtp_password_confirm: Option<String>,
    pub(super) smtp_encryption: String,
    pub(super) security_profile: String,
    pub(super) ssh_policy: String,
    pub(super) rollback: bool,
    pub(super) preserve_config: bool,
    pub(super) dns_check: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct ResetRequest {
    #[serde(default)]
    pub(super) dry_run: bool,
    #[serde(default)]
    pub(super) confirmation: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct RollbackRequest {
    #[serde(default)]
    pub(super) dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProvisionActionRequest {
    pub(super) action: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ProvisionActionReport {
    pub(super) action: String,
    pub(super) status: String,
    pub(super) message: String,
    pub(super) checks: Vec<InstallApiCheck>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiErrorBody {
    pub(super) error: String,
    pub(super) hint: Option<String>,
    pub(super) details: Vec<String>,
    pub(super) retryable: bool,
}

#[derive(Debug)]
pub(super) struct ApiError {
    pub(super) status: StatusCode,
    pub(super) message: String,
    pub(super) hint: Option<String>,
    pub(super) details: Vec<String>,
    pub(super) retryable: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct DoctorApiReport {
    pub(super) install_allowed: bool,
    pub(super) checks: Vec<DoctorApiCheck>,
    pub(super) resources: DoctorApiResources,
}

#[derive(Debug, Serialize)]
pub(super) struct DoctorApiResources {
    pub(super) total_memory_mib: Option<u64>,
    pub(super) available_memory_mib: Option<u64>,
    pub(super) swap_total_mib: Option<u64>,
    pub(super) root_available_mib: Option<u64>,
    pub(super) root_inode_free_percent: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(super) struct DoctorApiCheck {
    pub(super) name: &'static str,
    pub(super) status: &'static str,
    pub(super) message: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PlanApiReport {
    pub(super) text: String,
    pub(super) domain: String,
    pub(super) deployment_mode: String,
    pub(super) app_profile: String,
    pub(super) app_profile_label: &'static str,
    pub(super) app_document_root: String,
    pub(super) web_server: String,
    pub(super) php_version: String,
    pub(super) php_source: String,
    pub(super) database: String,
    pub(super) database_version: String,
    pub(super) database_name: String,
    pub(super) database_user: String,
    pub(super) database_password_policy: &'static str,
    pub(super) app_package: String,
    pub(super) site_user: String,
    pub(super) web_root: String,
    pub(super) packages: Vec<NameDescription>,
    pub(super) files: Vec<FilePlan>,
    pub(super) services: Vec<ServicePlan>,
    pub(super) ports: Vec<PortPlan>,
    pub(super) security_checks: Vec<SecurityCheckPlan>,
    pub(super) app_requirements: Vec<RequirementPlan>,
    pub(super) app_followup_steps: Vec<FollowupStepPlan>,
    pub(super) provisioning: Vec<ProvisioningSectionPlan>,
    pub(super) stop_conditions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct NameDescription {
    pub(super) name: String,
    pub(super) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct FilePlan {
    pub(super) path: String,
    pub(super) action: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct ServicePlan {
    pub(super) name: String,
    pub(super) action: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct PortPlan {
    pub(super) port: u16,
    pub(super) protocol: &'static str,
    pub(super) purpose: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct SecurityCheckPlan {
    pub(super) name: &'static str,
    pub(super) level: &'static str,
    pub(super) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct RequirementPlan {
    pub(super) name: String,
    pub(super) status: &'static str,
    pub(super) message: String,
}

#[derive(Debug, Serialize)]
pub(super) struct FollowupStepPlan {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct ProvisioningSectionPlan {
    pub(super) name: &'static str,
    pub(super) title: &'static str,
    pub(super) summary: String,
    pub(super) settings: Vec<ProvisioningSettingPlan>,
}

#[derive(Debug, Serialize)]
pub(super) struct ProvisioningSettingPlan {
    pub(super) key: &'static str,
    pub(super) value: String,
}

#[derive(Debug, Serialize)]
pub(super) struct InstallApiReport {
    pub(super) domain: String,
    pub(super) deployment_mode: String,
    pub(super) app_profile: String,
    pub(super) app_profile_label: &'static str,
    pub(super) app_document_root: String,
    pub(super) web_server: String,
    pub(super) php_version: String,
    pub(super) php_source: String,
    pub(super) database: String,
    pub(super) database_version: String,
    pub(super) database_name: String,
    pub(super) database_user: String,
    pub(super) database_password_policy: &'static str,
    pub(super) app_package: String,
    pub(super) site_user: String,
    pub(super) web_root_mode: String,
    pub(super) web_root: String,
    pub(super) app_url: String,
    pub(super) www_mode: String,
    pub(super) redis: String,
    pub(super) mail_mode: String,
    pub(super) smtp_host: Option<String>,
    pub(super) smtp_port: Option<u16>,
    pub(super) smtp_from: Option<String>,
    pub(super) smtp_username: Option<String>,
    pub(super) smtp_password_policy: &'static str,
    pub(super) smtp_encryption: Option<String>,
    pub(super) dns_check: bool,
    pub(super) security_profile: String,
    pub(super) ssh_policy: String,
    pub(super) phase: String,
    pub(super) state_path: String,
    pub(super) owned_files_path: String,
    pub(super) owned_files: Vec<String>,
    pub(super) completed_steps: Vec<String>,
    pub(super) safety_checks: Vec<InstallApiCheck>,
    pub(super) preinstall_package_checks: Vec<InstallApiCheck>,
    pub(super) package_checks: Vec<InstallApiCheck>,
    pub(super) service_checks: Vec<InstallApiCheck>,
    pub(super) port_checks: Vec<InstallApiCheck>,
    pub(super) network_checks: Vec<InstallApiCheck>,
    pub(super) runtime_checks: Vec<InstallApiCheck>,
    pub(super) database_checks: Vec<InstallApiCheck>,
    pub(super) firewall_checks: Vec<InstallApiCheck>,
    pub(super) mail_checks: Vec<InstallApiCheck>,
    pub(super) certbot_checks: Vec<InstallApiCheck>,
    pub(super) vhost_checks: Vec<InstallApiCheck>,
    pub(super) app_checks: Vec<InstallApiCheck>,
    pub(super) setup_guide_path: String,
    pub(super) backup_manifest_path: String,
    pub(super) app_requirements: Vec<InstallApiCheck>,
}

#[derive(Debug, Serialize)]
pub(super) struct InstallApiCheck {
    pub(super) name: String,
    pub(super) status: String,
    pub(super) message: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ResetApiReport {
    pub(super) dry_run: bool,
    pub(super) actions: Vec<ResetApiAction>,
    pub(super) removed: Vec<String>,
    pub(super) missing: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ResetApiAction {
    pub(super) name: String,
    pub(super) status: String,
    pub(super) message: String,
}

#[derive(Debug, Serialize)]
pub(super) struct RollbackApiReport {
    pub(super) dry_run: bool,
    pub(super) phase: String,
    pub(super) package_actions: Vec<RollbackApiAction>,
    pub(super) service_actions: Vec<RollbackApiAction>,
    pub(super) metadata_reset: ResetApiReport,
}

#[derive(Debug, Serialize)]
pub(super) struct RollbackApiAction {
    pub(super) name: String,
    pub(super) status: String,
    pub(super) message: String,
}

#[derive(Debug, Serialize)]
pub(super) struct StatusApiReport {
    pub(super) installed: bool,
    pub(super) install_running: bool,
    pub(super) domain: Option<String>,
    pub(super) phase: Option<String>,
    pub(super) components: Vec<ComponentApiStatus>,
    pub(super) problems: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RecoveryApiStatus {
    pub(super) can_resume: bool,
    pub(super) can_retry_step: bool,
    pub(super) can_reset: bool,
    pub(super) can_rollback: bool,
    pub(super) recommended_action: &'static str,
    pub(super) failed_step: Option<String>,
    pub(super) restore_status: Option<String>,
    pub(super) message: String,
    pub(super) metadata_paths: Vec<String>,
    pub(super) rollback_reason: Option<String>,
    pub(super) resume_reason: Option<String>,
    pub(super) g7_database_created: bool,
    pub(super) g7_database_confirmed: Option<bool>,
    pub(super) g7_database_name: Option<String>,
    pub(super) server_configured: bool,
    pub(super) app_files_prepared: bool,
    pub(super) g7_install_completed: bool,
    pub(super) g7_install_lock_path: Option<String>,
    pub(super) app_install_url: Option<String>,
    pub(super) lifecycle_status: &'static str,
}

#[derive(Debug, Deserialize)]
pub(super) struct ResumeRequest {}

#[derive(Debug, Serialize)]
pub(super) struct ComponentApiStatus {
    pub(super) name: String,
    pub(super) state: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ReportApiPayload {
    pub(super) exists: bool,
    pub(super) path: &'static str,
    pub(super) content: String,
}

pub(super) async fn api_doctor(
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

pub(super) async fn api_plan(
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
    validate_mail_request(&request)?;
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

pub(super) async fn api_install_prepare(
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
    validate_mail_request(&request)?;

    if state.install_running.swap(true, Ordering::SeqCst) {
        emit_log(&state, "설치 요청 거부: 이미 다른 설치 작업이 진행 중");
        return Err(ApiError::conflict("install is already running"));
    }

    emit_progress(&state, "install", 5, "install progress: starting preflight");
    emit_stage(&state, "preflight", "진행", "preflight started");
    let domain = request.domain.clone();
    let options = options_from_request(request);
    emit_progress(
        &state,
        "install",
        15,
        "install progress: running server install",
    );
    let operation_id = format!(
        "install-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis())
    );
    let worker_state = state.clone();
    let running = state.install_running.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _running_guard = InstallRunningGuard(running);
        let observer = Arc::new(WebCommandObserver::new(
            worker_state,
            "install",
            operation_id,
        ));
        let probe = SystemProbe::new(RealCommandRunner::with_observer(observer));
        install::run_with_probe_and_paths(domain, options, &probe, &install::InstallPaths::system())
    })
    .await
    .map_err(|error| {
        state.install_running.store(false, Ordering::SeqCst);
        ApiError::bad_request(format!("설치 작업 실행기가 중단되었습니다: {error}"))
            .with_hint("상태와 최근 실시간 로그를 확인한 뒤 이어서 진행하거나 초기화하세요.")
    })?;

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
            emit_stage(&state, "runtime", "성공", "PHP runtime configured");
            emit_progress(
                &state,
                "install",
                54,
                "install progress: runtime configured",
            );
            emit_stage(
                &state,
                "vhost",
                "성공",
                "web server vhost and HTTP smoke verified",
            );
            emit_progress(&state, "install", 66, "install progress: vhost verified");
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
            Ok(Json(install_to_api(report)))
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

pub(super) async fn api_resume(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(_request): Json<ResumeRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;
    if state.install_running.swap(true, Ordering::SeqCst) {
        return Err(ApiError::conflict("another installer operation is running"));
    }

    emit_log(&state, "중단된 설치 이어서 진행");
    emit_progress(&state, "resume", 5, "resume progress: starting");
    let operation_id = format!(
        "resume-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis())
    );
    let worker_state = state.clone();
    let running = state.install_running.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _running_guard = InstallRunningGuard(running);
        let observer = Arc::new(WebCommandObserver::new(
            worker_state,
            "resume",
            operation_id,
        ));
        let probe = SystemProbe::new(RealCommandRunner::with_observer(observer));
        install::resume_with_probe_and_paths(&probe, &install::InstallPaths::system())
    })
    .await
    .map_err(|error| {
        state.install_running.store(false, Ordering::SeqCst);
        ApiError::bad_request(format!("설치 이어서 진행 작업이 중단되었습니다: {error}"))
    })?;

    match result {
        Ok(report) => {
            emit_progress(&state, "resume", 100, "resume progress: completed");
            emit_log(&state, "설치 이어서 진행 완료");
            Ok(Json(install_to_api(report)))
        }
        Err(error) => {
            emit_progress(&state, "resume", 100, "resume progress: failed");
            emit_log(&state, format!("설치 이어서 진행 실패: {error}"));
            Err(ApiError::bad_request(error)
                .with_hint("최근 실시간 로그와 저장된 리포트의 실패 단계를 확인하세요."))
        }
    }
}

pub(super) async fn api_provision_action(
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

    let _operation_lock = g7_state::lock::InstallerLock::acquire(
        std::path::Path::new(g7_state::lock::LOCK_PATH),
        "provision",
    )
    .map_err(|error| {
        ApiError::conflict(format!("another installer operation is running: {error}"))
    })?;

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

pub(super) async fn api_reset(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<ResetRequest>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_authenticated_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;

    if request.confirmation.trim() != "초기화" {
        return Err(
            ApiError::bad_request("재설치 초기화 확인 문구가 일치하지 않습니다.")
                .with_hint("초기화를 실행하려면 확인 입력란에 `초기화`를 정확히 입력하세요."),
        );
    }

    if state.install_running.swap(true, Ordering::SeqCst) {
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
    let worker_state = state.clone();
    let running = state.install_running.clone();
    let dry_run = request.dry_run;
    let operation_id = format!(
        "reset-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis())
    );
    let report = tokio::task::spawn_blocking(move || {
        let _running_guard = InstallRunningGuard(running);
        let observer = Arc::new(WebCommandObserver::new(worker_state, "reset", operation_id));
        let probe = SystemProbe::new(RealCommandRunner::with_observer(observer));
        reset::run_with_probe_and_paths(true, dry_run, &probe, &reset::ResetPaths::system())
    })
    .await
    .map_err(|error| {
        state.install_running.store(false, Ordering::SeqCst);
        ApiError::bad_request(format!("초기화 작업 실행기가 중단되었습니다: {error}"))
    })?;
    let report = match report {
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

pub(super) async fn api_rollback(
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
    let worker_state = state.clone();
    let running = state.install_running.clone();
    let dry_run = request.dry_run;
    let operation_id = format!(
        "rollback-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis())
    );
    let report = tokio::task::spawn_blocking(move || {
        let _running_guard = InstallRunningGuard(running);
        let observer = Arc::new(WebCommandObserver::new(
            worker_state,
            "rollback",
            operation_id,
        ));
        let probe = SystemProbe::new(RealCommandRunner::with_observer(observer));
        rollback::run_with_probe_and_paths(
            true,
            dry_run,
            &probe,
            &rollback::RollbackPaths::system(),
        )
    })
    .await
    .map_err(|error| {
        state.install_running.store(false, Ordering::SeqCst);
        ApiError::bad_request(format!("되돌리기 작업 실행기가 중단되었습니다: {error}"))
    })?;

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

pub(super) async fn api_status(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers, peer.ip())?;
    let current = status::read();

    Ok(Json(StatusApiReport {
        installed: current.installed,
        install_running: state.install_running.load(Ordering::SeqCst),
        domain: current.domain,
        phase: current.phase,
        components: current
            .components
            .into_iter()
            .map(|component| ComponentApiStatus {
                name: component.name,
                state: component.state,
            })
            .collect(),
        problems: current.problems,
    }))
}

pub(super) async fn api_recovery(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers, peer.ip())?;

    Ok(Json(recovery_status()))
}

pub(super) async fn api_report(
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

pub(super) fn options_from_request(request: SetupRequest) -> plan::PlanOptions {
    crate::plan_options(
        request.local_test,
        request.app_package,
        request.web_server,
        request.php_version,
        request.php_source,
        request.database,
        request.database_version,
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
        request
            .smtp_username
            .filter(|value| !value.trim().is_empty()),
        request
            .smtp_password
            .filter(|value| !value.trim().is_empty()),
        request.smtp_encryption,
        request.security_profile,
        request.ssh_policy,
        request.rollback,
        request.preserve_config,
        request.dns_check,
    )
}

pub(super) fn default_php_source() -> String {
    plan::DEFAULT_PHP_SOURCE.to_string()
}

pub(super) fn validate_template_app_request(
    request: &SetupRequest,
) -> std::result::Result<(), ApiError> {
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

    if request.database != "mysql" {
        return Err(ApiError::bad_request("공개 설치기는 MySQL만 지원합니다.")
            .with_hint("데이터베이스를 MySQL로 선택하세요."));
    }

    if !matches!(request.database_version.as_str(), "8.0" | "8.4") {
        return Err(ApiError::bad_request("지원하지 않는 MySQL 버전입니다.")
            .with_hint("MySQL 8.0 또는 MySQL 8.4 LTS를 선택하세요."));
    }

    if app_package != "gnuboard7" {
        return Err(
            ApiError::bad_request("공개 설치기는 그누보드7 앱만 지원합니다.")
                .with_hint("설치할 앱을 그누보드7로 바꾸세요."),
        );
    }

    Ok(())
}

pub(super) fn validate_site_password_request(
    request: &SetupRequest,
) -> std::result::Result<(), ApiError> {
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

pub(super) fn validate_database_request(
    request: &SetupRequest,
) -> std::result::Result<(), ApiError> {
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

pub(super) fn validate_mail_request(request: &SetupRequest) -> std::result::Result<(), ApiError> {
    if request.mail_mode != "smtp-relay" {
        return Ok(());
    }
    let username = request.smtp_username.as_deref().unwrap_or("").trim();
    let password = request.smtp_password.as_deref().unwrap_or("");
    let confirm = request.smtp_password_confirm.as_deref().unwrap_or("");
    if username.is_empty() {
        return Err(ApiError::bad_request("SMTP 계정을 입력하세요."));
    }
    if password.is_empty() {
        return Err(ApiError::bad_request("SMTP 비밀번호를 입력하세요."));
    }
    if password != confirm {
        return Err(ApiError::bad_request(
            "SMTP 비밀번호 확인이 일치하지 않습니다.",
        ));
    }
    if password.len() < 8 {
        return Err(ApiError::bad_request(
            "SMTP 비밀번호는 8자 이상이어야 합니다.",
        ));
    }
    if username
        .chars()
        .any(|ch| ch == '"' || ch == '\\' || ch == '\n' || ch == '\r' || ch.is_control())
        || password
            .chars()
            .any(|ch| ch == '"' || ch == '\\' || ch == '\n' || ch == '\r' || ch.is_control())
    {
        return Err(ApiError::bad_request(
            "SMTP 인증 정보에 큰따옴표, 백슬래시, 줄바꿈, 제어문자를 사용할 수 없습니다.",
        ));
    }
    Ok(())
}

pub(super) fn is_database_identifier(value: &str, max_len: usize) -> bool {
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
