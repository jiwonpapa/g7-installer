use super::*;

pub struct InstallReport {
    pub domain: String,
    pub deployment_mode: String,
    pub app_profile: String,
    pub app_profile_label: &'static str,
    pub app_document_root: String,
    pub web_server: String,
    pub php_version: String,
    pub php_source: String,
    pub database_engine: String,
    pub database_name: String,
    pub database_user: String,
    pub database_password_policy: &'static str,
    pub site_user: String,
    pub web_root_mode: String,
    pub web_root: String,
    pub app_url: String,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_from: Option<String>,
    pub smtp_encryption: Option<String>,
    pub dns_check: bool,
    pub security_profile: String,
    pub ssh_policy: String,
    pub phase: String,
    pub state_path: PathBuf,
    pub owned_files_path: PathBuf,
    pub owned_files: Vec<String>,
    pub completed_steps: Vec<String>,
    pub safety_checks: Vec<InstallCheck>,
    pub preinstall_package_checks: Vec<InstallCheck>,
    pub package_checks: Vec<InstallCheck>,
    pub service_checks: Vec<InstallCheck>,
    pub port_checks: Vec<InstallCheck>,
    pub network_checks: Vec<InstallCheck>,
    pub runtime_checks: Vec<InstallCheck>,
    pub database_checks: Vec<InstallCheck>,
    pub firewall_checks: Vec<InstallCheck>,
    pub mail_checks: Vec<InstallCheck>,
    pub certbot_checks: Vec<InstallCheck>,
    pub vhost_checks: Vec<InstallCheck>,
    pub app_checks: Vec<InstallCheck>,
    pub setup_guide_path: PathBuf,
    pub backup_manifest_path: PathBuf,
    pub app_requirements: Vec<InstallCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallCheck {
    pub name: String,
    pub status: String,
    pub message: String,
}

impl InstallCheck {
    pub(super) fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "pass".to_string(),
            message: message.into(),
        }
    }

    pub(super) fn manual(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "manual".to_string(),
            message: message.into(),
        }
    }

    pub(super) fn warn(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "warn".to_string(),
            message: message.into(),
        }
    }

    pub(super) fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "fail".to_string(),
            message: message.into(),
        }
    }
}

pub(super) fn require_checks_passed(
    package_checks: &[InstallCheck],
    service_checks: &[InstallCheck],
    port_checks: &[InstallCheck],
) -> Result<()> {
    let failed = package_checks
        .iter()
        .chain(service_checks)
        .chain(port_checks)
        .filter(|check| check.status == "fail")
        .map(|check| format!("{}: {}", check.name, check.message))
        .collect::<Vec<String>>();

    if failed.is_empty() {
        Ok(())
    } else {
        Err(Error::InstallVerificationFailed {
            checks: failed.join(", "),
        })
    }
}

pub(super) fn require_success(
    step: &'static str,
    command: impl Into<String>,
    output: g7_system::command::CommandOutput,
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

pub(super) fn command_error(
    step: &'static str,
    command: impl Into<String>,
    err: impl ToString,
) -> Error {
    Error::InstallCommandFailed {
        step,
        command: command.into(),
        status: 128,
        stdout: String::new(),
        stderr: err.to_string(),
    }
}

pub(super) fn command_failure_message(prefix: &str, err: &Error) -> String {
    let mut message = format!("{prefix}: {err}");
    if let Some(details) = command_output_excerpt(err) {
        message.push_str("; ");
        message.push_str(&details);
    }
    message
}

pub(super) fn command_output_excerpt(err: &Error) -> Option<String> {
    match err {
        Error::InstallCommandFailed { stdout, stderr, .. } => {
            let mut parts = Vec::new();
            let stdout = short_text(stdout);
            if !stdout.is_empty() {
                parts.push(format!("stdout: {stdout}"));
            }
            let stderr = short_text(stderr);
            if !stderr.is_empty() {
                parts.push(format!("stderr: {stderr}"));
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" | "))
            }
        }
        _ => None,
    }
}

