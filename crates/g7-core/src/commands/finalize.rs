//! Finalize a browser-installed GnuBoard7 application.
//!
//! The base installer deliberately stops at GnuBoard7's official web installer.
//! This command runs only after that installer writes `INSTALLER_COMPLETED=true`.
//! It applies G7 settings through `SettingsService`, installs tracked systemd
//! units, validates runtime state, and records every created resource for reset.

use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, read_owned_files, write_owned_files};
use g7_state::state::{STATE_PATH, read_state_file, write_state_file};
use g7_system::SystemProbe;
use g7_system::command::{CommandOutput, CommandRunner, CommandSpec};
use g7_system::service::ServiceActivity;
use serde::Serialize;

use crate::installer_paths::{REPORT_PATH, SECRETS_PATH, SETUP_GUIDE_PATH};
use crate::runtime_resources::{
    G7_QUEUE_SERVICE, G7_QUEUE_SERVICE_PATH, G7_REVERB_SERVICE, G7_REVERB_SERVICE_PATH,
    G7_SCHEDULER_SERVICE_PATH, G7_SCHEDULER_TIMER, G7_SCHEDULER_TIMER_PATH,
};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizePaths {
    root: PathBuf,
}

impl FinalizePaths {
    pub fn system() -> Self {
        Self {
            root: PathBuf::from("/"),
        }
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        if self.root == Path::new("/") {
            return PathBuf::from(path);
        }
        self.root.join(path.trim_start_matches('/'))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinalizeCheck {
    pub name: String,
    pub status: String,
    pub message: String,
}

impl FinalizeCheck {
    fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "pass".to_string(),
            message: message.into(),
        }
    }

    fn manual(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "manual".to_string(),
            message: message.into(),
        }
    }

    fn warn(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "warn".to_string(),
            message: message.into(),
        }
    }

    fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "fail".to_string(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinalizeReport {
    pub status: String,
    pub message: String,
    pub checks: Vec<FinalizeCheck>,
    pub services: Vec<String>,
    pub owned_files: Vec<String>,
}

#[derive(Debug, Clone)]
struct FinalizeContext {
    domain: String,
    public_host: String,
    https_enabled: bool,
    site_user: String,
    web_root: String,
    redis_enabled: bool,
    mail_mode: String,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    smtp_from: Option<String>,
    smtp_username: Option<String>,
    smtp_encryption: Option<String>,
}

pub fn run() -> Result<FinalizeReport> {
    run_with_probe_and_paths(&SystemProbe::real(), &FinalizePaths::system())
}

pub fn run_with_probe_and_paths<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &FinalizePaths,
) -> Result<FinalizeReport> {
    let _operation_lock = g7_state::lock::InstallerLock::acquire(
        &paths.resolve(g7_state::lock::LOCK_PATH),
        "finalize",
    )
    .map_err(|source| Error::OperationLocked {
        operation: "finalize",
        source,
    })?;
    require_root(probe)?;

    let mut report_json = read_json(paths, REPORT_PATH)?;
    let context = FinalizeContext::from_report(&report_json)?;
    require_gnuboard7_report(&report_json)?;
    let mut state =
        read_state_file(&paths.resolve(STATE_PATH)).map_err(|source| Error::FileReadFailed {
            path: STATE_PATH.to_string(),
            source,
        })?;
    state.begin_step("gnuboard7-finalize");
    write_state_file(&paths.resolve(STATE_PATH), &state).map_err(|source| {
        Error::FileWriteFailed {
            path: STATE_PATH.to_string(),
            source,
        }
    })?;

    match finalize_inner(probe, paths, &context) {
        Ok(finalized) => {
            state.complete_step("gnuboard7-finalize");
            if !state
                .completed_steps
                .iter()
                .any(|step| step == "gnuboard7-finalized")
            {
                state
                    .completed_steps
                    .push("gnuboard7-finalized".to_string());
            }
            write_state_file(&paths.resolve(STATE_PATH), &state).map_err(|source| {
                Error::FileWriteFailed {
                    path: STATE_PATH.to_string(),
                    source,
                }
            })?;
            merge_finalize_report(&mut report_json, &finalized, None);
            write_json(paths, REPORT_PATH, &report_json)?;
            Ok(finalized)
        }
        Err(error) => {
            state.fail_step("gnuboard7-finalize", error.to_string(), false);
            let _ = write_state_file(&paths.resolve(STATE_PATH), &state);
            let current_report = read_json(paths, REPORT_PATH).ok();
            if current_report.as_ref().is_some_and(|report| {
                report
                    .get("finalize_phase")
                    .and_then(serde_json::Value::as_str)
                    == Some("fail")
                    && report
                        .get("finalize_checks")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|checks| !checks.is_empty())
            }) {
                return Err(error);
            }
            let failed = FinalizeReport {
                status: "fail".to_string(),
                message: "GnuBoard7 후속 설정에 실패했습니다.".to_string(),
                checks: vec![FinalizeCheck::fail("gnuboard7-finalize", error.to_string())],
                services: Vec::new(),
                owned_files: Vec::new(),
            };
            merge_finalize_report(&mut report_json, &failed, Some(&error.to_string()));
            let _ = write_json(paths, REPORT_PATH, &report_json);
            Err(error)
        }
    }
}

