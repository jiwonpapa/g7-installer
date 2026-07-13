//! Read-only installer and managed-service status reporting.

use std::fs;
use std::path::Path;

use g7_state::state::{InstallerPhase, STATE_PATH, read_state_file};
use g7_system::SystemProbe;
use g7_system::service::ServiceActivity;

use crate::installer_paths::REPORT_PATH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallerStatus {
    pub installed: bool,
    pub domain: Option<String>,
    pub phase: Option<String>,
    pub components: Vec<ComponentStatus>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentStatus {
    pub name: String,
    pub state: String,
}

pub fn read() -> InstallerStatus {
    let state = read_state_file(Path::new(STATE_PATH)).ok();
    let report = fs::read(REPORT_PATH)
        .ok()
        .and_then(|payload| serde_json::from_slice::<serde_json::Value>(&payload).ok());
    let phase = state
        .as_ref()
        .map(|state| state.phase.clone())
        .or_else(|| report_string(report.as_ref(), "phase"));
    let domain = state
        .as_ref()
        .map(|state| state.domain.clone())
        .or_else(|| report_string(report.as_ref(), "domain"));
    let probe = SystemProbe::real();

    let mut components = vec![ComponentStatus {
        name: "installer".to_string(),
        state: phase.clone().unwrap_or_else(|| "not-started".to_string()),
    }];
    if let Some(report) = report.as_ref() {
        let web_server = report
            .get("web_server")
            .and_then(|value| value.as_str())
            .unwrap_or("nginx");
        let edge_service = if web_server == "apache" {
            "apache2"
        } else {
            "nginx"
        };
        components.push(service_component(&probe, edge_service));
        if web_server == "frankenphp" {
            components.push(service_component(&probe, "g7-frankenphp"));
        }

        let database_service = if report
            .get("database_engine")
            .and_then(|value| value.as_str())
            == Some("mariadb")
        {
            "mariadb"
        } else {
            "mysql"
        };
        components.push(service_component(&probe, database_service));
        components.push(check_component(report, "tls", "certbot_checks"));
        components.push(check_component(report, "app", "app_checks"));
        if report.get("app_profile").and_then(|value| value.as_str()) == Some("gnuboard7") {
            components.push(check_component(report, "g7-finalize", "finalize_checks"));
            if let Some(services) = report
                .get("g7_runtime_services")
                .and_then(|value| value.as_array())
            {
                for service in services.iter().filter_map(|value| value.as_str()) {
                    components.push(service_component(&probe, service));
                }
            }
        }
    }

    let problems = report.as_ref().map(report_problems).unwrap_or_default();
    InstallerStatus {
        installed: phase.as_deref() == Some(InstallerPhase::Completed.as_str()),
        domain,
        phase,
        components,
        problems,
    }
}

fn service_component(
    probe: &SystemProbe<g7_system::command::RealCommandRunner>,
    service: &str,
) -> ComponentStatus {
    let state = match probe.service_activity(service) {
        Ok(ServiceActivity::Active) => "active".to_string(),
        Ok(ServiceActivity::Inactive) => "inactive".to_string(),
        Ok(ServiceActivity::NotFound) => "not-found".to_string(),
        Ok(ServiceActivity::Unknown) => "unknown".to_string(),
        Err(error) => format!("unknown ({error})"),
    };
    ComponentStatus {
        name: service.to_string(),
        state,
    }
}

fn check_component(report: &serde_json::Value, name: &str, section: &str) -> ComponentStatus {
    let checks = report
        .get(section)
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let state = checks
        .iter()
        .find_map(|check| {
            (check.get("status").and_then(|value| value.as_str()) == Some("fail"))
                .then_some("failed")
        })
        .or_else(|| {
            checks.iter().find_map(|check| {
                let status = check.get("status").and_then(|value| value.as_str())?;
                matches!(status, "warn" | "deferred" | "manual").then_some(status)
            })
        })
        .or_else(|| {
            checks.iter().find_map(|check| {
                (check.get("status").and_then(|value| value.as_str()) == Some("pass"))
                    .then_some("ready")
            })
        })
        .unwrap_or("unknown")
        .to_string();
    ComponentStatus {
        name: name.to_string(),
        state,
    }
}

fn report_problems(report: &serde_json::Value) -> Vec<String> {
    const SECTIONS: &[&str] = &[
        "safety_checks",
        "package_checks",
        "service_checks",
        "port_checks",
        "network_checks",
        "runtime_checks",
        "database_checks",
        "firewall_checks",
        "mail_checks",
        "certbot_checks",
        "vhost_checks",
        "app_checks",
        "finalize_checks",
    ];
    let mut problems = Vec::new();
    if let Some(problem) = report.get("problem").and_then(|value| value.as_str()) {
        if !problem.trim().is_empty() {
            problems.push(problem.to_string());
        }
    }
    for section in SECTIONS {
        let Some(checks) = report.get(section).and_then(|value| value.as_array()) else {
            continue;
        };
        for check in checks {
            if check.get("status").and_then(|value| value.as_str()) != Some("fail") {
                continue;
            }
            let name = check
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let message = check
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("failed");
            problems.push(format!("{section}.{name}: {message}"));
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

fn report_string(report: Option<&serde_json::Value>, key: &str) -> Option<String> {
    report?
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{check_component, report_problems};

    #[test]
    fn failed_report_checks_are_visible_in_status() {
        let report = serde_json::json!({
            "certbot_checks": [
                {"name": "tls-config", "status": "fail", "message": "certbot failed"}
            ],
            "app_checks": [
                {"name": "app-source", "status": "manual", "message": "finish in browser"}
            ],
            "finalize_checks": [
                {"name": "vite-manifest", "status": "fail", "message": "asset missing"}
            ]
        });

        assert_eq!(
            check_component(&report, "tls", "certbot_checks").state,
            "failed"
        );
        assert_eq!(
            check_component(&report, "app", "app_checks").state,
            "manual"
        );
        assert_eq!(
            report_problems(&report),
            vec![
                "certbot_checks.tls-config: certbot failed",
                "finalize_checks.vite-manifest: asset missing"
            ]
        );
    }
}
