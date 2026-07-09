use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub domain: String,
    pub deployment_mode: String,
    pub app_profile: String,
    pub app_profile_label: &'static str,
    pub app_summary: &'static str,
    pub app_document_root: String,
    pub web_server: String,
    pub php_version: String,
    pub php_source: String,
    pub database_engine: String,
    pub site_user: String,
    pub web_root_mode: String,
    pub web_root: String,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_from: Option<String>,
    pub smtp_encryption: Option<String>,
    pub security_profile: String,
    pub ssh_policy: String,
    pub database_name: String,
    pub database_user: String,
    pub database_password_policy: &'static str,
    pub rollback_enabled: bool,
    pub preserve_config: bool,
    pub dns_check_required: bool,
    pub mode: &'static str,
    pub fresh_server_only: bool,
    pub changes_made: bool,
    pub preflight_gates: Vec<PlanGate>,
    pub packages: Vec<PlanPackage>,
    pub files: Vec<PlanFile>,
    pub services: Vec<PlanService>,
    pub ports: Vec<PlanPort>,
    pub security_checks: Vec<PlanSecurityCheck>,
    pub app_requirements: Vec<AppRequirement>,
    pub app_followup_steps: Vec<AppFollowupStep>,
    pub provisioning: Vec<ProvisioningSection>,
    pub stop_conditions: Vec<PlanStopCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanGate {
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPackage {
    pub name: String,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFile {
    pub path: String,
    pub action: &'static str,
}

impl PlanFile {
    pub(super) fn new(path: impl Into<String>, action: &'static str) -> Self {
        Self {
            path: path.into(),
            action,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanService {
    pub name: String,
    pub action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPort {
    pub port: u16,
    pub protocol: &'static str,
    pub purpose: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSecurityCheck {
    pub name: &'static str,
    pub level: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStopCondition {
    pub reason: String,
}

impl PlanStopCondition {
    pub(super) fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisioningSection {
    pub name: &'static str,
    pub title: &'static str,
    pub summary: String,
    pub settings: Vec<ProvisioningSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisioningSetting {
    pub key: &'static str,
    pub value: String,
}

impl ProvisioningSetting {
    pub(super) fn new(key: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            value: value.into(),
        }
    }
}