fn finalize_inner<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &FinalizePaths,
    context: &FinalizeContext,
) -> Result<FinalizeReport> {
    require_browser_install_complete(paths, context)?;
    let mut checks = vec![FinalizeCheck::pass(
        "browser-install",
        "GnuBoard7 공식 웹 설치 완료 표식을 확인했습니다.",
    )];

    run_artisan(
        probe,
        context,
        "settings-install-merge",
        &["settings:install", "--merge"],
    )?;
    let settings = settings_payload(paths, context)?;
    run_g7_settings(probe, context, &settings)?;
    checks.push(FinalizeCheck::pass(
        "g7-settings",
        "G7 SettingsService로 Redis, 세션, 큐, 웹소켓, 메일 설정을 저장했습니다.",
    ));

    let storage_link = Path::new(&context.web_root).join("public/storage");
    if !paths
        .resolve(storage_link.to_string_lossy().as_ref())
        .exists()
    {
        run_artisan(probe, context, "storage-link", &["storage:link"])?;
    }
    checks.push(FinalizeCheck::pass(
        "storage-link",
        "공개 저장소 심볼릭 링크를 확인했습니다.",
    ));

    run_artisan(probe, context, "migrate-status", &["migrate:status"])?;
    for (step, args) in [
        ("module-list", &["module:list"][..]),
        ("plugin-list", &["plugin:list"][..]),
        ("template-list", &["template:list"][..]),
        ("route-list", &["route:list", "--json"][..]),
    ] {
        run_artisan(probe, context, step, args)?;
    }
    run_artisan(probe, context, "schedule-list", &["schedule:list"])?;
    run_artisan(
        probe,
        context,
        "sitemap-generate",
        &["seo:generate-sitemap", "--sync"],
    )?;
    run_artisan(probe, context, "optimize", &["optimize"])?;
    checks.push(FinalizeCheck::pass(
        "artisan-runtime",
        "DB 마이그레이션, 확장 목록, 라우트, 스케줄, 사이트맵과 Laravel 최적화를 검증했습니다.",
    ));
    checks.push(verify_effective_config(probe, context)?);
    if context.redis_enabled {
        let output = probe
            .redis_ping()
            .map_err(|error| command_error("redis-ping", "redis-cli --raw PING", error))?;
        require_success("redis-ping", "redis-cli --raw PING", output.clone())?;
        if output.stdout.trim() != "PONG" {
            return Err(Error::InstallVerificationFailed {
                checks: format!(
                    "Redis PING returned an unexpected response: {}",
                    output.stdout.trim()
                ),
            });
        }
        checks.push(FinalizeCheck::pass(
            "redis-ping",
            "Redis가 PONG으로 응답했습니다.",
        ));
    }

    let mut owned = read_owned_files(&paths.resolve(OWNED_FILES_PATH)).map_err(|source| {
        Error::FileReadFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;
    register_site_settings(&mut owned, context);
    let units = runtime_units(context);
    for (path, _, _) in &units {
        register_owned(&mut owned, path);
    }
    write_owned_files(&paths.resolve(OWNED_FILES_PATH), &owned).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;
    for (path, content, _) in &units {
        write_runtime_file(paths, path, content)?;
    }

    let unit_paths = units
        .iter()
        .map(|(path, _, _)| paths.resolve(path))
        .collect::<Vec<_>>();
    let output = probe
        .systemd_verify_units(&unit_paths)
        .map_err(|error| command_error("g7-systemd-verify", "systemd-analyze verify", error))?;
    require_success("g7-systemd-verify", "systemd-analyze verify", output)?;
    require_success(
        "g7-systemd-reload",
        "systemctl daemon-reload",
        probe.systemd_daemon_reload().map_err(|error| {
            command_error("g7-systemd-reload", "systemctl daemon-reload", error)
        })?,
    )?;

    let mut services = Vec::new();
    for (_, _, service) in units.iter().filter(|(_, _, service)| !service.is_empty()) {
        require_success(
            "g7-service-enable",
            format!("systemctl enable --now {service}"),
            probe.enable_service_now(service).map_err(|error| {
                command_error(
                    "g7-service-enable",
                    format!("systemctl enable --now {service}"),
                    error,
                )
            })?,
        )?;
        if probe.service_activity(service).map_err(|error| {
            command_error(
                "g7-service-active",
                format!("systemctl is-active {service}"),
                error,
            )
        })? != ServiceActivity::Active
        {
            return Err(Error::InstallVerificationFailed {
                checks: format!("GnuBoard7 runtime service is not active: {service}"),
            });
        }
        services.push((*service).to_string());
    }
    checks.push(FinalizeCheck::pass(
        "runtime-services",
        format!(
            "{}개 G7 런타임 서비스를 활성화하고 상태를 확인했습니다.",
            services.len()
        ),
    ));
    if context.redis_enabled {
        run_artisan(probe, context, "queue-restart", &["queue:restart"])?;
        checks.push(FinalizeCheck::pass(
            "queue-restart",
            "큐 워커에 안전한 재시작 신호를 전달했습니다.",
        ));
        checks.push(verify_queue_roundtrip(probe, paths, context)?);
        checks.push(verify_broadcast_publish(probe, context)?);
        let handshake = verify_websocket_handshake_with_grace(probe, context)?;
        checks.push(FinalizeCheck::pass(
            "reverb-listener",
            "Reverb 내부 소켓 127.0.0.1:8080의 WebSocket 응답을 확인했습니다.",
        ));
        checks.push(handshake);
    }
    if !probe
        .http_host_path_smoke(&context.public_host, "/sitemap.xml")
        .map_err(|error| command_error("sitemap-smoke", "curl /sitemap.xml", error))?
    {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "sitemap HTTP smoke failed for Host: {}",
                context.public_host
            ),
        });
    }
    checks.push(FinalizeCheck::pass(
        "sitemap-smoke",
        "웹서버를 통한 /sitemap.xml 응답을 확인했습니다.",
    ));

    let asset_checks = validate_vite_manifest(paths, context)?;
    checks.extend(asset_checks);
    checks.push(verify_public_assets(probe, context)?);
    checks.push(FinalizeCheck::manual(
        "external-integrations",
        "S3, GeoIP, 외부 SMTP 계정은 사용자 자격증명이 있을 때만 별도 설정합니다.",
    ));

    let finalized = FinalizeReport {
        status: "pass".to_string(),
        message: "GnuBoard7 후속 런타임 설정과 검증이 완료되었습니다.".to_string(),
        checks,
        services,
        owned_files: owned.files.clone(),
    };
    update_setup_guide(paths, context, &finalized)?;

    Ok(finalized)
}

impl FinalizeContext {
    fn from_report(report: &serde_json::Value) -> Result<Self> {
        let required = |key: &'static str| {
            report
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .filter(|value| !value.is_empty())
                .ok_or(Error::MissingInput { field: key })
        };
        Ok(Self {
            domain: required("domain")?,
            public_host: report
                .get("app_url")
                .and_then(serde_json::Value::as_str)
                .and_then(url_host)
                .unwrap_or_else(|| {
                    report
                        .get("domain")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                }),
            https_enabled: report
                .get("app_url")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|url| url.starts_with("https://")),
            site_user: required("site_user")?,
            web_root: required("web_root")?,
            redis_enabled: report.get("redis").and_then(serde_json::Value::as_str)
                == Some("enable"),
            mail_mode: report
                .get("mail_mode")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("none")
                .to_string(),
            smtp_host: report_string(report, "smtp_host"),
            smtp_port: report
                .get("smtp_port")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u16::try_from(value).ok()),
            smtp_from: report_string(report, "smtp_from"),
            smtp_username: report_string(report, "smtp_username"),
            smtp_encryption: report_string(report, "smtp_encryption"),
        })
    }
}

