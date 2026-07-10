use super::{
    CSRF_HEADER, DoctorCheckStatus, REPORT_PATH, SESSION_COOKIE, SESSION_TTL, Session,
    SetupRequest, WebState, api_install_prepare, api_plan, api_recovery, api_report, api_reset,
    api_rollback, api_status, app_css, app_js, bootstrap, browser_addr_for, create_session,
    doctor_status_label, doctor_to_api, emit_log, ensure_remote_binding_is_explicit,
    event_history_snapshot, failed_doctor_details, index, install_checks_to_api, install_to_api,
    lock_client_ip, options_from_request, parse_bind, promo_json, remove_session,
    require_allowed_client_ip, require_authenticated_session, require_csrf, require_session,
    require_session_id, rollback_to_api, secure_eq, secure_token, session_cookie,
};
use axum::Json;
use axum::body::to_bytes;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use g7_core::commands::doctor::{DoctorCheck, DoctorReport};
use g7_core::commands::{install, plan, reset, rollback};
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

fn test_state() -> WebState {
    WebState {
        access_token: "token".to_string(),
        domain: None,
        local_test: true,
        events: broadcast::channel(16).0,
        event_history: Arc::new(Mutex::new(VecDeque::new())),
        event_sequence: Arc::new(AtomicU64::new(0)),
        install_running: Arc::new(AtomicBool::new(false)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        allowed_client_ip: Arc::new(Mutex::new(None)),
    }
}

fn peer() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 49152))
}

#[test]
fn web_events_keep_ordered_reconnect_history() {
    let state = test_state();
    let first = emit_log(&state, "first");
    let second = emit_log(&state, "second");

    assert!(second.seq > first.seq);
    assert_eq!(
        event_history_snapshot(&state, first.seq)
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>(),
        vec!["second"]
    );
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
        database_password: Some("Test-only_9x!".to_string()),
        database_password_confirm: Some("Test-only_9x!".to_string()),
        app_package: "gnuboard7".to_string(),
        site_user: "g7".to_string(),
        site_password: Some("Test-only_9x!".to_string()),
        site_password_confirm: Some("Test-only_9x!".to_string()),
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
    let session_id =
        create_session(state, IpAddr::V4(Ipv4Addr::LOCALHOST)).expect("session should be created");
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
fn setup_token_is_hex_encoded_32_bytes() -> std::result::Result<(), Box<dyn std::error::Error>> {
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
        resources: Default::default(),
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
fn session_lifecycle_authentication_and_csrf() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let state = test_state();
    let session_id =
        create_session(&state, IpAddr::V4(Ipv4Addr::LOCALHOST)).expect("session should be created");
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
    let session = require_authenticated_session(&state, &headers, IpAddr::V4(Ipv4Addr::LOCALHOST))
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
    let error = super::validate_template_app_request(&request).expect_err("must reject Laravel");
    assert_eq!(error.status, StatusCode::BAD_REQUEST);

    request.app_package = "wordpress".to_string();
    assert!(super::validate_template_app_request(&request).is_ok());
}

#[test]
fn doctor_conversion_preserves_status_labels() {
    let report = doctor_to_api(DoctorReport {
        install_allowed: false,
        resources: Default::default(),
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
    let session_id =
        create_session(&state, IpAddr::V4(Ipv4Addr::LOCALHOST)).expect("session should be created");
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

    let status_response = api_status(axum::extract::State(state.clone()), peer(), headers.clone())
        .await
        .expect("status should respond")
        .into_response();
    let status_payload = response_json(status_response).await;
    assert_eq!(status_payload["installed"], false);

    let report_response = api_report(axum::extract::State(state.clone()), peer(), headers.clone())
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
