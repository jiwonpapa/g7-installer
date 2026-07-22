use super::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ApplySummary {
    pub(super) install_started_at_unix_ms: u128,
    pub(super) php_apt_source_added: bool,
    pub(super) mysql_apt_source_added: bool,
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

pub fn run(domain: String, options: plan::PlanOptions) -> Result<InstallReport> {
    run_with_probe_and_paths(
        domain,
        options,
        &SystemProbe::real(),
        &InstallPaths::system(),
    )
}

pub fn resume() -> Result<InstallReport> {
    resume_with_probe_and_paths(&SystemProbe::real(), &InstallPaths::system())
}

pub fn resume_with_probe_and_paths<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
) -> Result<InstallReport> {
    let _operation_lock =
        g7_state::lock::InstallerLock::acquire(&paths.resolve(g7_state::lock::LOCK_PATH), "resume")
            .map_err(|source| Error::OperationLocked {
                operation: "resume",
                source,
            })?;
    require_root(&doctor::run_with_probe(probe))?;

    let state_path = paths.resolve(STATE_PATH);
    let owned_files_path = paths.resolve(OWNED_FILES_PATH);
    let mut state = read_state_file(&state_path).map_err(|source| Error::FileReadFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    if state.phase == InstallerPhase::Completed.as_str() {
        return Err(Error::ResumeUnavailable {
            reason: "installation is already completed".to_string(),
        });
    }
    let report_value = read_report_value(paths)?;
    let mut options = plan_options_from_report(&report_value)?;
    options.database_password = read_database_password(paths)?;
    options.smtp_password = read_smtp_password(paths)?;
    let mut install_plan = plan::build_with_options(state.domain.clone(), options)?;
    let mut apply_summary = apply_summary_from_report(&report_value);
    let mut owned_files =
        read_owned_files(&owned_files_path).map_err(|source| Error::FileReadFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        })?;
    let mut owned_file_list = owned_files.files.clone();
    let mut completed_steps = state.completed_steps.clone();
    let progress = ProgressContext {
        paths,
        state_path: &state_path,
        owned_files_path: &owned_files_path,
    };

    if let Some(step) = state.current_step.clone() {
        if restore_unfinished_transaction(paths, &state.install_id, &step)? {
            let error = state
                .steps
                .iter()
                .find(|record| record.id == step)
                .and_then(|record| record.last_error.clone())
                .unwrap_or_else(|| "이전 실행이 설정 적용 중 중단되었습니다.".to_string());
            state.fail_step(&step, error, true);
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

    resume_pre_tls_steps(
        probe,
        paths,
        &mut install_plan,
        &mut state,
        &mut apply_summary,
        &mut owned_files,
        &mut owned_file_list,
        &mut completed_steps,
        &progress,
    )?;

    if !tls_is_ready(&apply_summary.certbot_checks) {
        let transaction = StepTransaction::begin(
            paths,
            &state.install_id,
            "tls",
            &tls_transaction_files(&install_plan),
        )?;
        state.begin_step("tls");
        persist_resume_step(
            &progress,
            &mut owned_files,
            &owned_file_list,
            &mut state,
            &install_plan,
            &apply_summary,
            None,
        )?;
        match apply_tls_phase(
            probe,
            paths,
            &install_plan,
            &mut owned_file_list,
            &apply_summary.network_checks,
        ) {
            Ok(checks) => {
                let tls_passed = checks
                    .iter()
                    .any(|check| check.name == "tls-certificate" && check.status == "pass");
                apply_summary.certbot_checks = checks;
                if tls_passed {
                    mark_step(
                        &mut completed_steps,
                        if tls_certificate_was_reused(&apply_summary.certbot_checks) {
                            "certbot-reused"
                        } else {
                            "certbot-issued"
                        },
                    );
                    mark_step(&mut completed_steps, "https-vhost-written");
                    mark_step(&mut completed_steps, "certbot-renew-dry-run");
                    state.set_phase(InstallerPhase::TlsEnabled);
                } else if tls_is_ready(&apply_summary.certbot_checks) {
                    mark_step(&mut completed_steps, "tls-skipped");
                }
                transaction.complete()?;
                if tls_passed || tls_is_ready(&apply_summary.certbot_checks) {
                    state.complete_step("tls");
                } else {
                    state.fail_step("tls", "TLS 설정이 보류되었습니다.", false);
                }
            }
            Err(err) => {
                let restored = transaction.restore().is_ok();
                if restored {
                    let _ = reload_restored_web_service(probe, &install_plan);
                }
                apply_summary.certbot_checks = if is_letsencrypt_rate_limited(&err) {
                    vec![InstallCheck::warn(
                        "tls-rate-limited",
                        command_failure_message("Let's Encrypt issuance remains deferred", &err),
                    )]
                } else {
                    vec![InstallCheck::fail(
                        "tls-config",
                        command_failure_message("TLS configuration failed", &err),
                    )]
                };
                mark_step(&mut completed_steps, "tls-deferred");
                state.fail_step("tls", err.to_string(), restored);
            }
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

    let app_ready = app_is_ready(&completed_steps, &apply_summary.app_checks);
    if !app_ready {
        let transaction = StepTransaction::begin(
            paths,
            &state.install_id,
            "app",
            &app_transaction_files(&install_plan),
        )?;
        state.begin_step("app");
        persist_resume_step(
            &progress,
            &mut owned_files,
            &owned_file_list,
            &mut state,
            &install_plan,
            &apply_summary,
            None,
        )?;
        match apply_app_phase(
            probe,
            paths,
            &install_plan,
            &mut owned_file_list,
            &apply_summary,
        ) {
            Ok(checks) => {
                let source_ready = checks
                    .iter()
                    .any(|check| check.name == "app-source" && check.status == "pass");
                apply_summary.app_checks = checks;
                mark_step(
                    &mut completed_steps,
                    if source_ready {
                        "app-source-prepared"
                    } else {
                        "app-source-deferred"
                    },
                );
                mark_step(&mut completed_steps, "app-link-ready");
                if state.phase != InstallerPhase::TlsEnabled.as_str() {
                    state.set_phase(InstallerPhase::AppConfigured);
                }
                if source_ready {
                    state.complete_step("app");
                } else {
                    state.fail_step("app", "웹앱 소스 준비가 보류되었습니다.", false);
                }
                transaction.complete()?;
            }
            Err(err) => {
                let restored = transaction.restore().is_ok();
                if restored {
                    let _ = reload_restored_app_runtime(probe, &install_plan);
                }
                apply_summary.app_checks = vec![InstallCheck::fail(
                    "app-source",
                    command_failure_message("Application source setup failed", &err),
                )];
                state.fail_step("app", err.to_string(), restored);
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
    }

    let tls_ready = tls_is_ready(&apply_summary.certbot_checks);
    let app_ready = app_is_ready(&completed_steps, &apply_summary.app_checks);
    if tls_ready && app_ready {
        state.set_phase(InstallerPhase::Completed);
    }
    write_existing_file(
        paths,
        CONFIG_PATH,
        &config_content_for_phase(&install_plan, &state.phase),
    )?;
    mark_step(&mut completed_steps, "setup-guide-written");
    state.completed_steps = completed_steps.clone();
    write_or_update_tracked_file(
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
    write_or_update_tracked_file(
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
    mark_step(&mut completed_steps, "backup-manifest-written");
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

    if !tls_ready || !app_ready {
        let reason = if !tls_ready && !app_ready {
            "TLS and application setup are still deferred"
        } else if !tls_ready {
            "TLS is still deferred"
        } else {
            "application source setup is still deferred"
        };
        return Err(Error::ResumeUnavailable {
            reason: format!("{reason}; the report contains the current failure"),
        });
    }

    Ok(build_install_report(
        paths,
        state_path,
        owned_files_path,
        state,
        install_plan,
        apply_summary,
        owned_files,
        completed_steps,
    ))
}

pub fn run_with_probe_and_paths<R: CommandRunner>(
    domain: String,
    options: plan::PlanOptions,
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
) -> Result<InstallReport> {
    let _operation_lock = g7_state::lock::InstallerLock::acquire(
        &paths.resolve(g7_state::lock::LOCK_PATH),
        "install",
    )
    .map_err(|source| Error::OperationLocked {
        operation: "install",
        source,
    })?;
    let site_user_password = options.site_user_password.clone();
    let database_password = Some(
        options
            .database_password
            .clone()
            .unwrap_or(random_hex_secret()?),
    );
    let smtp_password = options.smtp_password.clone();
    let mut install_plan = plan::build_with_options(domain, options)?;
    let doctor_report = doctor::run_with_probe(probe);

    require_root(&doctor_report)?;
    require_install_allowed(&doctor_report)?;

    let mut owned = Vec::new();
    create_owned_dir(paths, LIB_DIR, &mut owned)?;
    let owned_files_path = paths.resolve(OWNED_FILES_PATH);
    let mut reserved = vec![
        LIB_DIR.to_string(),
        TRANSACTION_DIR.to_string(),
        ETC_DIR.to_string(),
        LOG_DIR.to_string(),
        BACKUP_DIR.to_string(),
        CONFIG_PATH.to_string(),
        LOG_PATH.to_string(),
        ROLLBACK_PATH.to_string(),
        REPORT_PATH.to_string(),
        STATE_PATH.to_string(),
        OWNED_FILES_PATH.to_string(),
        COMMAND_AUDIT_LOG_PATH.to_string(),
        MYSQL_CONFIG_CANDIDATE_PATH.to_string(),
    ];
    if install_plan.deployment_mode == "local-test" {
        reserved.push(LOCAL_HOSTS_PATH.to_string());
    }
    reserved.sort();
    reserved.dedup();
    let mut owned_files = OwnedFiles {
        version: 1,
        files: reserved,
    };
    write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;

    create_owned_dir(paths, ETC_DIR, &mut owned)?;
    create_owned_dir(paths, LOG_DIR, &mut owned)?;
    create_owned_dir(paths, BACKUP_DIR, &mut owned)?;
    create_owned_dir(paths, TRANSACTION_DIR, &mut owned)?;

    write_secret_file(
        paths,
        PENDING_SECRETS_PATH,
        &pending_secrets_content(
            database_password.as_deref().unwrap_or_default(),
            site_user_password.as_deref(),
            smtp_password.as_deref(),
        ),
        &mut owned,
    )?;

    write_new_file(
        paths,
        CONFIG_PATH,
        &config_content(&install_plan),
        &mut owned,
    )?;
    write_new_file(paths, LOG_PATH, "G7 installer prepared.\n", &mut owned)?;
    write_new_file(paths, ROLLBACK_PATH, &rollback_content(&owned), &mut owned)?;
    let install_started_at_unix_ms = unix_timestamp_millis();
    let initial_summary = ApplySummary {
        install_started_at_unix_ms,
        ..ApplySummary::default()
    };
    write_new_file(
        paths,
        REPORT_PATH,
        &report_content(&install_plan, "prepared", &initial_summary, None)?,
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
    owned_file_list.push(COMMAND_AUDIT_LOG_PATH.to_string());
    owned_file_list.sort();
    owned_file_list.dedup();
    owned_files.files = owned_file_list.clone();

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
    append_phase_log(paths, &state.phase, false)?;
    completed_steps.push("state-written".to_string());

    let sizing = detected_memory_sizing(probe);
    let early_runtime_checks =
        apply_swap_configuration(probe, paths, &sizing, &mut owned_file_list)?;
    completed_steps.push("swap-configured-before-packages".to_string());
    owned_file_list.sort();
    owned_file_list.dedup();
    owned_files.files = owned_file_list.clone();
    write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;
    write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
    state.completed_steps = completed_steps.clone();
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;

    state.begin_step("packages");
    state.completed_steps = completed_steps.clone();
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    let mut apply_summary = match apply_package_phase(probe, paths, &mut install_plan) {
        Ok(summary) => summary,
        Err(failure) => {
            let failure = *failure;
            let err = failure.error;
            let problem = command_failure_message("패키지 설치 단계 실패", &err);
            let mut failed_summary = failure.summary;
            failed_summary.install_started_at_unix_ms = install_started_at_unix_ms;
            failed_summary.safety_checks = safety_checks(&install_plan, "package-failed");
            completed_steps.extend(failure.completed_steps);
            state.set_phase(InstallerPhase::PackageFailed);
            state.fail_step("packages", problem.clone(), false);
            state.completed_steps = completed_steps.clone();
            write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
                path: STATE_PATH.to_string(),
                source,
            })?;
            write_existing_file(
                paths,
                REPORT_PATH,
                &report_content(&install_plan, &state.phase, &failed_summary, Some(&problem))?,
            )?;
            return Err(err);
        }
    };
    apply_summary.install_started_at_unix_ms = install_started_at_unix_ms;
    state.complete_step("packages");
    apply_summary.runtime_checks.extend(early_runtime_checks);

    completed_steps.push("apt-updated".to_string());
    if apply_summary.php_apt_source_added {
        completed_steps.push("php-apt-source-added".to_string());
        completed_steps.push("apt-updated-after-php-source".to_string());
    }
    if apply_summary.mysql_apt_source_added {
        completed_steps.push("mysql-apt-source-added".to_string());
        completed_steps.push("apt-updated-after-mysql-source".to_string());
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
    append_phase_log(paths, &state.phase, false)?;

    let site_transaction = StepTransaction::begin(
        paths,
        &state.install_id,
        "site",
        &[ready_probe_path(&install_plan)],
    )?;
    state.begin_step("site");
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
    let site_checks = match apply_site_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        site_user_password.as_deref(),
    ) {
        Ok(site_checks) => site_checks,
        Err(err) => {
            let restored = site_transaction.restore().is_ok();
            apply_summary.safety_checks = safety_checks(&install_plan, "vhost-failed");
            apply_summary.vhost_checks = vec![InstallCheck::fail(
                "site-provision",
                format!("Site account and web root setup failed: {err}"),
            )];
            state.set_phase(InstallerPhase::VhostFailed);
            state.fail_step("site", err.to_string(), restored);
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
    site_transaction.complete()?;
    state.complete_step("site");

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

    let runtime_transaction = StepTransaction::begin(
        paths,
        &state.install_id,
        "runtime",
        &runtime_transaction_files(&install_plan),
    )?;
    state.begin_step("runtime");
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
    match apply_runtime_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        &apply_summary.preinstall_package_checks,
    ) {
        Ok(runtime_checks) => {
            let frankenphp_service_active = install_plan.web_server == "frankenphp"
                && runtime_checks
                    .iter()
                    .any(|check| check.name == "frankenphp-service" && check.status == "pass");
            apply_summary.runtime_checks.extend(runtime_checks);
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
            if let Some(message) = blocking_runtime_failure(&apply_summary.runtime_checks) {
                let restored = runtime_transaction.restore().is_ok();
                if restored {
                    let _ = reload_restored_runtime(probe, &install_plan);
                }
                state.fail_step("runtime", &message, restored);
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
            runtime_transaction.complete()?;
            state.complete_step("runtime");
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
            let restored = runtime_transaction.restore().is_ok();
            if restored {
                let _ = reload_restored_runtime(probe, &install_plan);
            }
            apply_summary.runtime_checks = vec![InstallCheck::fail(
                "runtime-config",
                command_failure_message("Runtime configuration failed", &err),
            )];
            state.fail_step("runtime", err.to_string(), restored);
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

    let vhost_transaction = StepTransaction::begin(
        paths,
        &state.install_id,
        "vhost",
        &vhost_transaction_files(&install_plan),
    )?;
    state.begin_step("vhost");
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
    match apply_vhost_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        &apply_summary.preinstall_package_checks,
    ) {
        Ok(vhost_checks) => {
            if !vhost_checks.is_empty() {
                apply_summary.vhost_checks.extend(vhost_checks);
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
            vhost_transaction.complete()?;
            state.complete_step("vhost");
        }
        Err(err) => {
            let restored = vhost_transaction.restore().is_ok();
            if restored {
                let _ = reload_restored_web_service(probe, &install_plan);
            }
            apply_summary.safety_checks = safety_checks(&install_plan, "vhost-failed");
            apply_summary.vhost_checks = vec![InstallCheck::fail(
                "webserver-vhost",
                command_failure_message("Web server vhost setup failed", &err),
            )];
            state.set_phase(InstallerPhase::VhostFailed);
            state.fail_step("vhost", err.to_string(), restored);
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

    let database_transaction = StepTransaction::begin(
        paths,
        &state.install_id,
        "database",
        &[database_config_path(&install_plan).to_string()],
    )?;
    state.begin_step("database");
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
    match apply_database_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        database_password.as_deref(),
        smtp_password.as_deref(),
    ) {
        Ok(database_checks) => {
            apply_summary.database_checks = database_checks;
            completed_steps.push("database-runtime-configured".to_string());
            completed_steps.push("database-secret-written".to_string());
            completed_steps.push("database-created".to_string());
            completed_steps.push("database-user-created".to_string());
            apply_summary.safety_checks = safety_checks(&install_plan, "database-configured");
            state.set_phase(InstallerPhase::DatabaseConfigured);
            database_transaction.complete()?;
            state.complete_step("database");
            remove_pending_secrets(paths, &mut owned_file_list)?;
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
            let restored = database_transaction.restore().is_ok();
            if restored {
                let _ = restart_restored_database(probe, &install_plan);
            }
            apply_summary.database_checks = vec![InstallCheck::fail(
                "database-config",
                command_failure_message("Database configuration failed", &err),
            )];
            state.fail_step("database", err.to_string(), restored);
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

    let tls_transaction = StepTransaction::begin(
        paths,
        &state.install_id,
        "tls",
        &tls_transaction_files(&install_plan),
    )?;
    state.begin_step("tls");
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
                completed_steps.push(
                    if tls_certificate_was_reused(&apply_summary.certbot_checks) {
                        "certbot-reused"
                    } else {
                        "certbot-issued"
                    }
                    .to_string(),
                );
                completed_steps.push("https-vhost-written".to_string());
                completed_steps.push("certbot-renew-dry-run".to_string());
                state.set_phase(InstallerPhase::TlsEnabled);
            } else if tls_skipped {
                completed_steps.push("tls-skipped".to_string());
            } else {
                completed_steps.push("tls-deferred".to_string());
            }
            tls_transaction.complete()?;
            if tls_passed || tls_skipped {
                state.complete_step("tls");
            } else {
                state.fail_step("tls", "TLS 설정이 보류되었습니다.", false);
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
            let restored = tls_transaction.restore().is_ok();
            if restored {
                let _ = reload_restored_web_service(probe, &install_plan);
            }
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
            state.fail_step("tls", err.to_string(), restored);
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

    let app_transaction = StepTransaction::begin(
        paths,
        &state.install_id,
        "app",
        &app_transaction_files(&install_plan),
    )?;
    state.begin_step("app");
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
            if app_source_ready {
                state.complete_step("app");
            } else {
                state.fail_step("app", "웹앱 소스 준비가 보류되었습니다.", false);
            }
            app_transaction.complete()?;
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
            let restored = app_transaction.restore().is_ok();
            if restored {
                let _ = reload_restored_app_runtime(probe, &install_plan);
            }
            apply_summary.app_checks = vec![InstallCheck::fail(
                "app-source",
                command_failure_message("Application source setup failed", &err),
            )];
            state.fail_step("app", err.to_string(), restored);
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
    write_existing_file(
        paths,
        CONFIG_PATH,
        &config_content_for_phase(&install_plan, &state.phase),
    )?;
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

    Ok(build_install_report(
        paths,
        state_path,
        owned_files_path,
        state,
        install_plan,
        apply_summary,
        owned_files,
        completed_steps,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_install_report(
    paths: &InstallPaths,
    state_path: PathBuf,
    owned_files_path: PathBuf,
    state: InstallerState,
    install_plan: plan::InstallPlan,
    apply_summary: ApplySummary,
    owned_files: OwnedFiles,
    completed_steps: Vec<String>,
) -> InstallReport {
    let app_url = app_access_url(&install_plan, &apply_summary);
    InstallReport {
        install_started_at_unix_ms: apply_summary.install_started_at_unix_ms,
        install_completed_at_unix_ms: if state.phase == InstallerPhase::Completed.as_str() {
            Some(unix_timestamp_millis())
        } else {
            None
        },
        elapsed_ms: unix_timestamp_millis()
            .saturating_sub(apply_summary.install_started_at_unix_ms),
        domain: state.domain,
        deployment_mode: install_plan.deployment_mode,
        app_profile: install_plan.app_profile,
        app_profile_label: install_plan.app_profile_label,
        app_document_root: install_plan.app_document_root,
        web_server: install_plan.web_server,
        php_version: install_plan.php_version,
        php_source: install_plan.php_source,
        database_engine: install_plan.database_engine,
        database_version: install_plan.database_version,
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
        smtp_username: install_plan.smtp_username,
        smtp_password_policy: install_plan.smtp_password_policy,
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
    }
}

fn read_report_value(paths: &InstallPaths) -> Result<serde_json::Value> {
    let path = paths.resolve(REPORT_PATH);
    let payload = fs::read(&path).map_err(|source| Error::FileReadFailed {
        path: REPORT_PATH.to_string(),
        source,
    })?;
    serde_json::from_slice(&payload).map_err(|source| Error::ResumeUnavailable {
        reason: format!("{REPORT_PATH} is not valid JSON: {source}"),
    })
}

pub(super) fn plan_options_from_report(report: &serde_json::Value) -> Result<plan::PlanOptions> {
    let mut options = plan::PlanOptions {
        local_test: required_report_string(report, "deployment_mode")? == "local-test",
        app_profile: required_report_string(report, "app_profile")?,
        web_server: required_report_string(report, "web_server")?,
        php_version: required_report_string(report, "php_version")?,
        php_source: required_report_string(report, "php_source")?,
        database_engine: required_report_string(report, "database")?,
        database_version: optional_report_string(report, "database_version")
            .unwrap_or_else(|| "8.0".to_string()),
        database_name: Some(required_report_string(report, "database_name")?),
        database_user: Some(required_report_string(report, "database_user")?),
        site_user: required_report_string(report, "site_user")?,
        web_root_mode: required_report_string(report, "web_root_mode")?,
        www_mode: required_report_string(report, "www_mode")?,
        redis_mode: required_report_string(report, "redis")?,
        mail_mode: required_report_string(report, "mail_mode")?,
        security_profile: required_report_string(report, "security_profile")?,
        ssh_policy: required_report_string(report, "ssh_policy")?,
        dns_check: report
            .get("dns_check")
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
        ..plan::PlanOptions::default()
    };
    if options.web_root_mode == "custom" {
        options.custom_web_root = Some(required_report_string(report, "web_root")?);
    }
    options.smtp_host = optional_report_string(report, "smtp_host");
    options.smtp_from = optional_report_string(report, "smtp_from");
    options.smtp_username = optional_report_string(report, "smtp_username");
    options.smtp_encryption = optional_report_string(report, "smtp_encryption")
        .unwrap_or_else(|| plan::DEFAULT_SMTP_ENCRYPTION.to_string());
    options.smtp_port = report
        .get("smtp_port")
        .and_then(|value| value.as_u64())
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(plan::DEFAULT_SMTP_PORT);
    Ok(options)
}

fn required_report_string(report: &serde_json::Value, key: &str) -> Result<String> {
    optional_report_string(report, key).ok_or_else(|| Error::ResumeUnavailable {
        reason: format!("{REPORT_PATH} is missing required field `{key}`"),
    })
}

fn optional_report_string(report: &serde_json::Value, key: &str) -> Option<String> {
    report
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn apply_summary_from_report(report: &serde_json::Value) -> ApplySummary {
    ApplySummary {
        install_started_at_unix_ms: report
            .get("install_started_at_unix_ms")
            .and_then(|value| value.as_u64())
            .map(u128::from)
            .unwrap_or_else(unix_timestamp_millis),
        php_apt_source_added: false,
        mysql_apt_source_added: false,
        safety_checks: checks_from_report(report, "safety_checks"),
        preinstall_package_checks: checks_from_report(report, "preinstall_package_checks"),
        package_checks: checks_from_report(report, "package_checks"),
        service_checks: checks_from_report(report, "service_checks"),
        port_checks: checks_from_report(report, "port_checks"),
        network_checks: checks_from_report(report, "network_checks"),
        runtime_checks: checks_from_report(report, "runtime_checks"),
        database_checks: checks_from_report(report, "database_checks"),
        firewall_checks: checks_from_report(report, "firewall_checks"),
        mail_checks: checks_from_report(report, "mail_checks"),
        certbot_checks: checks_from_report(report, "certbot_checks"),
        vhost_checks: checks_from_report(report, "vhost_checks"),
        app_checks: checks_from_report(report, "app_checks"),
    }
}

fn checks_from_report(report: &serde_json::Value, key: &str) -> Vec<InstallCheck> {
    report
        .get(key)
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|check| {
            Some(InstallCheck {
                name: check.get("name")?.as_str()?.to_string(),
                status: check.get("status")?.as_str()?.to_string(),
                message: check.get("message")?.as_str()?.to_string(),
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn resume_pre_tls_steps<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &mut plan::InstallPlan,
    state: &mut InstallerState,
    summary: &mut ApplySummary,
    owned_files: &mut OwnedFiles,
    owned: &mut Vec<String>,
    completed_steps: &mut Vec<String>,
    progress: &ProgressContext<'_>,
) -> Result<()> {
    if !install_step_completed(state, completed_steps, "packages") {
        state.begin_step("packages");
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
        let package_baseline = summary.preinstall_package_checks.clone();
        match apply_package_phase_with_baseline(probe, paths, plan, Some(&package_baseline)) {
            Ok(package_summary) => {
                let php_apt_source_added = package_summary.php_apt_source_added;
                let mysql_apt_source_added = package_summary.mysql_apt_source_added;
                summary.php_apt_source_added = php_apt_source_added;
                summary.mysql_apt_source_added = mysql_apt_source_added;
                summary.preinstall_package_checks = package_summary.preinstall_package_checks;
                summary.package_checks = package_summary.package_checks;
                summary.service_checks = package_summary.service_checks;
                summary.port_checks = package_summary.port_checks;
                summary.network_checks = package_summary.network_checks;
                summary.mail_checks = package_summary.mail_checks;
                summary.certbot_checks = package_summary.certbot_checks;
                for step in [
                    "apt-updated",
                    "package-candidates-checked",
                    "packages-installed",
                    "services-enabled",
                    "package-verification-passed",
                    "service-verification-passed",
                    "port-verification-passed",
                    "network-readiness-checked",
                    "mail-readiness-checked",
                    "certbot-readiness-checked",
                ] {
                    mark_step(completed_steps, step);
                }
                if php_apt_source_added {
                    mark_step(completed_steps, "php-apt-source-added");
                    mark_step(completed_steps, "apt-updated-after-php-source");
                }
                if mysql_apt_source_added {
                    mark_step(completed_steps, "mysql-apt-source-added");
                    mark_step(completed_steps, "apt-updated-after-mysql-source");
                }
                state.set_phase(InstallerPhase::PackagesInstalled);
                state.complete_step("packages");
                state.completed_steps = completed_steps.clone();
            }
            Err(failure) => {
                let failure = *failure;
                summary.preinstall_package_checks = failure.summary.preinstall_package_checks;
                summary.package_checks = failure.summary.package_checks;
                summary.service_checks = failure.summary.service_checks;
                for step in failure.completed_steps {
                    mark_step(completed_steps, &step);
                }
                state.set_phase(InstallerPhase::PackageFailed);
                state.fail_step("packages", failure.error.to_string(), false);
                state.completed_steps = completed_steps.clone();
                persist_resume_step(
                    progress,
                    owned_files,
                    owned,
                    state,
                    plan,
                    summary,
                    Some(&failure.error.to_string()),
                )?;
                return Err(failure.error);
            }
        }
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
    }

    if !install_step_completed(state, completed_steps, "site") {
        let transaction =
            StepTransaction::begin(paths, &state.install_id, "site", &[ready_probe_path(plan)])?;
        state.begin_step("site");
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
        let site_password = read_site_password(paths)?;
        match apply_site_phase(probe, paths, plan, owned, site_password.as_deref()) {
            Ok(checks) => {
                summary.vhost_checks = checks;
                mark_step(completed_steps, "site-user-verified");
                mark_step(completed_steps, "web-root-created");
                transaction.complete()?;
                state.complete_step("site");
                state.completed_steps = completed_steps.clone();
            }
            Err(error) => {
                let restored = transaction.restore().is_ok();
                summary.vhost_checks = vec![InstallCheck::fail(
                    "site-provision",
                    format!("Site account and web root setup failed: {error}"),
                )];
                state.set_phase(InstallerPhase::VhostFailed);
                state.fail_step("site", error.to_string(), restored);
                persist_resume_step(
                    progress,
                    owned_files,
                    owned,
                    state,
                    plan,
                    summary,
                    Some(&error.to_string()),
                )?;
                return Err(error);
            }
        }
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
    }

    if !install_step_completed(state, completed_steps, "runtime") {
        let transaction = StepTransaction::begin(
            paths,
            &state.install_id,
            "runtime",
            &runtime_transaction_files(plan),
        )?;
        state.begin_step("runtime");
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
        match apply_runtime_phase(
            probe,
            paths,
            plan,
            owned,
            &summary.preinstall_package_checks,
        ) {
            Ok(checks) => {
                let frankenphp_service_active = plan.web_server == "frankenphp"
                    && checks
                        .iter()
                        .any(|check| check.name == "frankenphp-service" && check.status == "pass");
                summary
                    .runtime_checks
                    .retain(|check| matches!(check.name.as_str(), "swapfile" | "swap-sysctl"));
                summary.runtime_checks.extend(checks);
                if frankenphp_service_active
                    && !summary
                        .service_checks
                        .iter()
                        .any(|check| check.name == FRANKENPHP_SERVICE_NAME)
                {
                    summary.service_checks.push(InstallCheck::pass(
                        FRANKENPHP_SERVICE_NAME,
                        format!("FrankenPHP app server is active on {FRANKENPHP_LISTEN}."),
                    ));
                }
                if let Some(message) = blocking_runtime_failure(&summary.runtime_checks) {
                    let restored = transaction.restore().is_ok();
                    if restored {
                        let _ = reload_restored_runtime(probe, plan);
                    }
                    state.fail_step("runtime", &message, restored);
                    persist_resume_step(
                        progress,
                        owned_files,
                        owned,
                        state,
                        plan,
                        summary,
                        Some(&message),
                    )?;
                    return Err(Error::InstallVerificationFailed { checks: message });
                }
                for step in [
                    "php-runtime-config-written",
                    "php-runtime-diagnostics-passed",
                ] {
                    mark_step(completed_steps, step);
                }
                mark_step(
                    completed_steps,
                    if plan.web_server == "frankenphp" {
                        "frankenphp-runtime-config-written"
                    } else {
                        "php-fpm-config-written"
                    },
                );
                state.set_phase(InstallerPhase::RuntimeConfigured);
                transaction.complete()?;
                state.complete_step("runtime");
                state.completed_steps = completed_steps.clone();
            }
            Err(error) => {
                let restored = transaction.restore().is_ok();
                if restored {
                    let _ = reload_restored_runtime(probe, plan);
                }
                summary.runtime_checks = vec![InstallCheck::fail(
                    "runtime-config",
                    command_failure_message("Runtime configuration failed", &error),
                )];
                state.fail_step("runtime", error.to_string(), restored);
                persist_resume_step(
                    progress,
                    owned_files,
                    owned,
                    state,
                    plan,
                    summary,
                    Some(&error.to_string()),
                )?;
                return Err(error);
            }
        }
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
    }

    if !install_step_completed(state, completed_steps, "vhost") {
        let transaction = StepTransaction::begin(
            paths,
            &state.install_id,
            "vhost",
            &vhost_transaction_files(plan),
        )?;
        state.begin_step("vhost");
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
        match apply_vhost_phase(
            probe,
            paths,
            plan,
            owned,
            &summary.preinstall_package_checks,
        ) {
            Ok(checks) => {
                summary.vhost_checks.retain(|check| {
                    matches!(
                        check.name.as_str(),
                        "site-user"
                            | "site-user-password"
                            | "ssh-password-auth"
                            | "web-root"
                            | "php-ready-probe"
                            | "web-root-permissions"
                    )
                });
                summary.vhost_checks.extend(checks);
                for step in ["vhost-written", "vhost-enabled", "http-smoke-passed"] {
                    mark_step(completed_steps, step);
                }
                mark_step(
                    completed_steps,
                    &format!("{}-config-tested", web_service_name(plan)),
                );
                mark_step(
                    completed_steps,
                    &format!("{}-reloaded", web_service_name(plan)),
                );
                state.set_phase(InstallerPhase::VhostEnabled);
                transaction.complete()?;
                state.complete_step("vhost");
                state.completed_steps = completed_steps.clone();
            }
            Err(error) => {
                let restored = transaction.restore().is_ok();
                if restored {
                    let _ = reload_restored_web_service(probe, plan);
                }
                summary.vhost_checks = vec![InstallCheck::fail(
                    "webserver-vhost",
                    command_failure_message("Web server vhost setup failed", &error),
                )];
                state.set_phase(InstallerPhase::VhostFailed);
                state.fail_step("vhost", error.to_string(), restored);
                persist_resume_step(
                    progress,
                    owned_files,
                    owned,
                    state,
                    plan,
                    summary,
                    Some(&error.to_string()),
                )?;
                return Err(error);
            }
        }
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
    }

    if !install_step_completed(state, completed_steps, "database") {
        let transaction = StepTransaction::begin(
            paths,
            &state.install_id,
            "database",
            &[database_config_path(plan).to_string()],
        )?;
        state.begin_step("database");
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
        let database_password = read_database_password(paths)?;
        let smtp_password = read_smtp_password(paths)?;
        match apply_database_phase(
            probe,
            paths,
            plan,
            owned,
            database_password.as_deref(),
            smtp_password.as_deref(),
        ) {
            Ok(checks) => {
                summary.database_checks = checks;
                for step in [
                    "database-runtime-configured",
                    "database-secret-written",
                    "database-created",
                    "database-user-created",
                ] {
                    mark_step(completed_steps, step);
                }
                state.set_phase(InstallerPhase::DatabaseConfigured);
                transaction.complete()?;
                state.complete_step("database");
                remove_pending_secrets(paths, owned)?;
                state.completed_steps = completed_steps.clone();
            }
            Err(error) => {
                let restored = transaction.restore().is_ok();
                if restored {
                    let _ = restart_restored_database(probe, plan);
                }
                summary.database_checks = vec![InstallCheck::fail(
                    "database-config",
                    command_failure_message("Database configuration failed", &error),
                )];
                state.fail_step("database", error.to_string(), restored);
                persist_resume_step(
                    progress,
                    owned_files,
                    owned,
                    state,
                    plan,
                    summary,
                    Some(&error.to_string()),
                )?;
                return Err(error);
            }
        }
        persist_resume_step(progress, owned_files, owned, state, plan, summary, None)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn persist_resume_step(
    progress: &ProgressContext<'_>,
    owned_files: &mut OwnedFiles,
    owned: &[String],
    state: &mut InstallerState,
    plan: &plan::InstallPlan,
    summary: &ApplySummary,
    problem: Option<&str>,
) -> Result<()> {
    state.completed_steps.sort();
    state.completed_steps.dedup();
    persist_progress(progress, owned_files, owned, state, plan, summary, problem)
}

fn install_step_completed(state: &InstallerState, completed_steps: &[String], step: &str) -> bool {
    if state.step_is_completed(step) {
        return true;
    }
    let marker = match step {
        "packages" => "package-verification-passed",
        "site" => "web-root-created",
        "vhost" => "vhost-enabled",
        "runtime" => "php-runtime-diagnostics-passed",
        "database" => "database-user-created",
        "tls" => "certbot-renew-dry-run",
        "app" => "app-source-prepared",
        _ => return false,
    };
    completed_steps.iter().any(|completed| completed == marker)
}

fn tls_is_ready(checks: &[InstallCheck]) -> bool {
    checks.iter().any(|check| {
        check.name == "tls-certificate" && check.status == "pass"
            || check.name == "tls" && check.status == "skipped"
    })
}

pub(super) fn tls_certificate_was_reused(checks: &[InstallCheck]) -> bool {
    checks
        .iter()
        .any(|check| check.name == "certbot-renewal-webroot" && check.status == "pass")
}

pub(super) fn app_is_ready(completed_steps: &[String], checks: &[InstallCheck]) -> bool {
    completed_steps
        .iter()
        .any(|step| step == "app-source-prepared")
        && checks
            .iter()
            .any(|check| check.name == "app-source" && check.status == "pass")
        && !checks.iter().any(|check| check.status == "fail")
}

fn mark_step(completed_steps: &mut Vec<String>, step: &str) {
    if !completed_steps.iter().any(|existing| existing == step) {
        completed_steps.push(step.to_string());
    }
}

fn vhost_transaction_files(plan: &plan::InstallPlan) -> Vec<String> {
    let mut files = if plan.web_server == "apache" {
        vec![
            g7_system::apache::G7_SITE_AVAILABLE.to_string(),
            g7_system::apache::G7_SITE_ENABLED.to_string(),
        ]
    } else {
        vec![
            g7_system::nginx::G7_SITE_AVAILABLE.to_string(),
            g7_system::nginx::G7_SITE_ENABLED.to_string(),
            g7_system::nginx::G7_DEFAULT_DENY_AVAILABLE.to_string(),
            g7_system::nginx::G7_DEFAULT_DENY_ENABLED.to_string(),
            "/etc/nginx/sites-enabled/default".to_string(),
        ]
    };
    files.push(ready_probe_path(plan));
    files
}

fn runtime_transaction_files(plan: &plan::InstallPlan) -> Vec<String> {
    let mut files = vec![php_ini_override_path(plan)];
    if plan.web_server == "apache" {
        files.extend([
            "/etc/apache2/conf-available/g7-runtime.conf".to_string(),
            "/etc/apache2/conf-enabled/g7-runtime.conf".to_string(),
            g7_system::apache::G7_SITE_AVAILABLE.to_string(),
        ]);
    } else {
        files.extend([
            NGINX_MAIN_CONFIG_PATH.to_string(),
            "/etc/nginx/conf.d/g7-runtime-tuning.conf".to_string(),
            g7_system::nginx::G7_SITE_AVAILABLE.to_string(),
        ]);
    }
    if plan.web_server == "frankenphp" {
        files.extend([
            FRANKENPHP_BIN_PATH.to_string(),
            FRANKENPHP_SERVICE_PATH.to_string(),
            format!("/etc/systemd/system/multi-user.target.wants/{FRANKENPHP_SERVICE_NAME}"),
        ]);
    } else {
        files.push(php_pool_path(plan));
        files.push(format!("/etc/php/{}/fpm/pool.d/www.conf", plan.php_version));
    }
    files
}

fn tls_transaction_files(plan: &plan::InstallPlan) -> Vec<String> {
    vec![
        if plan.web_server == "apache" {
            g7_system::apache::G7_SITE_AVAILABLE.to_string()
        } else {
            g7_system::nginx::G7_SITE_AVAILABLE.to_string()
        },
        format!("/etc/letsencrypt/renewal/{}.conf", plan.domain),
    ]
}

fn app_transaction_files(plan: &plan::InstallPlan) -> Vec<String> {
    let mut files = Vec::new();
    if matches!(
        plan.app_profile.as_str(),
        "gnuboard7" | "gnuboard7-octane" | "laravel" | "laravel-octane"
    ) {
        files.push(format!("{}/.env", plan.web_root));
    }
    for unit in app_runtime_unit_names(plan) {
        files.push(systemd_unit_path(unit));
        let target = if unit.ends_with(".timer") {
            "timers.target.wants"
        } else {
            "multi-user.target.wants"
        };
        files.push(format!("/etc/systemd/system/{target}/{unit}"));
    }
    if plan.app_profile == "laravel-octane" {
        files.push(FRANKENPHP_SERVICE_PATH.to_string());
    }
    files
}

fn reload_restored_app_runtime<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Result<()> {
    let units = app_runtime_unit_names(plan);
    if units.is_empty() && plan.app_profile != "laravel-octane" {
        return Ok(());
    }
    for unit in units {
        let _ = probe.disable_service_now(unit);
    }
    let output = probe.systemd_daemon_reload().map_err(|error| {
        command_error(
            "app-restore-daemon-reload",
            "systemctl daemon-reload",
            error,
        )
    })?;
    require_success(
        "app-restore-daemon-reload",
        "systemctl daemon-reload",
        output,
    )?;
    if plan.app_profile == "laravel-octane" {
        let output = probe
            .restart_service(FRANKENPHP_SERVICE_NAME)
            .map_err(|error| {
                command_error(
                    "app-restore-frankenphp",
                    format!("systemctl restart {FRANKENPHP_SERVICE_NAME}"),
                    error,
                )
            })?;
        require_success(
            "app-restore-frankenphp",
            format!("systemctl restart {FRANKENPHP_SERVICE_NAME}"),
            output,
        )?;
    }
    Ok(())
}

fn reload_restored_runtime<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Result<()> {
    if plan.web_server == "frankenphp" {
        let _ = probe.disable_service_now(FRANKENPHP_SERVICE_NAME);
        let output = probe.systemd_daemon_reload().map_err(|error| {
            command_error(
                "frankenphp-restore-daemon-reload",
                "systemctl daemon-reload",
                error,
            )
        })?;
        require_success(
            "frankenphp-restore-daemon-reload",
            "systemctl daemon-reload",
            output,
        )?;
    } else {
        let service = format!("php{}-fpm", plan.php_version);
        let output = probe.reload_service(&service).map_err(|error| {
            command_error(
                "php-restore-reload",
                format!("systemctl reload {service}"),
                error,
            )
        })?;
        require_success(
            "php-restore-reload",
            format!("systemctl reload {service}"),
            output,
        )?;
    }
    reload_restored_web_service(probe, plan)
}

fn restart_restored_database<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Result<()> {
    let service = database_service_name(plan);
    let command = format!("systemctl restart {service}");
    let output = probe
        .restart_service(service)
        .map_err(|error| command_error("database-restore-restart", &command, error))?;
    require_success("database-restore-restart", command, output)
}

fn remove_pending_secrets(paths: &InstallPaths, owned: &mut Vec<String>) -> Result<()> {
    match fs::remove_file(paths.resolve(PENDING_SECRETS_PATH)) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(Error::FileWriteFailed {
                path: PENDING_SECRETS_PATH.to_string(),
                source,
            });
        }
    }
    owned.retain(|path| path != PENDING_SECRETS_PATH);
    Ok(())
}

fn reload_restored_web_service<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Result<()> {
    if plan.web_server == "apache" {
        let output = probe.apache_config_test().map_err(|error| {
            command_error("apache-restore-configtest", "apache2ctl configtest", error)
        })?;
        require_success("apache-restore-configtest", "apache2ctl configtest", output)?;
        let output = probe
            .reload_service(g7_system::apache::SERVICE_NAME)
            .map_err(|error| {
                command_error("apache-restore-reload", "systemctl reload apache2", error)
            })?;
        require_success("apache-restore-reload", "systemctl reload apache2", output)
    } else {
        let output = probe
            .nginx_config_test()
            .map_err(|error| command_error("nginx-restore-configtest", "nginx -t", error))?;
        require_success("nginx-restore-configtest", "nginx -t", output)?;
        let output = probe
            .reload_service(g7_system::nginx::SERVICE_NAME)
            .map_err(|error| {
                command_error("nginx-restore-reload", "systemctl reload nginx", error)
            })?;
        require_success("nginx-restore-reload", "systemctl reload nginx", output)
    }
}

fn write_or_update_tracked_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    if paths.resolve(path).exists() {
        if !owned.iter().any(|owned_path| owned_path == path) {
            return Err(Error::ResumeUnavailable {
                reason: format!("refusing to overwrite unowned resume file `{path}`"),
            });
        }
        write_existing_file(paths, path, content)
    } else {
        write_new_file(paths, path, content, owned)
    }
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