fn require_gnuboard7_report(report: &serde_json::Value) -> Result<()> {
    let app = report
        .get("app_profile")
        .or_else(|| report.get("app_package"))
        .and_then(serde_json::Value::as_str);
    if app == Some("gnuboard7") {
        Ok(())
    } else {
        Err(Error::InvalidOption {
            field: "app_profile",
            value: app.unwrap_or("missing").to_string(),
            supported: "gnuboard7".to_string(),
        })
    }
}

fn require_browser_install_complete(
    paths: &FinalizePaths,
    context: &FinalizeContext,
) -> Result<()> {
    let env_path = format!("{}/.env", context.web_root);
    let content =
        fs::read_to_string(paths.resolve(&env_path)).map_err(|source| Error::FileReadFailed {
            path: env_path.clone(),
            source,
        })?;
    let completed = content.lines().any(|line| {
        line.split_once('=').is_some_and(|(key, value)| {
            key.trim() == "INSTALLER_COMPLETED"
                && matches!(
                    value
                        .trim()
                        .trim_matches(['\'', '"'])
                        .to_ascii_lowercase()
                        .as_str(),
                    "true" | "1" | "yes"
                )
        })
    });
    if completed {
        Ok(())
    } else {
        Err(Error::InstallVerificationFailed {
            checks: "GnuBoard7 browser installer is not completed (INSTALLER_COMPLETED is absent)"
                .to_string(),
        })
    }
}

fn settings_payload(paths: &FinalizePaths, context: &FinalizeContext) -> Result<serde_json::Value> {
    let (cache_driver, session_driver, queue_driver, websocket_enabled) = if context.redis_enabled {
        ("redis", "redis", "redis", true)
    } else {
        ("file", "file", "sync", false)
    };
    let existing_drivers = read_optional_json(
        paths,
        &format!("{}/storage/app/settings/drivers.json", context.web_root),
    );
    let existing_string = |key: &str| {
        existing_drivers
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let mut payload = serde_json::json!({
        "drivers": {
            "storage_driver": "local",
            "cache_driver": cache_driver,
            "session_driver": session_driver,
            "queue_driver": queue_driver,
            "redis_host": "127.0.0.1",
            "redis_port": 6379,
            "redis_password": "",
            "redis_database": 0,
            "log_driver": "daily",
            "log_level": "error",
            "log_days": 14,
            "websocket_enabled": websocket_enabled,
            "websocket_app_id": existing_string("websocket_app_id").unwrap_or(random_hex(8)?),
            "websocket_app_key": existing_string("websocket_app_key").unwrap_or(random_hex(16)?),
            "websocket_app_secret": existing_string("websocket_app_secret").unwrap_or(random_hex(24)?),
            "websocket_host": context.public_host,
            "websocket_port": 443,
            "websocket_scheme": "https",
            "websocket_verify_ssl": true,
            "websocket_server_host": "127.0.0.1",
            "websocket_server_port": 8080,
            "websocket_server_scheme": "http",
            "search_engine_driver": "mysql-fulltext"
        }
    });

    if context.mail_mode == "local-postfix" {
        payload["mail"] = serde_json::json!({
            "mailer": "smtp",
            "host": "127.0.0.1",
            "port": 25,
            "username": "",
            "password": "",
            "encryption": "",
            "from_address": context.smtp_from.clone().unwrap_or_else(|| format!("noreply@{}", context.domain)),
            "from_name": "GnuBoard7"
        });
    } else if context.mail_mode == "smtp-relay" {
        let secret = read_toml_secret(paths, "smtp_password")?;
        payload["mail"] = serde_json::json!({
            "mailer": "smtp",
            "host": context.smtp_host.clone().unwrap_or_default(),
            "port": context.smtp_port.unwrap_or(587),
            "username": context.smtp_username.clone().unwrap_or_default(),
            "password": secret,
            "encryption": context.smtp_encryption.clone().unwrap_or_else(|| "tls".to_string()),
            "from_address": context.smtp_from.clone().unwrap_or_else(|| format!("noreply@{}", context.domain)),
            "from_name": "GnuBoard7"
        });
    }
    Ok(payload)
}

fn run_g7_settings<R: CommandRunner>(
    probe: &SystemProbe<R>,
    context: &FinalizeContext,
    settings: &serde_json::Value,
) -> Result<()> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        serde_json::to_vec(settings).map_err(|source| Error::InstallVerificationFailed {
            checks: format!("failed to serialize G7 settings: {source}"),
        })?,
    );
    let root = php_string(&context.web_root);
    let script = format!(
        "<?php\nchdir('{root}');\nrequire 'vendor/autoload.php';\n$app = require 'bootstrap/app.php';\n$app->make(Illuminate\\Contracts\\Console\\Kernel::class)->bootstrap();\n$payload = json_decode(base64_decode('{encoded}'), true, 512, JSON_THROW_ON_ERROR);\n$service = app(App\\Services\\SettingsService::class);\nforeach ($payload as $tab => $values) {{\n  if (!$service->saveSettings(['_tab' => $tab, $tab => $values])) {{ fwrite(STDERR, \"settings save failed: $tab\\n\"); exit(2); }}\n}}\necho \"g7-settings-applied\\n\";\n"
    );
    let command = CommandSpec::new("runuser")
        .args(["-u", context.site_user.as_str(), "--", "/usr/bin/php"])
        .stdin_bytes(script.into_bytes());
    let output = probe
        .runner()
        .run(&command)
        .map_err(|error| command_error("g7-settings", "runuser -- php [settings script]", error))?;
    require_success("g7-settings", "runuser -- php [settings script]", output)
}

