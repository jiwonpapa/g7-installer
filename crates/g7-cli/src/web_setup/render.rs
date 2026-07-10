use super::*;

pub(super) fn doctor_to_api(report: doctor::DoctorReport) -> DoctorApiReport {
    let resources = DoctorApiResources {
        total_memory_mib: report.resources.total_memory_kib.map(|value| value / 1024),
        available_memory_mib: report
            .resources
            .available_memory_kib
            .map(|value| value / 1024),
        swap_total_mib: report.resources.swap_total_kib.map(|value| value / 1024),
        root_available_mib: report
            .resources
            .root_available_kib
            .map(|value| value / 1024),
        root_inode_free_percent: report
            .resources
            .root_available_inodes
            .zip(report.resources.root_total_inodes)
            .map(|(available, total)| available.saturating_mul(100) / total.max(1)),
    };
    DoctorApiReport {
        install_allowed: report.install_allowed,
        resources,
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

pub(super) fn doctor_status_label(status: DoctorCheckStatus) -> &'static str {
    match status {
        DoctorCheckStatus::Pass => "pass",
        DoctorCheckStatus::Warn => "warn",
        DoctorCheckStatus::Fail => "fail",
        DoctorCheckStatus::Pending => "pending",
    }
}

#[cfg(test)]
pub(super) fn failed_doctor_details(report: doctor::DoctorReport) -> Vec<String> {
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

pub(super) fn failed_report_details() -> Vec<String> {
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

pub(super) fn plan_to_api(
    install_plan: plan::InstallPlan,
    database_version: String,
) -> PlanApiReport {
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

pub(super) fn install_to_api(
    report: install::InstallReport,
    database_version: String,
) -> InstallApiReport {
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
        smtp_username: report.smtp_username,
        smtp_password_policy: report.smtp_password_policy,
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

pub(super) fn normalize_database_version(value: &str) -> String {
    let _ = value;
    "apt-default".to_string()
}

pub(super) fn install_checks_to_api(checks: Vec<install::InstallCheck>) -> Vec<InstallApiCheck> {
    checks
        .into_iter()
        .map(|check| InstallApiCheck {
            name: check.name,
            status: check.status,
            message: check.message,
        })
        .collect()
}

pub(super) fn rollback_to_api(report: rollback::RollbackReport) -> RollbackApiReport {
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

pub(super) fn reset_actions_to_api(actions: Vec<reset::ResetAction>) -> Vec<ResetApiAction> {
    actions
        .into_iter()
        .map(|action| ResetApiAction {
            name: action.name,
            status: action.status,
            message: action.message,
        })
        .collect()
}

pub(super) fn rollback_actions_to_api(
    actions: Vec<rollback::RollbackAction>,
) -> Vec<RollbackApiAction> {
    actions
        .into_iter()
        .map(|action| RollbackApiAction {
            name: action.name,
            status: action.status,
            message: action.message,
        })
        .collect()
}
