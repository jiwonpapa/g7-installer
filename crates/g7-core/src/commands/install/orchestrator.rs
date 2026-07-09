use super::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ApplySummary {
    pub(super) safety_checks: Vec<InstallCheck>,
    pub(super) preinstall_package_checks: Vec<InstallCheck>,
    pub(super) package_checks: Vec<InstallCheck>,
    pub(super) service_checks: Vec<InstallCheck>,
    pub(super) port_checks: Vec<InstallCheck>,
    pub(super) network_checks: Vec<InstallCheck>,
    pub(super) runtime_checks: Vec<InstallCheck>,
    pub(super) database_checks: Vec<InstallCheck>,
    pub(super) firewall_checks: Vec<InstallCheck>,
    pub(super) mail_checks: Vec<InstallCheck>,
    pub(super) certbot_checks: Vec<InstallCheck>,
    pub(super) vhost_checks: Vec<InstallCheck>,
    pub(super) app_checks: Vec<InstallCheck>,
}

#[derive(Debug)]
pub(super) struct PackagePhaseFailure {
    pub(super) error: Error,
    pub(super) summary: ApplySummary,
    pub(super) completed_steps: Vec<String>,
}

pub(super) struct ProgressContext<'a> {
    pub(super) paths: &'a InstallPaths,
    pub(super) state_path: &'a Path,
    pub(super) owned_files_path: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    root: PathBuf,
}

impl InstallPaths {
    pub fn system() -> Self {
        Self {
            root: PathBuf::from("/"),
        }
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(super) fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);

        if self.root == Path::new("/") {
            return path.to_path_buf();
        }

        match path.strip_prefix("/") {
            Ok(stripped) => self.root.join(stripped),
            Err(_) => self.root.join(path),
        }
    }
}

pub fn run(domain: String, options: plan::PlanOptions) -> Result<InstallReport> {
    run_with_probe_and_paths(
        domain,
        options,
        &SystemProbe::real(),
        &InstallPaths::system(),
    )
}