fn verify_effective_config<R: CommandRunner>(
    probe: &SystemProbe<R>,
    context: &FinalizeContext,
) -> Result<FinalizeCheck> {
    let root = php_string(&context.web_root);
    let script = format!(
        "<?php\nchdir('{root}');\nrequire 'vendor/autoload.php';\n$app = require 'bootstrap/app.php';\n$app->make(Illuminate\\Contracts\\Console\\Kernel::class)->bootstrap();\necho json_encode([\n  'storage' => config('filesystems.default'),\n  'cache' => config('cache.default'),\n  'session' => config('session.driver'),\n  'queue' => config('queue.default'),\n  'search' => config('scout.driver'),\n  'broadcasting' => config('broadcasting.default'),\n  'client_host' => config('g7.websocket.client.host'),\n  'client_port' => config('g7.websocket.client.port'),\n  'client_scheme' => config('g7.websocket.client.scheme'),\n  'server_host' => config('broadcasting.connections.reverb.options.host'),\n  'server_port' => config('broadcasting.connections.reverb.options.port'),\n  'server_scheme' => config('broadcasting.connections.reverb.options.scheme'),\n  'mail' => config('mail.default'),\n], JSON_THROW_ON_ERROR) . PHP_EOL;\n"
    );
    let command = CommandSpec::new("runuser")
        .args(["-u", context.site_user.as_str(), "--", "/usr/bin/php"])
        .stdin_bytes(script.into_bytes());
    let output = probe.runner().run(&command).map_err(|error| {
        command_error(
            "g7-effective-config",
            "runuser -- php [effective config script]",
            error,
        )
    })?;
    require_success(
        "g7-effective-config",
        "runuser -- php [effective config script]",
        output.clone(),
    )?;
    let json_line = output
        .stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| Error::InstallVerificationFailed {
            checks: "G7 effective config check returned no data".to_string(),
        })?;
    let actual: serde_json::Value =
        serde_json::from_str(json_line).map_err(|source| Error::InstallVerificationFailed {
            checks: format!("G7 effective config check returned invalid JSON: {source}"),
        })?;
    let expected = if context.redis_enabled {
        serde_json::json!({
            "storage": "local",
            "cache": "redis",
            "session": "redis",
            "queue": "redis",
            "search": "mysql-fulltext",
            "broadcasting": "reverb",
            "client_host": context.public_host,
            "client_port": 443,
            "client_scheme": "https",
            "server_host": "127.0.0.1",
            "server_port": 8080,
            "server_scheme": "http"
        })
    } else {
        serde_json::json!({
            "storage": "local",
            "cache": "file",
            "session": "file",
            "queue": "sync",
            "search": "mysql-fulltext",
            "broadcasting": "null",
            "client_host": ""
        })
    };
    let mismatches = expected
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, expected_value)| {
            let actual_value = actual.get(key).unwrap_or(&serde_json::Value::Null);
            (actual_value != expected_value)
                .then(|| format!("{key}: expected {expected_value}, got {actual_value}"))
        })
        .collect::<Vec<_>>();
    if !mismatches.is_empty() {
        return Err(Error::InstallVerificationFailed {
            checks: format!("G7 effective config mismatch: {}", mismatches.join("; ")),
        });
    }
    Ok(FinalizeCheck::pass(
        "effective-config",
        "Laravel이 읽는 저장소, 캐시, 세션, 큐, 검색, 웹소켓 실효 설정값을 확인했습니다.",
    ))
}

fn verify_websocket_handshake<R: CommandRunner>(
    probe: &SystemProbe<R>,
    context: &FinalizeContext,
) -> Result<FinalizeCheck> {
    let root = php_string(&context.web_root);
    let public_host = php_string(&context.public_host);
    let verify_external = if context.https_enabled {
        "true"
    } else {
        "false"
    };
    let script = format!(
        r#"<?php
chdir('{root}');
$settings = json_decode(file_get_contents('storage/app/settings/drivers.json'), true, 512, JSON_THROW_ON_ERROR);
$key = (string) ($settings['websocket_app_key'] ?? '');
if ($key === '') {{ fwrite(STDERR, "missing websocket key\n"); exit(2); }}
function handshake(string $transport, string $host, int $port, string $path, bool $tls): int {{
    $context = $tls ? stream_context_create(['ssl' => ['verify_peer' => true, 'verify_peer_name' => true, 'peer_name' => $host]]) : null;
    $socket = @stream_socket_client("{{$transport}}://{{$host}}:{{$port}}", $errno, $error, 8, STREAM_CLIENT_CONNECT, $context);
    if (!$socket) {{ return 0; }}
    stream_set_timeout($socket, 8);
    $nonce = base64_encode(random_bytes(16));
    $request = "GET {{$path}} HTTP/1.1\r\nHost: {{$host}}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {{$nonce}}\r\nSec-WebSocket-Version: 13\r\n\r\n";
    fwrite($socket, $request);
    $line = fgets($socket) ?: '';
    fclose($socket);
    return preg_match('/^HTTP\\/1\\.[01] 101 /', $line) === 1 ? 101 : 0;
}}
$path = '/app/' . rawurlencode($key) . '?protocol=7&client=g7inst&version=1.0&flash=false';
$result = ['internal' => handshake('tcp', '127.0.0.1', 8080, $path, false), 'external' => null];
if ({verify_external}) {{ $result['external'] = handshake('tls', '{public_host}', 443, $path, true); }}
echo json_encode($result, JSON_THROW_ON_ERROR) . PHP_EOL;
"#
    );
    let command = CommandSpec::new("runuser")
        .args(["-u", context.site_user.as_str(), "--", "/usr/bin/php"])
        .stdin_bytes(script.into_bytes());
    let output = probe.runner().run(&command).map_err(|error| {
        command_error(
            "reverb-handshake",
            "runuser -- php [WebSocket handshake script]",
            error,
        )
    })?;
    require_success(
        "reverb-handshake",
        "runuser -- php [WebSocket handshake script]",
        output.clone(),
    )?;
    let result: serde_json::Value =
        serde_json::from_str(output.stdout.trim()).map_err(|source| {
            Error::InstallVerificationFailed {
                checks: format!("Reverb handshake returned invalid JSON: {source}"),
            }
        })?;
    if result.get("internal").and_then(serde_json::Value::as_u64) != Some(101) {
        return Err(Error::InstallVerificationFailed {
            checks: "Reverb internal WebSocket handshake did not return HTTP 101".to_string(),
        });
    }
    if context.https_enabled
        && result.get("external").and_then(serde_json::Value::as_u64) != Some(101)
    {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "Reverb external WSS handshake did not return HTTP 101 for {}",
                context.public_host
            ),
        });
    }
    Ok(FinalizeCheck::pass(
        "reverb-handshake",
        if context.https_enabled {
            "Reverb 내부 WebSocket과 외부 WSS 프록시가 HTTP 101로 연결됐습니다."
        } else {
            "Reverb 내부 WebSocket은 연결됐으며 외부 WSS는 TLS 적용 후 확인해야 합니다."
        },
    ))
}