pub(super) fn is_letsencrypt_rate_limited(err: &Error) -> bool {
    let haystack = match err {
        Error::InstallCommandFailed { stdout, stderr, .. } => format!("{stdout}\n{stderr}\n{err}"),
        _ => err.to_string(),
    }
    .to_ascii_lowercase();

    haystack.contains("ratelimited")
        || haystack.contains("rate limited")
        || haystack.contains("too many certificates")
        || haystack.contains("too many requests")
}

pub(super) fn write_secret_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    write_new_file(paths, path, content, owned)?;
    #[cfg(unix)]
    {
        let target = paths.resolve(path);
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).map_err(|source| {
            Error::FileWriteFailed {
                path: path.to_string(),
                source,
            }
        })?;
    }
    Ok(())
}

pub(super) fn persist_progress(
    progress: &ProgressContext<'_>,
    owned_files: &mut OwnedFiles,
    owned_file_list: &[String],
    state: &InstallerState,
    plan: &plan::InstallPlan,
    summary: &ApplySummary,
    problem: Option<&str>,
) -> Result<()> {
    owned_files.files = owned_file_list.to_vec();
    write_owned_files(progress.owned_files_path, owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;
    write_existing_file(
        progress.paths,
        ROLLBACK_PATH,
        &rollback_content(owned_file_list),
    )?;
    write_state_file(progress.state_path, state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    write_existing_file(
        progress.paths,
        REPORT_PATH,
        &report_content(plan, &state.phase, summary, problem)?,
    )?;
    append_phase_log(progress.paths, &state.phase, problem.is_some())?;
    Ok(())
}

pub(super) fn append_phase_log(paths: &InstallPaths, phase: &str, failed: bool) -> Result<()> {
    let path = paths.resolve(LOG_PATH);
    let mut file = OpenOptions::new()
        .append(true)
        .open(&path)
        .map_err(|source| Error::FileWriteFailed {
            path: LOG_PATH.to_string(),
            source,
        })?;
    writeln!(
        file,
        "phase={phase} result={}",
        if failed { "failed" } else { "recorded" }
    )
    .map_err(|source| Error::FileWriteFailed {
        path: LOG_PATH.to_string(),
        source,
    })?;
    file.sync_data().map_err(|source| Error::FileWriteFailed {
        path: LOG_PATH.to_string(),
        source,
    })
}

pub(super) fn random_hex_secret() -> Result<String> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|source| Error::InstallVerificationFailed {
        checks: format!("failed to generate database password: {source}"),
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(super) fn random_laravel_app_key() -> Result<String> {
    use base64::Engine;

    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|source| Error::InstallVerificationFailed {
        checks: format!("failed to generate Laravel APP_KEY: {source}"),
    })?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub(super) fn config_content(plan: &plan::InstallPlan) -> String {
    let mut content = String::new();
    content.push_str(&format!("domain = \"{}\"\n", plan.domain));
    content.push_str(&format!("deployment_mode = \"{}\"\n", plan.deployment_mode));
    content.push_str("phase = \"prepared\"\n");
    content.push_str(&format!("app_profile = \"{}\"\n", plan.app_profile));
    content.push_str(&format!(
        "app_document_root = \"{}\"\n",
        plan.app_document_root
    ));
    content.push_str(&format!("web_server = \"{}\"\n", plan.web_server));
    content.push_str(&format!("php_version = \"{}\"\n", plan.php_version));
    content.push_str(&format!("php_source = \"{}\"\n", plan.php_source));
    content.push_str(&format!("database = \"{}\"\n", plan.database_engine));
    content.push_str(&format!("database_name = \"{}\"\n", plan.database_name));
    content.push_str(&format!("database_user = \"{}\"\n", plan.database_user));
    content.push_str(&format!(
        "database_password_policy = \"{}\"\n",
        plan.database_password_policy
    ));
    content.push_str(&format!("site_user = \"{}\"\n", plan.site_user));
    content.push_str(&format!("web_root_mode = \"{}\"\n", plan.web_root_mode));
    content.push_str(&format!("web_root = \"{}\"\n", plan.web_root));
    content.push_str(&format!("www_mode = \"{}\"\n", plan.www_mode));
    content.push_str(&format!("redis = \"{}\"\n", plan.redis_mode));
    content.push_str(&format!("mail_mode = \"{}\"\n", plan.mail_mode));
    content.push_str(&format!(
        "security_profile = \"{}\"\n",
        plan.security_profile
    ));
    content.push_str(&format!("ssh_policy = \"{}\"\n", plan.ssh_policy));
    content.push_str(&format!("rollback = {}\n", plan.rollback_enabled));
    content.push_str(&format!("preserve_config = {}\n", plan.preserve_config));
    content.push_str(&format!("dns_check = {}\n", plan.dns_check_required));

    if let Some(host) = &plan.smtp_host {
        content.push_str(&format!("smtp_host = \"{host}\"\n"));
    }
    if let Some(port) = plan.smtp_port {
        content.push_str(&format!("smtp_port = {port}\n"));
    }
    if let Some(from) = &plan.smtp_from {
        content.push_str(&format!("smtp_from = \"{from}\"\n"));
    }
    if let Some(encryption) = &plan.smtp_encryption {
        content.push_str(&format!("smtp_encryption = \"{encryption}\"\n"));
    }

    content
}

pub(super) fn rollback_content(owned: &[String]) -> String {
    let files = owned
        .iter()
        .map(|path| format!("    \"{path}\""))
        .collect::<Vec<String>>()
        .join(",\n");

    format!("{{\n  \"version\": 1,\n  \"created_paths\": [\n{files}\n  ]\n}}\n")
}

pub(super) fn backup_manifest_content(
    plan: &plan::InstallPlan,
    phase: &str,
    owned: &[String],
    completed_steps: &[String],
) -> Result<String> {
    let value = serde_json::json!({
        "version": 1,
        "kind": "g7-installer-recovery-manifest",
        "domain": &plan.domain,
        "phase": phase,
        "scope": "installer-created configuration/state manifest, not a full site data backup",
        "config_paths": {
            "config": CONFIG_PATH,
            "state": STATE_PATH,
            "owned_files": OWNED_FILES_PATH,
            "rollback": ROLLBACK_PATH,
            "report": REPORT_PATH,
            "setup_guide": SETUP_GUIDE_PATH,
            "secrets": SECRETS_PATH
        },
        "site_paths": {
            "web_root": &plan.web_root,
            "app_document_root": &plan.app_document_root
        },
        "certificate_policy": {
            "path": format!("/etc/letsencrypt/live/{}", plan.domain),
            "reset": "preserve existing Let's Encrypt certificates to avoid duplicate issuance limits"
        },
        "completed_steps": completed_steps,
        "owned_paths": owned
    });

    let mut payload =
        serde_json::to_string_pretty(&value).map_err(|source| Error::FileWriteFailed {
            path: BACKUP_MANIFEST_PATH.to_string(),
            source: io::Error::other(source),
        })?;
    payload.push('\n');
    Ok(payload)
}

pub(super) fn report_content(
    plan: &plan::InstallPlan,
    phase: &str,
    summary: &ApplySummary,
    problem: Option<&str>,
) -> Result<String> {
    let mut value = serde_json::Map::new();
    value.insert("schema_version".to_string(), serde_json::json!(1));
    value.insert("version".to_string(), serde_json::json!(1));
    value.insert("domain".to_string(), serde_json::json!(&plan.domain));
    value.insert("phase".to_string(), serde_json::json!(phase));
    value.insert(
        "deployment_mode".to_string(),
        serde_json::json!(&plan.deployment_mode),
    );
    value.insert(
        "app_package".to_string(),
        serde_json::json!(&plan.app_profile),
    );
    value.insert(
        "app_profile".to_string(),
        serde_json::json!(&plan.app_profile),
    );
    value.insert(
        "app_profile_label".to_string(),
        serde_json::json!(&plan.app_profile_label),
    );
    value.insert(
        "app_summary".to_string(),
        serde_json::json!(&plan.app_summary),
    );
    value.insert(
        "app_document_root".to_string(),
        serde_json::json!(&plan.app_document_root),
    );
    value.insert(
        "app_url".to_string(),
        serde_json::json!(app_access_url(plan, summary)),
    );
    value.insert(
        "web_server".to_string(),
        serde_json::json!(&plan.web_server),
    );
    value.insert(
        "php_version".to_string(),
        serde_json::json!(&plan.php_version),
    );
    value.insert(
        "php_source".to_string(),
        serde_json::json!(&plan.php_source),
    );
    value.insert(
        "database".to_string(),
        serde_json::json!(&plan.database_engine),
    );
    value.insert(
        "database_name".to_string(),
        serde_json::json!(&plan.database_name),
    );
    value.insert(
        "database_user".to_string(),
        serde_json::json!(&plan.database_user),
    );
    value.insert(
        "database_password_policy".to_string(),
        serde_json::json!(plan.database_password_policy),
    );
    value.insert("site_user".to_string(), serde_json::json!(&plan.site_user));
    value.insert(
        "web_root_mode".to_string(),
        serde_json::json!(&plan.web_root_mode),
    );
    value.insert("web_root".to_string(), serde_json::json!(&plan.web_root));
    value.insert("www_mode".to_string(), serde_json::json!(&plan.www_mode));
    value.insert("redis".to_string(), serde_json::json!(&plan.redis_mode));
    value.insert("mail_mode".to_string(), serde_json::json!(&plan.mail_mode));
    value.insert("smtp_host".to_string(), serde_json::json!(&plan.smtp_host));
    value.insert("smtp_port".to_string(), serde_json::json!(plan.smtp_port));
    value.insert("smtp_from".to_string(), serde_json::json!(&plan.smtp_from));
    value.insert(
        "smtp_encryption".to_string(),
        serde_json::json!(&plan.smtp_encryption),
    );
    value.insert(
        "dns_check".to_string(),
        serde_json::json!(plan.dns_check_required),
    );
    value.insert(
        "security_profile".to_string(),
        serde_json::json!(&plan.security_profile),
    );
    value.insert(
        "ssh_policy".to_string(),
        serde_json::json!(&plan.ssh_policy),
    );
    value.insert(
        "safety_checks".to_string(),
        serde_json::json!(checks_json(&summary.safety_checks)),
    );
    value.insert(
        "preinstall_package_checks".to_string(),
        serde_json::json!(checks_json(&summary.preinstall_package_checks)),
    );
    value.insert(
        "package_checks".to_string(),
        serde_json::json!(checks_json(&summary.package_checks)),
    );
    value.insert(
        "service_checks".to_string(),
        serde_json::json!(checks_json(&summary.service_checks)),
    );
    value.insert(
        "port_checks".to_string(),
        serde_json::json!(checks_json(&summary.port_checks)),
    );
    value.insert(
        "network_checks".to_string(),
        serde_json::json!(checks_json(&summary.network_checks)),
    );
    value.insert(
        "runtime_checks".to_string(),
        serde_json::json!(checks_json(&summary.runtime_checks)),
    );
    value.insert(
        "database_checks".to_string(),
        serde_json::json!(checks_json(&summary.database_checks)),
    );
    value.insert(
        "firewall_checks".to_string(),
        serde_json::json!(checks_json(&summary.firewall_checks)),
    );
    value.insert(
        "mail_checks".to_string(),
        serde_json::json!(checks_json(&summary.mail_checks)),
    );
    value.insert(
        "certbot_checks".to_string(),
        serde_json::json!(checks_json(&summary.certbot_checks)),
    );
    value.insert(
        "vhost_checks".to_string(),
        serde_json::json!(checks_json(&summary.vhost_checks)),
    );
    value.insert(
        "app_checks".to_string(),
        serde_json::json!(checks_json(&summary.app_checks)),
    );
    value.insert(
        "setup_guide_path".to_string(),
        serde_json::json!(SETUP_GUIDE_PATH),
    );
    value.insert(
        "backup_manifest_path".to_string(),
        serde_json::json!(BACKUP_MANIFEST_PATH),
    );
    value.insert(
        "app_requirements".to_string(),
        serde_json::json!(requirements_json(&plan.app_requirements)),
    );
    value.insert(
        "app_followup_steps".to_string(),
        serde_json::json!(followup_steps_json(&plan.app_followup_steps)),
    );
    value.insert("problem".to_string(), serde_json::json!(problem));
    let mut payload =
        serde_json::to_string_pretty(&serde_json::Value::Object(value)).map_err(|source| {
            Error::FileWriteFailed {
                path: REPORT_PATH.to_string(),
                source: io::Error::other(source),
            }
        })?;
    payload.push('\n');
    Ok(payload)
}

pub(super) fn checks_json(checks: &[InstallCheck]) -> Vec<serde_json::Value> {
    checks
        .iter()
        .map(|check| {
            serde_json::json!({
                "name": &check.name,
                "status": &check.status,
                "message": &check.message,
            })
        })
        .collect()
}

pub(super) fn requirements_json(
    requirements: &[crate::app_profile::AppRequirement],
) -> Vec<serde_json::Value> {
    requirements
        .iter()
        .map(|requirement| {
            serde_json::json!({
                "name": &requirement.name,
                "status": requirement.status,
                "message": &requirement.message,
            })
        })
        .collect()
}

pub(super) fn followup_steps_json(
    steps: &[crate::app_profile::AppFollowupStep],
) -> Vec<serde_json::Value> {
    steps
        .iter()
        .map(|step| {
            serde_json::json!({
                "name": step.name,
                "description": step.description,
            })
        })
        .collect()
}

pub(super) fn app_requirements_to_checks(
    requirements: Vec<crate::app_profile::AppRequirement>,
) -> Vec<InstallCheck> {
    requirements
        .into_iter()
        .map(|requirement| InstallCheck {
            name: requirement.name,
            status: requirement.status.to_string(),
            message: requirement.message,
        })
        .collect()
}

pub(super) fn local_hosts_content(domain: &str) -> String {
    format!(
        "# Add this on the test client if {domain} is not resolvable yet:\n127.0.0.1 {domain}\n"
    )
}

pub(super) fn setup_guide_content(
    plan: &plan::InstallPlan,
    phase: &str,
    summary: &ApplySummary,
    completed_steps: &[String],
) -> String {
    let web_service = web_service_name(plan);
    let fpm_service = format!("php{}-fpm", plan.php_version);
    let db_service = database_service_name(plan);
    let access_url = app_access_url(plan, summary);

    let mut content = String::new();
    content.push_str(&format!("# G7 Installer Setup Guide - {}\n\n", plan.domain));
    content.push_str("이 문서는 설치기가 만든 서버 구성을 사람이 확인하기 위한 안내서입니다. DB 비밀번호 같은 민감값은 화면에 쓰지 않고 root 전용 파일 경로만 표시합니다.\n\n");
    content.push_str("## 요약\n\n");
    content.push_str(&format!("- 도메인: `{}`\n", plan.domain));
    content.push_str(&format!("- 웹앱 접속 주소: `{access_url}`\n"));
    content.push_str(&format!("- 현재 단계: `{phase}`\n"));
    content.push_str(&format!(
        "- 앱 프로필: `{}` ({})\n",
        plan.app_profile, plan.app_profile_label
    ));
    content.push_str(&format!(
        "- 웹서버 / PHP: `{}` / `PHP {}` ({})\n",
        plan.web_server, plan.php_version, plan.php_source
    ));
    content.push_str(&format!("- DB: `{}`\n", plan.database_engine));
    content.push_str(&format!("- 사이트 계정: `{}`\n", plan.site_user));
    content.push_str("\n## 주요 경로\n\n");
    content.push_str(&format!("- 웹 루트: `{}`\n", plan.web_root));
    content.push_str(&format!("- 앱 문서 루트: `{}`\n", plan.app_document_root));
    content.push_str(&format!("- 웹앱 링크: `{access_url}`\n"));
    content.push_str(&format!("- 설치 상태: `{STATE_PATH}`\n"));
    content.push_str(&format!("- 설치 리포트(JSON): `{REPORT_PATH}`\n"));
    content.push_str(&format!("- 설정 안내서(MD): `{SETUP_GUIDE_PATH}`\n"));
    content.push_str(&format!(
        "- 복구 매니페스트(JSON): `{BACKUP_MANIFEST_PATH}`\n"
    ));
    content.push_str(&format!("- 기본 설정: `{CONFIG_PATH}`\n"));
    content.push_str(&format!("- 비밀 설정: `{SECRETS_PATH}`\n"));
    content.push_str("\n## 설정 파일\n\n");
    if plan.web_server == "nginx" {
        content.push_str(&format!(
            "- Nginx vhost: `{}`\n",
            g7_system::nginx::G7_SITE_AVAILABLE
        ));
        content.push_str(&format!(
            "- Nginx enabled: `{}`\n",
            g7_system::nginx::G7_SITE_ENABLED
        ));
    } else if plan.web_server == "apache" {
        content.push_str("- Apache vhost: `/etc/apache2/sites-available/g7.conf`\n");
        content.push_str("- Apache enabled: `/etc/apache2/sites-enabled/g7.conf`\n");
    } else {
        content.push_str("- Nginx edge vhost: `/etc/nginx/sites-available/g7.conf`\n");
        content.push_str("- Nginx edge enabled: `/etc/nginx/sites-enabled/g7.conf`\n");
        content.push_str(&format!(
            "- FrankenPHP binary: `{FRANKENPHP_BIN_PATH}` ({FRANKENPHP_VERSION})\n"
        ));
        content.push_str(&format!(
            "- FrankenPHP service: `{FRANKENPHP_SERVICE_PATH}` listening `{FRANKENPHP_LISTEN}`\n"
        ));
    }
    if plan.web_server != "frankenphp" {
        content.push_str(&format!("- PHP-FPM pool: `{}`\n", php_pool_path(plan)));
    }
    content.push_str(&format!(
        "- PHP override: `{}`\n",
        php_ini_override_path(plan)
    ));
    content.push_str(&format!("- DB tuning: `{}`\n", database_config_path(plan)));
    if plan.redis_mode == "enable" {
        content.push_str("- Redis config: `/etc/redis/redis.conf`\n");
    }
    for unit in app_runtime_unit_names(plan) {
        content.push_str(&format!(
            "- 앱 systemd unit: `{}`\n",
            systemd_unit_path(unit)
        ));
    }
    content.push_str("\n## 계정과 DB\n\n");
    content.push_str(&format!("- Linux 사이트 계정: `{}`\n", plan.site_user));
    content.push_str(&format!("- DB 이름: `{}`\n", plan.database_name));
    content.push_str(&format!(
        "- DB 사용자: `{}`@`localhost`\n",
        plan.database_user
    ));
    content.push_str(&format!(
        "- DB 비밀번호 위치: `{SECRETS_PATH}` (root만 읽기)\n"
    ));
    content.push_str("\n## 서비스 명령\n\n");
    content.push_str(&format!(
        "- 웹서버 상태: `sudo systemctl status {web_service}`\n"
    ));
    content.push_str(&format!(
        "- 웹서버 재시작: `sudo systemctl restart {web_service}`\n"
    ));
    if plan.web_server == "frankenphp" {
        content.push_str(&format!(
            "- FrankenPHP 상태: `sudo systemctl status {FRANKENPHP_SERVICE_NAME}`\n"
        ));
        content.push_str(&format!(
            "- FrankenPHP 재시작: `sudo systemctl restart {FRANKENPHP_SERVICE_NAME}`\n"
        ));
    } else {
        content.push_str(&format!(
            "- PHP-FPM 상태: `sudo systemctl status {fpm_service}`\n"
        ));
        content.push_str(&format!(
            "- PHP-FPM 재시작: `sudo systemctl restart {fpm_service}`\n"
        ));
    }
    content.push_str(&format!(
        "- DB 상태: `sudo systemctl status {db_service}`\n"
    ));
    content.push_str(&format!(
        "- DB 재시작: `sudo systemctl restart {db_service}`\n"
    ));
    if plan.redis_mode == "enable" {
        content.push_str("- Redis 상태: `sudo systemctl status redis-server`\n");
        content.push_str("- Redis 재시작: `sudo systemctl restart redis-server`\n");
    }
    if plan.mail_mode == "local-postfix" {
        content.push_str("- Postfix 상태: `sudo systemctl status postfix`\n");
        content.push_str("- Postfix 재시작: `sudo systemctl restart postfix`\n");
    }
    for unit in app_runtime_unit_names(plan) {
        if unit.ends_with(".timer") {
            content.push_str(&format!(
                "- 앱 타이머 상태: `sudo systemctl status {unit}`\n"
            ));
        } else if unit.ends_with("-scheduler.service") {
            content.push_str(&format!(
                "- 앱 스케줄러 수동 실행: `sudo systemctl start {unit}`\n"
            ));
        } else {
            content.push_str(&format!(
                "- 앱 서비스 상태: `sudo systemctl status {unit}`\n"
            ));
            content.push_str(&format!(
                "- 앱 서비스 재시작: `sudo systemctl restart {unit}`\n"
            ));
        }
    }
    content.push_str("- SSL 자동갱신 타이머: `sudo systemctl status certbot.timer`\n");
    content
        .push_str("- SSL 갱신 테스트: `sudo certbot renew --dry-run --no-random-sleep-on-renew`\n");
    content.push_str("\n## 설치 결과\n\n");
    content.push_str(&format!(
        "- 완료 단계: `{}`\n",
        completed_steps.join("`, `")
    ));
    content.push_str("\n### 런타임\n\n");
    content.push_str(&checks_markdown(&summary.runtime_checks));
    content.push_str("\n### DB\n\n");
    content.push_str(&checks_markdown(&summary.database_checks));
    content.push_str("\n### 방화벽\n\n");
    content.push_str(&checks_markdown(&summary.firewall_checks));
    content.push_str("\n### 메일\n\n");
    content.push_str(&checks_markdown(&summary.mail_checks));
    content.push_str("\n### SSL\n\n");
    content.push_str(&checks_markdown(&summary.certbot_checks));
    content.push_str("\n### 앱 설치 준비\n\n");
    content.push_str(&checks_markdown(&summary.app_checks));
    content.push_str("\n## 주의\n\n");
    content.push_str("- VPS 전체 복구는 제공자 스냅샷을 기준으로 처리합니다.\n");
    content.push_str("- 복구 매니페스트는 설치기가 만든 설정/상태 추적용이며 DB 덤프나 웹루트 운영 데이터 백업이 아닙니다.\n");
    content.push_str("- 설치기가 소유하지 않은 기존 서비스/파일은 자동 삭제하지 않습니다.\n");
    content.push_str("- PDF가 필요하면 이 Markdown을 웹 UI에서 표시한 뒤 브라우저 인쇄/PDF 저장으로 내보내는 방식을 권장합니다.\n");
    content
}

pub(super) fn checks_markdown(checks: &[InstallCheck]) -> String {
    if checks.is_empty() {
        return "- 기록된 항목 없음\n".to_string();
    }

    checks
        .iter()
        .map(|check| format!("- `{}` [{}] {}\n", check.name, check.status, check.message))
        .collect()
}

pub(super) fn install_id(domain: &str) -> String {
    let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    };

    format!("g7-{domain}-{seconds}")
}