pub fn run_with_probe_and_paths<R: CommandRunner>(
    domain: String,
    options: plan::PlanOptions,
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
) -> Result<InstallReport> {
    let site_user_password = options.site_user_password.clone();
    let database_password = options.database_password.clone();
    let install_plan = plan::build_with_options(domain, options)?;
    let doctor_report = doctor::run_with_probe(probe);

    require_root(&doctor_report)?;
    require_install_allowed(&doctor_report)?;

    let mut owned = Vec::new();
    create_owned_dir(paths, ETC_DIR, &mut owned)?;
    create_owned_dir(paths, LIB_DIR, &mut owned)?;
    create_owned_dir(paths, LOG_DIR, &mut owned)?;
    create_owned_dir(paths, BACKUP_DIR, &mut owned)?;

    write_new_file(
        paths,
        CONFIG_PATH,
        &config_content(&install_plan),
        &mut owned,
    )?;
    write_new_file(paths, LOG_PATH, "G7 installer prepared.\n", &mut owned)?;
    write_new_file(paths, ROLLBACK_PATH, &rollback_content(&owned), &mut owned)?;
    write_new_file(
        paths,
        REPORT_PATH,
        &report_content(&install_plan, "prepared", &ApplySummary::default(), None)?,
        &mut owned,
    )?;
    let mut optional_steps = Vec::new();
    if install_plan.deployment_mode == "local-test" {
        write_new_file(
            paths,
            LOCAL_HOSTS_PATH,
            &local_hosts_content(&install_plan.domain),
            &mut owned,
        )?;
        optional_steps.push("local-hosts-suggestion-written".to_string());
    }

    let mut owned_file_list = owned.clone();
    owned_file_list.push(STATE_PATH.to_string());
    owned_file_list.push(OWNED_FILES_PATH.to_string());
    let mut owned_files = OwnedFiles {
        version: 1,
        files: owned_file_list.clone(),
    };

    let owned_files_path = paths.resolve(OWNED_FILES_PATH);
    write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;

    let mut completed_steps = vec![
        "preflight-passed".to_string(),
        "directories-created".to_string(),
        "config-written".to_string(),
        "log-created".to_string(),
        "rollback-prepared".to_string(),
        "problem-report-prepared".to_string(),
    ];
    completed_steps.extend(optional_steps);
    completed_steps.push("owned-files-written".to_string());
    let mut state = InstallerState::new(
        install_id(&install_plan.domain),
        install_plan.domain.clone(),
    );
    state.set_phase(InstallerPhase::Prepared);
    state.completed_steps = completed_steps.clone();

    let state_path = paths.resolve(STATE_PATH);
    let progress = ProgressContext {
        paths,
        state_path: &state_path,
        owned_files_path: &owned_files_path,
    };
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    completed_steps.push("state-written".to_string());

    let mut apply_summary = match apply_package_phase(probe, &install_plan) {
        Ok(summary) => summary,
        Err(failure) => {
            let failure = *failure;
            let err = failure.error;
            let mut failed_summary = failure.summary;
            failed_summary.safety_checks = safety_checks(&install_plan, "package-failed");
            completed_steps.extend(failure.completed_steps);
            state.set_phase(InstallerPhase::PackageFailed);
            state.completed_steps = completed_steps.clone();
            write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
                path: STATE_PATH.to_string(),
                source,
            })?;
            write_existing_file(
                paths,
                REPORT_PATH,
                &report_content(
                    &install_plan,
                    &state.phase,
                    &failed_summary,
                    Some(&err.to_string()),
                )?,
            )?;
            return Err(err);
        }
    };

    completed_steps.push("apt-updated".to_string());
    if install_plan.php_source == g7_system::php::PHP_SOURCE_ONDREJ {
        completed_steps.push("php-apt-source-added".to_string());
        completed_steps.push("apt-updated-after-php-source".to_string());
    }
    completed_steps.push("package-candidates-checked".to_string());
    completed_steps.push("packages-installed".to_string());
    completed_steps.push("services-enabled".to_string());
    completed_steps.push("package-verification-passed".to_string());
    completed_steps.push("service-verification-passed".to_string());
    completed_steps.push("port-verification-passed".to_string());
    completed_steps.push("network-readiness-checked".to_string());
    completed_steps.push("mail-readiness-checked".to_string());
    completed_steps.push("certbot-readiness-checked".to_string());
    apply_summary.safety_checks = safety_checks(&install_plan, "packages-installed");
    state.set_phase(InstallerPhase::PackagesInstalled);
    state.completed_steps = completed_steps.clone();
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    write_existing_file(
        paths,
        REPORT_PATH,
        &report_content(&install_plan, &state.phase, &apply_summary, None)?,
    )?;

    let site_checks = match apply_site_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        site_user_password.as_deref(),
    ) {
        Ok(site_checks) => site_checks,
        Err(err) => {
            apply_summary.safety_checks = safety_checks(&install_plan, "vhost-failed");
            apply_summary.vhost_checks = vec![InstallCheck::fail(
                "site-provision",
                format!("Site account and web root setup failed: {err}"),
            )];
            state.set_phase(InstallerPhase::VhostFailed);
            state.completed_steps = completed_steps.clone();
            owned_files.files = owned_file_list.clone();
            write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
                Error::FileWriteFailed {
                    path: OWNED_FILES_PATH.to_string(),
                    source,
                }
            })?;
            write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
            write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
                path: STATE_PATH.to_string(),
                source,
            })?;
            write_existing_file(
                paths,
                REPORT_PATH,
                &report_content(
                    &install_plan,
                    &state.phase,
                    &apply_summary,
                    Some(&err.to_string()),
                )?,
            )?;
            return Err(err);
        }
    };

    apply_summary.vhost_checks = site_checks;
    completed_steps.push("site-user-verified".to_string());
    if site_user_password.is_some() {
        completed_steps.push("site-user-password-set".to_string());
    }
    completed_steps.push("web-root-created".to_string());
    state.completed_steps = completed_steps.clone();
    owned_files.files = owned_file_list.clone();
    write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;
    write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    write_existing_file(
        paths,
        REPORT_PATH,
        &report_content(&install_plan, &state.phase, &apply_summary, None)?,
    )?;

    match apply_vhost_phase(probe, paths, &install_plan, &mut owned_file_list) {
        Ok(vhost_checks) => {
            if !vhost_checks.is_empty() {
                let frankenphp_service_active = install_plan.web_server == "frankenphp"
                    && vhost_checks
                        .iter()
                        .any(|check| check.name == "frankenphp-service" && check.status == "pass");
                apply_summary.vhost_checks.extend(vhost_checks);
                if frankenphp_service_active
                    && !apply_summary
                        .service_checks
                        .iter()
                        .any(|check| check.name == FRANKENPHP_SERVICE_NAME)
                {
                    apply_summary.service_checks.push(InstallCheck::pass(
                        FRANKENPHP_SERVICE_NAME,
                        format!("FrankenPHP app server is active on {FRANKENPHP_LISTEN}."),
                    ));
                }
                completed_steps.push("vhost-written".to_string());
                completed_steps.push("vhost-enabled".to_string());
                completed_steps.push(format!("{}-config-tested", web_service_name(&install_plan)));
                completed_steps.push(format!("{}-reloaded", web_service_name(&install_plan)));
                completed_steps.push("http-smoke-passed".to_string());
                apply_summary.safety_checks = safety_checks(&install_plan, "vhost-enabled");
                state.set_phase(InstallerPhase::VhostEnabled);
                state.completed_steps = completed_steps.clone();
                owned_files.files = owned_file_list.clone();
                write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
                    Error::FileWriteFailed {
                        path: OWNED_FILES_PATH.to_string(),
                        source,
                    }
                })?;
                write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
                write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
                    path: STATE_PATH.to_string(),
                    source,
                })?;
                write_existing_file(
                    paths,
                    REPORT_PATH,
                    &report_content(&install_plan, &state.phase, &apply_summary, None)?,
                )?;
            } else {
                apply_summary.safety_checks = safety_checks(&install_plan, "packages-installed");
                apply_summary
                    .vhost_checks
                    .extend(deferred_vhost_checks(&install_plan));
                write_existing_file(
                    paths,
                    REPORT_PATH,
                    &report_content(&install_plan, &state.phase, &apply_summary, None)?,
                )?;
            }
        }
        Err(err) => {
            apply_summary.safety_checks = safety_checks(&install_plan, "vhost-failed");
            apply_summary.vhost_checks = vec![InstallCheck::fail(
                "webserver-vhost",
                format!("Web server vhost setup failed: {err}"),
            )];
            if install_plan.web_server == "frankenphp"
                && owned_file_list
                    .iter()
                    .any(|path| path == FRANKENPHP_SERVICE_PATH)
                && !apply_summary
                    .service_checks
                    .iter()
                    .any(|check| check.name == FRANKENPHP_SERVICE_NAME)
            {
                apply_summary.service_checks.push(InstallCheck::manual(
                    FRANKENPHP_SERVICE_NAME,
                    "FrankenPHP unit was created before vhost setup failed; rollback/reset may disable it.",
                ));
            }
            state.set_phase(InstallerPhase::VhostFailed);
            state.completed_steps = completed_steps.clone();
            owned_files.files = owned_file_list.clone();
            write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
                Error::FileWriteFailed {
                    path: OWNED_FILES_PATH.to_string(),
                    source,
                }
            })?;
            write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
            write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
                path: STATE_PATH.to_string(),
                source,
            })?;
            write_existing_file(
                paths,
                REPORT_PATH,
                &report_content(
                    &install_plan,
                    &state.phase,
                    &apply_summary,
                    Some(&err.to_string()),
                )?,
            )?;
            return Err(err);
        }
    }

    match apply_runtime_phase(probe, paths, &install_plan, &mut owned_file_list) {
        Ok(runtime_checks) => {
            apply_summary.runtime_checks = runtime_checks;
            if let Some(message) = blocking_runtime_failure(&apply_summary.runtime_checks) {
                state.completed_steps = completed_steps.clone();
                persist_progress(
                    &progress,
                    &mut owned_files,
                    &owned_file_list,
                    &state,
                    &install_plan,
                    &apply_summary,
                    Some(&message),
                )?;
                return Err(Error::InstallVerificationFailed { checks: message });
            }
            completed_steps.push(if install_plan.web_server == "frankenphp" {
                "frankenphp-runtime-config-written".to_string()
            } else {
                "php-fpm-config-written".to_string()
            });
            completed_steps.push("php-runtime-config-written".to_string());
            completed_steps.push("php-runtime-diagnostics-passed".to_string());
            completed_steps.push(format!(
                "{}-runtime-config-written",
                web_service_name(&install_plan)
            ));
            completed_steps.push(format!(
                "{}-runtime-reloaded",
                web_service_name(&install_plan)
            ));
            apply_summary.safety_checks = safety_checks(&install_plan, "runtime-configured");
            state.set_phase(InstallerPhase::RuntimeConfigured);
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.runtime_checks = vec![InstallCheck::fail(
                "runtime-config",
                format!("Runtime configuration failed: {err}"),
            )];
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                Some(&err.to_string()),
            )?;
            return Err(err);
        }
    }

    match apply_database_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        database_password.as_deref(),
    ) {
        Ok(database_checks) => {
            apply_summary.database_checks = database_checks;
            completed_steps.push("database-runtime-configured".to_string());
            completed_steps.push("database-secret-written".to_string());
            completed_steps.push("database-created".to_string());
            completed_steps.push("database-user-created".to_string());
            apply_summary.safety_checks = safety_checks(&install_plan, "database-configured");
            state.set_phase(InstallerPhase::DatabaseConfigured);
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.database_checks = vec![InstallCheck::fail(
                "database-config",
                format!("Database configuration failed: {err}"),
            )];
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                Some(&err.to_string()),
            )?;
            return Err(err);
        }
    }

    let (firewall_checks, mail_checks, certbot_checks, app_checks) =
        apply_post_database_guidance(&install_plan);
    apply_summary.firewall_checks = firewall_checks;
    apply_summary.mail_checks.extend(mail_checks);
    apply_summary.certbot_checks = certbot_checks;
    apply_summary.app_checks = app_checks;

    match apply_tls_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        &apply_summary.network_checks,
    ) {
        Ok(certbot_checks) => {
            let tls_passed = certbot_checks
                .iter()
                .any(|check| check.name == "tls-certificate" && check.status == "pass");
            let tls_skipped = certbot_checks
                .iter()
                .any(|check| check.name == "tls" && check.status == "skipped");
            apply_summary.certbot_checks = certbot_checks;
            if tls_passed {
                completed_steps.push("certbot-issued".to_string());
                completed_steps.push("https-vhost-written".to_string());
                completed_steps.push("certbot-renew-dry-run".to_string());
                state.set_phase(InstallerPhase::TlsEnabled);
            } else if tls_skipped {
                completed_steps.push("tls-skipped".to_string());
            } else {
                completed_steps.push("tls-deferred".to_string());
            }
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.certbot_checks = if is_letsencrypt_rate_limited(&err) {
                vec![InstallCheck::warn(
                    "tls-rate-limited",
                    command_failure_message(
                        "Let's Encrypt 발급 제한으로 TLS 설정을 보류했습니다. 기존 인증서가 없으면 HTTP로 먼저 진행하고 제한 해제 후 SSL을 다시 적용하세요",
                        &err,
                    ),
                )]
            } else {
                vec![InstallCheck::fail(
                    "tls-config",
                    command_failure_message("TLS configuration failed", &err),
                )]
            };
            completed_steps.push("tls-deferred".to_string());
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
    }

    match apply_app_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        &apply_summary,
    ) {
        Ok(app_checks) => {
            let app_source_ready = app_checks
                .iter()
                .any(|check| check.name == "app-source" && check.status == "pass");
            apply_summary.app_checks = app_checks;
            completed_steps.push(if app_source_ready {
                "app-source-prepared".to_string()
            } else {
                "app-source-deferred".to_string()
            });
            completed_steps.push("app-link-ready".to_string());
            if state.phase != InstallerPhase::TlsEnabled.as_str() {
                state.set_phase(InstallerPhase::AppConfigured);
            }
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.app_checks = vec![InstallCheck::fail(
                "app-source",
                command_failure_message("Application source setup failed", &err),
            )];
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                Some(&err.to_string()),
            )?;
            return Err(err);
        }
    }

    completed_steps.push("setup-guide-written".to_string());
    if state.phase == InstallerPhase::TlsEnabled.as_str()
        || completed_steps.iter().any(|step| step == "tls-skipped")
    {
        state.set_phase(InstallerPhase::Completed);
    }
    state.completed_steps = completed_steps.clone();
    write_new_file(
        paths,
        SETUP_GUIDE_PATH,
        &setup_guide_content(
            &install_plan,
            &state.phase,
            &apply_summary,
            &completed_steps,
        ),
        &mut owned_file_list,
    )?;
    write_tracked_file(
        paths,
        BACKUP_MANIFEST_PATH,
        &backup_manifest_content(
            &install_plan,
            &state.phase,
            &owned_file_list,
            &completed_steps,
        )?,
        &mut owned_file_list,
    )?;
    completed_steps.push("backup-manifest-written".to_string());
    state.completed_steps = completed_steps.clone();
    persist_progress(
        &progress,
        &mut owned_files,
        &owned_file_list,
        &state,
        &install_plan,
        &apply_summary,
        None,
    )?;

    let app_url = app_access_url(&install_plan, &apply_summary);

    Ok(InstallReport {
        domain: state.domain,
        deployment_mode: install_plan.deployment_mode,
        app_profile: install_plan.app_profile,
        app_profile_label: install_plan.app_profile_label,
        app_document_root: install_plan.app_document_root,
        web_server: install_plan.web_server,
        php_version: install_plan.php_version,
        php_source: install_plan.php_source,
        database_engine: install_plan.database_engine,
        database_name: install_plan.database_name,
        database_user: install_plan.database_user,
        database_password_policy: install_plan.database_password_policy,
        site_user: install_plan.site_user,
        web_root_mode: install_plan.web_root_mode,
        web_root: install_plan.web_root.clone(),
        app_url,
        www_mode: install_plan.www_mode,
        redis_mode: install_plan.redis_mode,
        mail_mode: install_plan.mail_mode,
        smtp_host: install_plan.smtp_host,
        smtp_port: install_plan.smtp_port,
        smtp_from: install_plan.smtp_from,
        smtp_encryption: install_plan.smtp_encryption,
        dns_check: install_plan.dns_check_required,
        security_profile: install_plan.security_profile,
        ssh_policy: install_plan.ssh_policy,
        phase: state.phase,
        state_path,
        owned_files_path,
        owned_files: owned_files.files,
        completed_steps,
        safety_checks: apply_summary.safety_checks,
        preinstall_package_checks: apply_summary.preinstall_package_checks,
        package_checks: apply_summary.package_checks,
        service_checks: apply_summary.service_checks,
        port_checks: apply_summary.port_checks,
        network_checks: apply_summary.network_checks,
        runtime_checks: apply_summary.runtime_checks,
        database_checks: apply_summary.database_checks,
        firewall_checks: apply_summary.firewall_checks,
        mail_checks: apply_summary.mail_checks,
        certbot_checks: apply_summary.certbot_checks,
        vhost_checks: apply_summary.vhost_checks,
        app_checks: apply_summary.app_checks,
        setup_guide_path: paths.resolve(SETUP_GUIDE_PATH),
        backup_manifest_path: paths.resolve(BACKUP_MANIFEST_PATH),
        app_requirements: app_requirements_to_checks(install_plan.app_requirements),
    })
}

pub(super) fn require_root(report: &doctor::DoctorReport) -> Result<()> {
    let root = report
        .checks
        .iter()
        .any(|check| check.name == "privilege" && check.status == DoctorCheckStatus::Pass);

    if root {
        Ok(())
    } else {
        Err(Error::PrivilegeRequired)
    }
}

pub(super) fn require_install_allowed(report: &doctor::DoctorReport) -> Result<()> {
    if report.install_allowed {
        return Ok(());
    }

    let checks = report
        .checks
        .iter()
        .filter(|check| {
            matches!(
                check.status,
                DoctorCheckStatus::Fail | DoctorCheckStatus::Pending
            )
        })
        .map(|check| check.name)
        .collect::<Vec<_>>()
        .join(", ");

    Err(Error::InstallBlocked { checks })
}