fn verify_queue_roundtrip<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &FinalizePaths,
    context: &FinalizeContext,
) -> Result<FinalizeCheck> {
    let probe_path = format!(
        "{}/storage/app/.g7inst-queue-probe-{}.php",
        context.web_root,
        random_hex(8)?
    );
    let root = php_string(&context.web_root);
    let script = format!(
        r#"<?php
chdir('{root}');
require 'vendor/autoload.php';
$app = require 'bootstrap/app.php';
$app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
$key = 'g7inst:queue-probe:' . bin2hex(random_bytes(8));
cache()->forget($key);
dispatch(function () use ($key): void {{ cache()->put($key, 'processed', 60); }});
for ($attempt = 0; $attempt < 40; $attempt++) {{
    if (cache()->get($key) === 'processed') {{ cache()->forget($key); echo "processed\n"; exit(0); }}
    usleep(250000);
}}
cache()->forget($key);
fwrite(STDERR, "queue probe timed out\n");
exit(3);
"#
    );
    write_runtime_file(paths, &probe_path, &script)?;
    let output = probe.runner().run(&CommandSpec::new("runuser").args([
        "-u",
        context.site_user.as_str(),
        "--",
        "/usr/bin/php",
        probe_path.as_str(),
    ]));
    let _ = fs::remove_file(paths.resolve(&probe_path));
    let output = output
        .map_err(|error| command_error("queue-roundtrip", "runuser -- php queue probe", error))?;
    require_success(
        "queue-roundtrip",
        "runuser -- php queue probe",
        output.clone(),
    )?;
    if output.stdout.trim() != "processed" {
        return Err(Error::InstallVerificationFailed {
            checks: "GnuBoard7 queue worker did not process the transient Redis cache probe"
                .to_string(),
        });
    }
    Ok(FinalizeCheck::pass(
        "queue-roundtrip",
        "임시 Queue Job을 Redis에 넣고 워커가 처리한 결과를 확인한 뒤 테스트 키를 삭제했습니다.",
    ))
}

fn verify_broadcast_publish<R: CommandRunner>(
    probe: &SystemProbe<R>,
    context: &FinalizeContext,
) -> Result<FinalizeCheck> {
    let root = php_string(&context.web_root);
    let script = format!(
        r#"<?php
chdir('{root}');
require 'vendor/autoload.php';
$app = require 'bootstrap/app.php';
$app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
app(Illuminate\Contracts\Broadcasting\Factory::class)
    ->connection('reverb')
    ->broadcast(['g7inst-runtime-probe'], 'runtime-probe', ['ok' => true]);
echo "published\n";
"#
    );
    let output = probe
        .runner()
        .run(
            &CommandSpec::new("runuser")
                .args(["-u", context.site_user.as_str(), "--", "/usr/bin/php"])
                .stdin_bytes(script.into_bytes()),
        )
        .map_err(|error| {
            command_error(
                "broadcast-publish",
                "runuser -- php [broadcast probe]",
                error,
            )
        })?;
    require_success(
        "broadcast-publish",
        "runuser -- php [broadcast probe]",
        output.clone(),
    )?;
    if output.stdout.trim() != "published" {
        return Err(Error::InstallVerificationFailed {
            checks: "GnuBoard7 backend broadcast probe returned an unexpected response".to_string(),
        });
    }
    Ok(FinalizeCheck::pass(
        "broadcast-publish",
        "Laravel broadcasting 연결을 통해 Reverb 이벤트 송신을 확인했습니다.",
    ))
}

const REVERB_HANDSHAKE_ATTEMPTS: usize = 5;

fn verify_websocket_handshake_with_grace<R: CommandRunner>(
    probe: &SystemProbe<R>,
    context: &FinalizeContext,
) -> Result<FinalizeCheck> {
    let mut last_error = None;
    for attempt in 0..REVERB_HANDSHAKE_ATTEMPTS {
        match verify_websocket_handshake(probe, context) {
            Ok(check) => return Ok(check),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < REVERB_HANDSHAKE_ATTEMPTS {
            #[cfg(not(test))]
            std::thread::sleep(Duration::from_secs(1));
            #[cfg(test)]
            std::thread::sleep(Duration::ZERO);
        }
    }
    Err(
        last_error.unwrap_or_else(|| Error::InstallVerificationFailed {
            checks: "Reverb WebSocket readiness check did not run".to_string(),
        }),
    )
}

fn url_host(url: &str) -> Option<String> {
    url.split_once("://")?
        .1
        .split('/')
        .next()
        .and_then(|authority| authority.split('@').next_back())
        .and_then(|authority| authority.split(':').next())
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_string)
}

fn run_artisan<R: CommandRunner>(
    probe: &SystemProbe<R>,
    context: &FinalizeContext,
    step: &'static str,
    args: &[&str],
) -> Result<CommandOutput> {
    let mut command_args = vec![
        "-u",
        context.site_user.as_str(),
        "--",
        "/usr/bin/php",
        "artisan",
    ];
    command_args.extend_from_slice(args);
    let command = CommandSpec::new("runuser")
        .args(command_args)
        .current_dir(&context.web_root);
    let output = probe
        .runner()
        .run(&command)
        .map_err(|error| command_error(step, format!("php artisan {}", args.join(" ")), error))?;
    require_success(
        step,
        format!("php artisan {}", args.join(" ")),
        output.clone(),
    )?;
    Ok(output)
}

fn runtime_units(context: &FinalizeContext) -> Vec<(&'static str, String, &'static str)> {
    let after = if context.redis_enabled {
        "network-online.target mysql.service redis-server.service"
    } else {
        "network-online.target mysql.service"
    };
    let mut units = vec![
        (
            G7_SCHEDULER_SERVICE_PATH,
            format!("[Unit]\nDescription=GnuBoard7 scheduler\nAfter={after}\n\n[Service]\nType=oneshot\nUser={}\nGroup=www-data\nUMask=0002\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan schedule:run\n", context.site_user, context.web_root),
            "",
        ),
        (
            G7_SCHEDULER_TIMER_PATH,
            "[Unit]\nDescription=GnuBoard7 scheduler every minute\n\n[Timer]\nOnCalendar=*-*-* *:*:00\nAccuracySec=10s\nPersistent=true\nUnit=g7-scheduler.service\n\n[Install]\nWantedBy=timers.target\n".to_string(),
            G7_SCHEDULER_TIMER,
        ),
    ];
    if context.redis_enabled {
        units.push((
            G7_QUEUE_SERVICE_PATH,
            format!("[Unit]\nDescription=GnuBoard7 queue worker\nAfter={after}\n\n[Service]\nType=simple\nUser={}\nGroup=www-data\nUMask=0002\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan queue:work redis --sleep=3 --tries=3 --timeout=90 --max-time=3600\nRestart=always\nRestartSec=5\nTimeoutStopSec=360\nKillSignal=SIGTERM\n\n[Install]\nWantedBy=multi-user.target\n", context.site_user, context.web_root),
            G7_QUEUE_SERVICE,
        ));
        units.push((
            G7_REVERB_SERVICE_PATH,
            format!("[Unit]\nDescription=GnuBoard7 Reverb WebSocket server\nAfter={after}\n\n[Service]\nType=simple\nUser={}\nGroup=www-data\nUMask=0002\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan reverb:start --host=127.0.0.1 --port=8080\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n", context.site_user, context.web_root),
            G7_REVERB_SERVICE,
        ));
    }
    units
}

fn validate_vite_manifest(
    paths: &FinalizePaths,
    context: &FinalizeContext,
) -> Result<Vec<FinalizeCheck>> {
    let build_dir = format!("{}/public/build", context.web_root);
    let manifest_path = format!("{build_dir}/manifest.json");
    let audit = crate::vite_manifest::audit_vite_manifest(
        &paths.resolve(&manifest_path),
        &paths.resolve(&build_dir),
    )?;
    if audit.referenced.is_empty() {
        return Ok(vec![FinalizeCheck::fail(
            "vite-manifest",
            "manifest.json에서 배포 자산 경로를 찾지 못했습니다.",
        )]);
    }
    if audit.missing.is_empty() {
        Ok(vec![FinalizeCheck::pass(
            "vite-manifest",
            "Vite manifest가 참조하는 모든 배포 자산을 확인했습니다.",
        )])
    } else {
        Ok(vec![FinalizeCheck::warn(
            "vite-manifest",
            format!(
                "G7 공식 릴리스의 Vite manifest 참조와 동봉 자산 이름이 일치하지 않습니다: {}. G7 코어와 동봉 자산은 수정하지 않았습니다.",
                audit.missing.join(", ")
            ),
        )])
    }
}

fn verify_public_assets<R: CommandRunner>(
    probe: &SystemProbe<R>,
    context: &FinalizeContext,
) -> Result<FinalizeCheck> {
    let scheme = if context.https_enabled {
        "https"
    } else {
        "http"
    };
    let base_url = format!("{scheme}://{}", context.public_host);
    let homepage = probe
        .runner()
        .run(
            &CommandSpec::new("curl")
                .args(["-fsSL", "--max-time", "15"])
                .arg(format!("{base_url}/")),
        )
        .map_err(|error| command_error("public-assets", "curl homepage", error))?;
    require_success("public-assets", "curl homepage", homepage.clone())?;

    let assets = public_asset_paths(&homepage.stdout, &context.public_host);
    if assets.is_empty() {
        return Ok(FinalizeCheck::warn(
            "public-assets",
            "메인 화면 HTML에서 같은 도메인의 JS/CSS 자산을 찾지 못했습니다.",
        ));
    }

    let mut missing = Vec::new();
    for asset in &assets {
        let output = probe
            .runner()
            .run(
                &CommandSpec::new("curl")
                    .args(["-fsSL", "--max-time", "15"])
                    .arg(format!("{base_url}{asset}"))
                    .arg("-o")
                    .arg("/dev/null"),
            )
            .map_err(|error| command_error("public-assets", "curl public asset", error))?;
        if output.status != 0 {
            missing.push(asset.clone());
        }
    }

    if missing.is_empty() {
        Ok(FinalizeCheck::pass(
            "public-assets",
            format!(
                "메인 화면이 참조하는 같은 도메인 JS/CSS 자산 {}개를 확인했습니다.",
                assets.len()
            ),
        ))
    } else {
        Ok(FinalizeCheck::warn(
            "public-assets",
            format!(
                "메인 화면이 참조하지만 HTTP로 제공되지 않는 자산이 있습니다: {}. G7 코어와 동봉 자산은 수정하지 않았습니다.",
                missing.join(", ")
            ),
        ))
    }
}

fn public_asset_paths(html: &str, public_host: &str) -> Vec<String> {
    let mut assets = Vec::new();
    for (attribute, quote) in [
        ("src=\"", '"'),
        ("href=\"", '"'),
        ("src='", '\''),
        ("href='", '\''),
    ] {
        let mut remaining = html;
        while let Some(start) = remaining.find(attribute) {
            remaining = &remaining[start + attribute.len()..];
            let Some(end) = remaining.find(quote) else {
                break;
            };
            let value = &remaining[..end];
            remaining = &remaining[end + quote.len_utf8()..];

            let same_origin = [
                format!("https://{public_host}"),
                format!("http://{public_host}"),
            ]
            .into_iter()
            .find_map(|origin| value.strip_prefix(&origin));
            let path = same_origin
                .or_else(|| (value.starts_with('/') && !value.starts_with("//")).then_some(value));
            let Some(path) = path else {
                continue;
            };
            let file_path = path
                .split('#')
                .next()
                .unwrap_or(path)
                .split('?')
                .next()
                .unwrap_or(path);
            if file_path.ends_with(".js") || file_path.ends_with(".css") {
                assets.push(path.to_string());
            }
        }
    }
    assets.sort();
    assets.dedup();
    assets
}

fn register_site_settings(owned: &mut OwnedFiles, context: &FinalizeContext) {
    for path in [
        format!("{}/storage/app/settings/drivers.json", context.web_root),
        format!("{}/storage/app/settings/mail.json", context.web_root),
        format!("{}/public/storage", context.web_root),
    ] {
        register_owned(owned, &path);
    }
}

fn register_owned(owned: &mut OwnedFiles, path: &str) {
    if !owned.files.iter().any(|owned_path| owned_path == path) {
        owned.files.push(path.to_string());
    }
}

fn update_setup_guide(
    paths: &FinalizePaths,
    context: &FinalizeContext,
    report: &FinalizeReport,
) -> Result<()> {
    const START: &str = "<!-- G7-RUNTIME-FINALIZE-START -->";
    const END: &str = "<!-- G7-RUNTIME-FINALIZE-END -->";
    let existing = fs::read_to_string(paths.resolve(SETUP_GUIDE_PATH)).unwrap_or_default();
    let before = existing
        .split_once(START)
        .map_or(existing.as_str(), |(head, _)| head);
    let settings_root = format!("{}/storage/app/settings", context.web_root);
    let mut block = format!(
        "{START}\n## GnuBoard7 런타임 마무리\n\n- 상태: `{}`\n- 드라이버 설정: `{settings_root}/drivers.json`\n- 메일 설정: `{settings_root}/mail.json`\n- 공개 저장소 링크: `{}/public/storage`\n- 재검증: `sudo g7inst finalize`\n- 전체 초기화: `sudo g7inst reset --yes` (G7 런타임 설정과 서비스는 삭제, Let's Encrypt 인증서는 보존)\n\n### 서비스\n\n",
        report.status, context.web_root
    );
    if report.services.is_empty() {
        block.push_str("- 별도 상시 서비스 없음\n");
    } else {
        for service in &report.services {
            block.push_str(&format!(
                "- `{service}`: `sudo systemctl status {service}` / `sudo systemctl restart {service}`\n"
            ));
        }
    }
    block.push_str("\n### 검증 결과\n\n");
    for check in &report.checks {
        block.push_str(&format!(
            "- [{}] `{}`: {}\n",
            check.status, check.name, check.message
        ));
    }
    block.push_str(&format!("\n{END}\n"));
    let content = format!("{}\n\n{}\n", before.trim_end(), block);
    write_runtime_file(paths, SETUP_GUIDE_PATH, &content)
}

fn write_runtime_file(paths: &FinalizePaths, path: &str, content: &str) -> Result<()> {
    let target = paths.resolve(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
    g7_state::atomic::atomic_write(&target, content.as_bytes()).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    #[cfg(unix)]
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    Ok(())
}

fn merge_finalize_report(
    report: &mut serde_json::Value,
    finalized: &FinalizeReport,
    problem: Option<&str>,
) {
    let Some(object) = report.as_object_mut() else {
        return;
    };
    object.insert(
        "finalize_phase".to_string(),
        serde_json::json!(finalized.status),
    );
    object.insert(
        "finalize_message".to_string(),
        serde_json::json!(finalized.message),
    );
    object.insert(
        "finalize_checks".to_string(),
        serde_json::to_value(&finalized.checks).unwrap_or_default(),
    );
    object.insert(
        "g7_runtime_services".to_string(),
        serde_json::json!(finalized.services),
    );
    object.insert(
        "finalized_at_unix_ms".to_string(),
        serde_json::json!(unix_timestamp_millis()),
    );
    object.insert("finalize_problem".to_string(), serde_json::json!(problem));
}

fn read_json(paths: &FinalizePaths, path: &str) -> Result<serde_json::Value> {
    let payload = fs::read(paths.resolve(path)).map_err(|source| Error::FileReadFailed {
        path: path.to_string(),
        source,
    })?;
    serde_json::from_slice(&payload).map_err(|source| Error::FileReadFailed {
        path: path.to_string(),
        source: io::Error::other(source),
    })
}

fn read_optional_json(paths: &FinalizePaths, path: &str) -> Option<serde_json::Value> {
    let payload = fs::read(paths.resolve(path)).ok()?;
    serde_json::from_slice(&payload).ok()
}

fn write_json(paths: &FinalizePaths, path: &str, value: &serde_json::Value) -> Result<()> {
    let mut payload =
        serde_json::to_vec_pretty(value).map_err(|source| Error::FileWriteFailed {
            path: path.to_string(),
            source: io::Error::other(source),
        })?;
    payload.push(b'\n');
    g7_state::atomic::atomic_write(&paths.resolve(path), &payload).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })
}

fn report_string(report: &serde_json::Value, key: &str) -> Option<String> {
    report
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn read_toml_secret(paths: &FinalizePaths, key: &str) -> Result<String> {
    let content = fs::read_to_string(paths.resolve(SECRETS_PATH)).map_err(|source| {
        Error::FileReadFailed {
            path: SECRETS_PATH.to_string(),
            source,
        }
    })?;
    let value = content
        .parse::<toml::Value>()
        .map_err(|source| Error::FileReadFailed {
            path: SECRETS_PATH.to_string(),
            source: io::Error::other(source),
        })?;
    value
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or(Error::MissingInput {
            field: "smtp_password",
        })
}

fn random_hex(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|source| Error::InstallVerificationFailed {
        checks: format!("failed to generate G7 runtime secret: {source}"),
    })?;
    Ok(buffer.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn php_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn require_root<R: CommandRunner>(probe: &SystemProbe<R>) -> Result<()> {
    match probe.current_privilege() {
        Ok(g7_system::privilege::Privilege::Root) => Ok(()),
        _ => Err(Error::PrivilegeRequired),
    }
}

fn require_success(
    step: &'static str,
    command: impl Into<String>,
    output: CommandOutput,
) -> Result<()> {
    if output.status == 0 {
        Ok(())
    } else {
        Err(Error::InstallCommandFailed {
            step,
            command: command.into(),
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn command_error(step: &'static str, command: impl Into<String>, error: impl ToString) -> Error {
    Error::InstallCommandFailed {
        step,
        command: command.into(),
        status: 128,
        stdout: String::new(),
        stderr: error.to_string(),
    }
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use g7_state::owned_files::write_owned_files;
    use g7_state::state::{InstallerState, write_state_file};
    use g7_system::command::{CommandOutput, FakeCommandRunner};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn context(redis_enabled: bool) -> FinalizeContext {
        FinalizeContext {
            domain: "example.com".to_string(),
            public_host: "example.com".to_string(),
            https_enabled: true,
            site_user: "example".to_string(),
            web_root: "/home/example/public_html".to_string(),
            redis_enabled,
            mail_mode: "none".to_string(),
            smtp_host: None,
            smtp_port: None,
            smtp_from: None,
            smtp_username: None,
            smtp_encryption: None,
        }
    }

    fn push_successful_finalize_outputs(runner: &FakeCommandRunner) {
        for output in [
            "0\n",
            "settings merged\n",
            "settings applied\n",
            "migrations\n",
            "modules\n",
            "plugins\n",
            "templates\n",
            "routes\n",
            "schedule\n",
            "sitemap\n",
            "optimized\n",
            r#"{"storage":"local","cache":"redis","session":"redis","queue":"redis","search":"mysql-fulltext","broadcasting":"reverb","client_host":"example.com","client_port":443,"client_scheme":"https","server_host":"127.0.0.1","server_port":8080,"server_scheme":"http"}
"#,
            "PONG\n",
            "verified\n",
            "reloaded\n",
            "enabled\n",
            "active\n",
            "enabled\n",
            "active\n",
            "enabled\n",
            "active\n",
            "queue restart\n",
            "processed\n",
            "published\n",
            "{\"internal\":101,\"external\":101}\n",
            "sitemap ok\n",
            "<link rel=\"stylesheet\" href=\"/build/assets/app.css\">\n",
            "asset ok\n",
        ] {
            runner.push_output(CommandOutput::success(output));
        }
    }

    #[test]
    fn redis_runtime_owns_all_g7_units() {
        let units = runtime_units(&context(true));
        let paths = units.iter().map(|(path, _, _)| *path).collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                G7_SCHEDULER_SERVICE_PATH,
                G7_SCHEDULER_TIMER_PATH,
                G7_QUEUE_SERVICE_PATH,
                G7_REVERB_SERVICE_PATH,
            ]
        );
    }

    #[test]
    fn redis_disabled_uses_only_scheduler() {
        let units = runtime_units(&context(false));
        assert_eq!(units.len(), 2);
        assert!(
            units
                .iter()
                .all(|(path, _, _)| !path.contains("queue") && !path.contains("reverb"))
        );
    }

    #[test]
    fn public_asset_paths_keep_only_same_origin_css_and_js() {
        let html = r#"
            <link rel="stylesheet" href="/build/assets/app.css?v=1">
            <script src='https://example.com/build/app.js'></script>
            <script src="https://cdn.example.net/vendor.js"></script>
            <img src="/logo.png">
        "#;

        assert_eq!(
            public_asset_paths(html, "example.com"),
            vec![
                "/build/app.js".to_string(),
                "/build/assets/app.css?v=1".to_string(),
            ]
        );
    }

    #[test]
    fn public_asset_http_failure_is_reported_as_upstream_warning()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(
            "<link rel=\"stylesheet\" href=\"/build/assets/app.css\">",
        ));
        runner.push_output(CommandOutput::failure(22, "404"));
        let probe = SystemProbe::new(runner);

        let check = verify_public_assets(&probe, &context(true))?;

        assert_eq!(check.name, "public-assets");
        assert_eq!(check.status, "warn");
        assert!(check.message.contains("/build/assets/app.css"));
        Ok(())
    }

    #[test]
    fn finalize_records_settings_services_state_and_report()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "g7-finalize-test-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let app = root.join("home/example/public_html");
        fs::create_dir_all(app.join("public/build/assets"))?;
        fs::create_dir_all(app.join("public/storage"))?;
        fs::create_dir_all(app.join("storage/app/settings"))?;
        fs::create_dir_all(root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(root.join("var/log/g7-installer"))?;
        fs::write(app.join(".env"), "INSTALLER_COMPLETED=true\n")?;
        fs::write(
            app.join("public/build/manifest.json"),
            r#"{"app":{"file":"assets/app.js","css":["assets/app.css"]}}"#,
        )?;
        fs::write(app.join("public/build/assets/app.js"), "ok")?;
        fs::write(app.join("public/build/assets/app.css"), "ok")?;
        fs::write(
            root.join("var/log/g7-installer/report.json"),
            r#"{
                "phase":"completed",
                "domain":"example.com",
                "app_url":"https://example.com/install/",
                "app_profile":"gnuboard7",
                "site_user":"example",
                "web_root":"/home/example/public_html",
                "redis":"enable",
                "mail_mode":"none"
            }"#,
        )?;
        write_state_file(
            &root.join("var/lib/g7-installer/state.json"),
            &InstallerState::new("test-install".to_string(), "example.com".to_string()),
        )?;
        write_owned_files(
            &root.join("var/lib/g7-installer/owned-files.json"),
            &OwnedFiles {
                version: 1,
                files: vec!["/home/example/public_html".to_string()],
            },
        )?;

        let runner = FakeCommandRunner::default();
        push_successful_finalize_outputs(&runner);
        let probe = SystemProbe::new(runner);
        let result = run_with_probe_and_paths(&probe, &FinalizePaths::with_root(&root))?;

        assert_eq!(result.status, "pass");
        for check in [
            "effective-config",
            "redis-ping",
            "queue-roundtrip",
            "broadcast-publish",
            "reverb-listener",
            "public-assets",
        ] {
            assert!(
                result
                    .checks
                    .iter()
                    .any(|candidate| candidate.name == check && candidate.status == "pass")
            );
        }
        for path in [
            G7_QUEUE_SERVICE_PATH,
            G7_SCHEDULER_SERVICE_PATH,
            G7_SCHEDULER_TIMER_PATH,
            G7_REVERB_SERVICE_PATH,
        ] {
            assert!(root.join(path.trim_start_matches('/')).is_file());
            assert!(result.owned_files.contains(&path.to_string()));
        }
        let report = read_json(&FinalizePaths::with_root(&root), REPORT_PATH)?;
        assert_eq!(report["finalize_phase"], "pass");
        let guide = fs::read_to_string(root.join(SETUP_GUIDE_PATH.trim_start_matches('/')))?;
        assert!(guide.contains("GnuBoard7 런타임 마무리"));
        assert!(guide.contains("g7-reverb.service"));
        assert!(guide.contains("Redis가 PONG으로 응답했습니다."));
        assert!(fs::read_dir(app.join("storage/app"))?.all(|entry| {
            !entry
                .ok()
                .and_then(|entry| entry.file_name().into_string().ok())
                .is_some_and(|name| name.starts_with(".g7inst-queue-probe-"))
        }));
        let state = read_state_file(&root.join("var/lib/g7-installer/state.json"))?;
        assert!(state.step_is_completed("gnuboard7-finalize"));
        assert!(
            state
                .completed_steps
                .contains(&"gnuboard7-finalized".to_string())
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn finalize_preserves_upstream_manifest_warning_without_blocking_runtime()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "g7-finalize-failure-test-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let app = root.join("home/example/public_html");
        fs::create_dir_all(app.join("public/build/assets"))?;
        fs::create_dir_all(app.join("public/storage"))?;
        fs::create_dir_all(app.join("storage/app/settings"))?;
        fs::create_dir_all(root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(root.join("var/log/g7-installer"))?;
        fs::write(app.join(".env"), "INSTALLER_COMPLETED=true\n")?;
        fs::write(
            app.join("public/build/manifest.json"),
            r#"{"app":{"file":"assets/missing.js"}}"#,
        )?;
        fs::write(
            root.join("var/log/g7-installer/report.json"),
            r#"{
                "phase":"completed",
                "domain":"example.com",
                "app_url":"https://example.com/install/",
                "app_profile":"gnuboard7",
                "site_user":"example",
                "web_root":"/home/example/public_html",
                "redis":"enable",
                "mail_mode":"none"
            }"#,
        )?;
        write_state_file(
            &root.join("var/lib/g7-installer/state.json"),
            &InstallerState::new("test-install".to_string(), "example.com".to_string()),
        )?;
        write_owned_files(
            &root.join("var/lib/g7-installer/owned-files.json"),
            &OwnedFiles {
                version: 1,
                files: vec!["/home/example/public_html".to_string()],
            },
        )?;

        let runner = FakeCommandRunner::default();
        push_successful_finalize_outputs(&runner);
        let probe = SystemProbe::new(runner);
        let finalized = run_with_probe_and_paths(&probe, &FinalizePaths::with_root(&root))?;
        assert_eq!(finalized.status, "pass");

        let report = read_json(&FinalizePaths::with_root(&root), REPORT_PATH)?;
        assert_eq!(report["finalize_phase"], "pass");
        let checks = report["finalize_checks"]
            .as_array()
            .expect("finalize checks should be preserved");
        assert!(checks.iter().any(|check| {
            check["name"] == "vite-manifest"
                && check["status"] == "warn"
                && check["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("assets/missing.js"))
        }));

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
