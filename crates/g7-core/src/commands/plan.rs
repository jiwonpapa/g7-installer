//! Canonical install policy for G7 Installer.
//!
//! This module is the source of truth for what the installer intends to manage.
//! CLI prompts, TUI fields, generated config, README examples, and release notes
//! must follow this plan instead of inventing separate defaults.
//!
//! Scope rule: plan may describe future server changes, but `install` must only
//! execute the subset that is implemented and tracked in state/owned-files.

use crate::app_profile::{AppFollowupStep, AppRequirement, resolve_app_profile};
use crate::{Error, Result};

pub use crate::app_profile::DEFAULT_APP_PROFILE;
use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::STATE_PATH;
use g7_system::php::{
    DEFAULT_FPM_VERSION, PHP_SOURCE_AUTO, PHP_SOURCE_ONDREJ, PHP_SOURCE_UBUNTU,
    SUPPORTED_FPM_VERSIONS, SUPPORTED_PHP_SOURCES,
};

pub const DEFAULT_PHP_VERSION: &str = DEFAULT_FPM_VERSION;
pub const DEFAULT_PHP_SOURCE: &str = PHP_SOURCE_AUTO;
pub const DEFAULT_WEB_SERVER: &str = "nginx";
pub const DEFAULT_DATABASE_ENGINE: &str = "mysql";
pub const DEFAULT_SITE_USER: &str = "g7";
pub const DEFAULT_WEB_ROOT_MODE: &str = "public-html";
pub const DEFAULT_WWW_MODE: &str = "redirect-to-root";
pub const DEFAULT_REDIS_MODE: &str = "enable";
pub const DEFAULT_MAIL_MODE: &str = "none";
pub const DEFAULT_SMTP_PORT: u16 = 587;
pub const DEFAULT_SMTP_ENCRYPTION: &str = "starttls";
pub const DEFAULT_SECURITY_PROFILE: &str = "standard";
pub const DEFAULT_SSH_POLICY: &str = "audit-only";

const SUPPORTED_WEB_SERVERS: [&str; 2] = ["nginx", "apache"];
const SUPPORTED_DATABASE_ENGINES: [&str; 2] = ["mysql", "mariadb"];
const SUPPORTED_WEB_ROOT_MODES: [&str; 4] = ["public-html", "www", "system", "custom"];
const SUPPORTED_WWW_MODES: [&str; 4] = ["redirect-to-root", "redirect-to-www", "include", "none"];
const SUPPORTED_REDIS_MODES: [&str; 2] = ["enable", "disable"];
const SUPPORTED_MAIL_MODES: [&str; 3] = ["none", "smtp-relay", "local-postfix"];
const SUPPORTED_SMTP_ENCRYPTION: [&str; 3] = ["none", "starttls", "tls"];
const SUPPORTED_SECURITY_PROFILES: [&str; 3] = ["audit-only", "standard", "hardened"];
const SUPPORTED_SSH_POLICIES: [&str; 2] = ["audit-only", "harden"];

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
    fn new(path: impl Into<String>, action: &'static str) -> Self {
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
    fn new(reason: impl Into<String>) -> Self {
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
    fn new(key: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemorySizingPreset {
    key: &'static str,
    label: &'static str,
    ram: &'static str,
    swap: &'static str,
    os_reserve: &'static str,
    php_max_children: &'static str,
    php_processes: &'static str,
    php_cpu_guard: &'static str,
    php_memory_limit: &'static str,
    php_upload_limit: &'static str,
    opcache_memory: &'static str,
    db_buffer_pool: &'static str,
    db_max_connections: &'static str,
    db_tmp_table_size: &'static str,
    redis_maxmemory: &'static str,
    nginx_worker_processes: &'static str,
    nginx_worker_connections: &'static str,
    nginx_worker_rlimit_nofile: &'static str,
    nginx_keepalive_timeout: &'static str,
    nginx_fastcgi_buffers: &'static str,
    apache_mpm: &'static str,
    apache_start_servers: &'static str,
    apache_server_limit: &'static str,
    apache_threads_per_child: &'static str,
    apache_max_request_workers: &'static str,
    apache_spare_threads: &'static str,
    apache_max_connections_per_child: &'static str,
    note: &'static str,
}

const MEMORY_SIZING_PRESETS: [MemorySizingPreset; 7] = [
    MemorySizingPreset {
        key: "tier_1gb",
        label: "1GB",
        ram: "0.75-1.5GB",
        swap: "2GB",
        os_reserve: "384M",
        php_max_children: "4",
        php_processes: "start=1,min_spare=1,max_spare=2",
        php_cpu_guard: "min(memory_budget, vCPU*4)",
        php_memory_limit: "192M",
        php_upload_limit: "32M",
        opcache_memory: "64M",
        db_buffer_pool: "128M",
        db_max_connections: "30",
        db_tmp_table_size: "32M",
        redis_maxmemory: "64M",
        nginx_worker_processes: "1",
        nginx_worker_connections: "512",
        nginx_worker_rlimit_nofile: "2048",
        nginx_keepalive_timeout: "15s",
        nginx_fastcgi_buffers: "8 16k",
        apache_mpm: "event",
        apache_start_servers: "1",
        apache_server_limit: "2",
        apache_threads_per_child: "25",
        apache_max_request_workers: "50",
        apache_spare_threads: "min=10,max=25",
        apache_max_connections_per_child: "1000",
        note: "single small site, Redis optional under pressure",
    },
    MemorySizingPreset {
        key: "tier_2gb",
        label: "2GB",
        ram: "1.5-3GB",
        swap: "2GB",
        os_reserve: "512M",
        php_max_children: "8",
        php_processes: "start=2,min_spare=2,max_spare=4",
        php_cpu_guard: "min(memory_budget, vCPU*4)",
        php_memory_limit: "256M",
        php_upload_limit: "64M",
        opcache_memory: "128M",
        db_buffer_pool: "384M",
        db_max_connections: "60",
        db_tmp_table_size: "64M",
        redis_maxmemory: "128M",
        nginx_worker_processes: "min(vCPU,2)",
        nginx_worker_connections: "1024",
        nginx_worker_rlimit_nofile: "4096",
        nginx_keepalive_timeout: "20s",
        nginx_fastcgi_buffers: "16 16k",
        apache_mpm: "event",
        apache_start_servers: "2",
        apache_server_limit: "3",
        apache_threads_per_child: "25",
        apache_max_request_workers: "75",
        apache_spare_threads: "min=25,max=50",
        apache_max_connections_per_child: "2000",
        note: "default low-cost VPS target",
    },
    MemorySizingPreset {
        key: "tier_4gb",
        label: "4GB",
        ram: "3-6GB",
        swap: "2GB",
        os_reserve: "768M",
        php_max_children: "16",
        php_processes: "start=4,min_spare=4,max_spare=8",
        php_cpu_guard: "min(memory_budget, vCPU*6)",
        php_memory_limit: "256M",
        php_upload_limit: "128M",
        opcache_memory: "192M",
        db_buffer_pool: "1G",
        db_max_connections: "100",
        db_tmp_table_size: "128M",
        redis_maxmemory: "256M",
        nginx_worker_processes: "min(vCPU,2)",
        nginx_worker_connections: "2048",
        nginx_worker_rlimit_nofile: "8192",
        nginx_keepalive_timeout: "20s",
        nginx_fastcgi_buffers: "16 16k",
        apache_mpm: "event",
        apache_start_servers: "2",
        apache_server_limit: "4",
        apache_threads_per_child: "25",
        apache_max_request_workers: "100",
        apache_spare_threads: "min=25,max=75",
        apache_max_connections_per_child: "3000",
        note: "small production site baseline",
    },
    MemorySizingPreset {
        key: "tier_8gb",
        label: "8GB",
        ram: "6-12GB",
        swap: "2GB",
        os_reserve: "1G",
        php_max_children: "32",
        php_processes: "start=6,min_spare=6,max_spare=12",
        php_cpu_guard: "min(memory_budget, vCPU*8)",
        php_memory_limit: "256M",
        php_upload_limit: "128M",
        opcache_memory: "256M",
        db_buffer_pool: "2G",
        db_max_connections: "150",
        db_tmp_table_size: "256M",
        redis_maxmemory: "512M",
        nginx_worker_processes: "min(vCPU,4)",
        nginx_worker_connections: "4096",
        nginx_worker_rlimit_nofile: "16384",
        nginx_keepalive_timeout: "30s",
        nginx_fastcgi_buffers: "32 16k",
        apache_mpm: "event",
        apache_start_servers: "3",
        apache_server_limit: "8",
        apache_threads_per_child: "25",
        apache_max_request_workers: "200",
        apache_spare_threads: "min=50,max=100",
        apache_max_connections_per_child: "5000",
        note: "busy single site or light multi-site",
    },
    MemorySizingPreset {
        key: "tier_16gb",
        label: "16GB",
        ram: "12-24GB",
        swap: "4GB",
        os_reserve: "2G",
        php_max_children: "64",
        php_processes: "start=8,min_spare=8,max_spare=16",
        php_cpu_guard: "min(memory_budget, vCPU*10)",
        php_memory_limit: "384M",
        php_upload_limit: "256M",
        opcache_memory: "512M",
        db_buffer_pool: "5G",
        db_max_connections: "250",
        db_tmp_table_size: "512M",
        redis_maxmemory: "1G",
        nginx_worker_processes: "min(vCPU,4)",
        nginx_worker_connections: "8192",
        nginx_worker_rlimit_nofile: "32768",
        nginx_keepalive_timeout: "30s",
        nginx_fastcgi_buffers: "32 32k",
        apache_mpm: "event",
        apache_start_servers: "4",
        apache_server_limit: "12",
        apache_threads_per_child: "25",
        apache_max_request_workers: "300",
        apache_spare_threads: "min=75,max=150",
        apache_max_connections_per_child: "5000",
        note: "high traffic single site",
    },
    MemorySizingPreset {
        key: "tier_32gb",
        label: "32GB",
        ram: "24-32GB",
        swap: "4GB",
        os_reserve: "3G",
        php_max_children: "96",
        php_processes: "start=12,min_spare=12,max_spare=24",
        php_cpu_guard: "min(memory_budget, vCPU*12, 96)",
        php_memory_limit: "512M",
        php_upload_limit: "256M",
        opcache_memory: "768M",
        db_buffer_pool: "10G",
        db_max_connections: "400",
        db_tmp_table_size: "512M",
        redis_maxmemory: "2G",
        nginx_worker_processes: "min(vCPU,8)",
        nginx_worker_connections: "16384",
        nginx_worker_rlimit_nofile: "65535",
        nginx_keepalive_timeout: "30s",
        nginx_fastcgi_buffers: "64 32k",
        apache_mpm: "event",
        apache_start_servers: "4",
        apache_server_limit: "16",
        apache_threads_per_child: "25",
        apache_max_request_workers: "400",
        apache_spare_threads: "min=100,max=200",
        apache_max_connections_per_child: "10000",
        note: "large single site, cap PHP until real traffic is measured",
    },
    MemorySizingPreset {
        key: "tier_gt32gb",
        label: ">32GB",
        ram: "32GB+",
        swap: "4GB fixed unless dump/hibernate policy requires more",
        os_reserve: "max(4GB, RAM*10%)",
        php_max_children: "min(floor(php_budget/128M), 192 per site)",
        php_processes: "start=min(16,max_children/8), spare=25%",
        php_cpu_guard: "min(memory_budget, vCPU*12, 192 per site)",
        php_memory_limit: "512M default, app profile may raise",
        php_upload_limit: "256M default, app profile may raise",
        opcache_memory: "min(max(RAM*2%, 768M), 1G)",
        db_buffer_pool: "min(RAM*40%, 24G) for single DB host",
        db_max_connections: "min(800, RAM_GB*12)",
        db_tmp_table_size: "min(1G, RAM*2%)",
        redis_maxmemory: "min(RAM*6%, 4G)",
        nginx_worker_processes: "min(vCPU,16)",
        nginx_worker_connections: "min(32768, RAM_GB*512)",
        nginx_worker_rlimit_nofile: "min(131072, workers*connections*2)",
        nginx_keepalive_timeout: "30s default, lower under L7 proxy pressure",
        nginx_fastcgi_buffers: "64 32k, tune by response size",
        apache_mpm: "event",
        apache_start_servers: "min(vCPU,8)",
        apache_server_limit: "ceil(max_request_workers/25)",
        apache_threads_per_child: "25",
        apache_max_request_workers: "min(vCPU*64, 800 per site)",
        apache_spare_threads: "25% of max workers",
        apache_max_connections_per_child: "10000",
        note: "formula mode; cap per site before multi-tenant support",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMemorySizing {
    pub tier_key: String,
    pub tier_label: String,
    pub total_memory_kib: u64,
    pub vcpu_count: usize,
    pub php_max_children: u16,
    pub php_start_servers: u16,
    pub php_min_spare_servers: u16,
    pub php_max_spare_servers: u16,
    pub php_memory_limit: String,
    pub php_upload_limit: String,
    pub opcache_memory: String,
    pub db_buffer_pool: String,
    pub db_max_connections: u16,
    pub db_tmp_table_size: String,
    pub redis_maxmemory: String,
    pub nginx_worker_processes: u16,
    pub nginx_worker_connections: u32,
    pub nginx_worker_rlimit_nofile: u32,
    pub nginx_keepalive_timeout: String,
    pub nginx_fastcgi_buffers: String,
    pub apache_max_request_workers: u16,
    pub note: String,
}

pub fn resolve_memory_sizing(total_memory_kib: u64, vcpu_count: usize) -> ResolvedMemorySizing {
    let total_mib = (total_memory_kib / 1024).max(1);
    let vcpu_count = vcpu_count.max(1);
    let preset = memory_preset_for_mib(total_mib);

    if preset.key == "tier_gt32gb" {
        return resolved_formula_sizing(preset, total_memory_kib, total_mib, vcpu_count);
    }

    let (php_start_servers, php_min_spare_servers, php_max_spare_servers) =
        php_process_counts_for_preset(preset.key);
    let nginx_worker_processes = nginx_worker_processes_for_preset(preset.key, vcpu_count);
    let db_max_connections = preset.db_max_connections.parse::<u16>().unwrap_or(100);
    let nginx_worker_connections = preset
        .nginx_worker_connections
        .parse::<u32>()
        .unwrap_or(1024);
    let nginx_worker_rlimit_nofile = preset
        .nginx_worker_rlimit_nofile
        .parse::<u32>()
        .unwrap_or(4096);
    let apache_max_request_workers = preset
        .apache_max_request_workers
        .parse::<u16>()
        .unwrap_or(100);

    ResolvedMemorySizing {
        tier_key: preset.key.to_string(),
        tier_label: preset.label.to_string(),
        total_memory_kib,
        vcpu_count,
        php_max_children: preset.php_max_children.parse::<u16>().unwrap_or(8),
        php_start_servers,
        php_min_spare_servers,
        php_max_spare_servers,
        php_memory_limit: preset.php_memory_limit.to_string(),
        php_upload_limit: preset.php_upload_limit.to_string(),
        opcache_memory: preset.opcache_memory.to_string(),
        db_buffer_pool: preset.db_buffer_pool.to_string(),
        db_max_connections,
        db_tmp_table_size: preset.db_tmp_table_size.to_string(),
        redis_maxmemory: preset.redis_maxmemory.to_string(),
        nginx_worker_processes,
        nginx_worker_connections,
        nginx_worker_rlimit_nofile,
        nginx_keepalive_timeout: preset.nginx_keepalive_timeout.to_string(),
        nginx_fastcgi_buffers: preset.nginx_fastcgi_buffers.to_string(),
        apache_max_request_workers,
        note: preset.note.to_string(),
    }
}

fn memory_preset_for_mib(total_mib: u64) -> &'static MemorySizingPreset {
    if total_mib <= 1536 {
        &MEMORY_SIZING_PRESETS[0]
    } else if total_mib <= 3072 {
        &MEMORY_SIZING_PRESETS[1]
    } else if total_mib <= 6144 {
        &MEMORY_SIZING_PRESETS[2]
    } else if total_mib <= 12288 {
        &MEMORY_SIZING_PRESETS[3]
    } else if total_mib <= 24576 {
        &MEMORY_SIZING_PRESETS[4]
    } else if total_mib <= 32768 {
        &MEMORY_SIZING_PRESETS[5]
    } else {
        &MEMORY_SIZING_PRESETS[6]
    }
}

fn php_process_counts_for_preset(key: &str) -> (u16, u16, u16) {
    match key {
        "tier_1gb" => (1, 1, 2),
        "tier_2gb" => (2, 2, 4),
        "tier_4gb" => (4, 4, 8),
        "tier_8gb" => (6, 6, 12),
        "tier_16gb" => (8, 8, 16),
        "tier_32gb" => (12, 12, 24),
        _ => (16, 16, 48),
    }
}

fn nginx_worker_processes_for_preset(key: &str, vcpu_count: usize) -> u16 {
    let cap = match key {
        "tier_1gb" => 1,
        "tier_2gb" | "tier_4gb" => 2,
        "tier_8gb" | "tier_16gb" => 4,
        "tier_32gb" => 8,
        _ => 16,
    };
    vcpu_count.min(cap).max(1) as u16
}

fn resolved_formula_sizing(
    preset: &'static MemorySizingPreset,
    total_memory_kib: u64,
    total_mib: u64,
    vcpu_count: usize,
) -> ResolvedMemorySizing {
    let ram_gb = (total_mib / 1024).max(33);
    let php_max_children = ((total_mib / 4) / 128)
        .max(32)
        .min((vcpu_count as u64 * 12).min(192)) as u16;
    let php_start_servers = (php_max_children / 8).clamp(4, 16);
    let php_spare = (php_max_children / 4).max(8);
    let opcache_mib = ((total_mib * 2) / 100).clamp(768, 1024);
    let db_buffer_pool_gb = ((ram_gb * 40) / 100).clamp(10, 24);
    let db_max_connections = (ram_gb * 12).clamp(400, 800) as u16;
    let db_tmp_table_mib = ((total_mib * 2) / 100).clamp(512, 1024);
    let redis_mib = ((total_mib * 6) / 100).clamp(2048, 4096);
    let nginx_worker_processes = vcpu_count.clamp(1, 16) as u16;
    let nginx_worker_connections = (ram_gb * 512).clamp(16384, 32768) as u32;
    let nginx_worker_rlimit_nofile =
        (nginx_worker_processes as u32 * nginx_worker_connections * 2).clamp(65535, 131072);
    let apache_max_request_workers = (vcpu_count as u16 * 64).clamp(400, 800);

    ResolvedMemorySizing {
        tier_key: preset.key.to_string(),
        tier_label: preset.label.to_string(),
        total_memory_kib,
        vcpu_count,
        php_max_children,
        php_start_servers,
        php_min_spare_servers: php_spare,
        php_max_spare_servers: php_spare.saturating_mul(2).min(php_max_children),
        php_memory_limit: "512M".to_string(),
        php_upload_limit: "256M".to_string(),
        opcache_memory: format!("{opcache_mib}M"),
        db_buffer_pool: format!("{db_buffer_pool_gb}G"),
        db_max_connections,
        db_tmp_table_size: format!("{db_tmp_table_mib}M"),
        redis_maxmemory: format!("{redis_mib}M"),
        nginx_worker_processes,
        nginx_worker_connections,
        nginx_worker_rlimit_nofile,
        nginx_keepalive_timeout: "30s".to_string(),
        nginx_fastcgi_buffers: "64 32k".to_string(),
        apache_max_request_workers,
        note: preset.note.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOptions {
    pub local_test: bool,
    pub app_profile: String,
    pub web_server: String,
    pub php_version: String,
    pub php_source: String,
    pub database_engine: String,
    pub site_user: String,
    pub site_user_password: Option<String>,
    pub web_root_mode: String,
    pub custom_web_root: Option<String>,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_from: Option<String>,
    pub smtp_encryption: String,
    pub security_profile: String,
    pub ssh_policy: String,
    pub rollback: bool,
    pub preserve_config: bool,
    pub dns_check: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            local_test: false,
            app_profile: DEFAULT_APP_PROFILE.to_string(),
            web_server: DEFAULT_WEB_SERVER.to_string(),
            php_version: DEFAULT_PHP_VERSION.to_string(),
            php_source: DEFAULT_PHP_SOURCE.to_string(),
            database_engine: DEFAULT_DATABASE_ENGINE.to_string(),
            site_user: DEFAULT_SITE_USER.to_string(),
            site_user_password: None,
            web_root_mode: DEFAULT_WEB_ROOT_MODE.to_string(),
            custom_web_root: None,
            www_mode: DEFAULT_WWW_MODE.to_string(),
            redis_mode: DEFAULT_REDIS_MODE.to_string(),
            mail_mode: DEFAULT_MAIL_MODE.to_string(),
            smtp_host: None,
            smtp_port: DEFAULT_SMTP_PORT,
            smtp_from: None,
            smtp_encryption: DEFAULT_SMTP_ENCRYPTION.to_string(),
            security_profile: DEFAULT_SECURITY_PROFILE.to_string(),
            ssh_policy: DEFAULT_SSH_POLICY.to_string(),
            rollback: true,
            preserve_config: true,
            dns_check: true,
        }
    }
}

pub fn build(domain: String) -> Result<InstallPlan> {
    build_with_options(domain, PlanOptions::default())
}

pub fn build_with_options(domain: String, options: PlanOptions) -> Result<InstallPlan> {
    let domain = normalize_domain(domain)?;
    let app_profile = resolve_app_profile(&options.app_profile)?;
    let web_server =
        normalize_supported_option("web-server", options.web_server, &SUPPORTED_WEB_SERVERS)?;
    let php_version = normalize_php_version(options.php_version)?;
    let database_engine = normalize_supported_option(
        "database",
        options.database_engine,
        &SUPPORTED_DATABASE_ENGINES,
    )?;
    let php_source = normalize_php_source(&php_version, options.php_source)?;
    let site_user = normalize_site_user(options.site_user)?;
    validate_site_user_password(options.site_user_password.as_deref())?;
    let web_root_mode = normalize_web_root_mode(options.web_root_mode, &options.custom_web_root)?;
    let web_root = web_root_for(
        &domain,
        &site_user,
        &web_root_mode,
        options.custom_web_root.as_deref(),
    )?;
    let app_document_root = app_profile.document_root_for(&web_root);
    let www_mode = normalize_supported_option("www-mode", options.www_mode, &SUPPORTED_WWW_MODES)?;
    let redis_mode =
        normalize_supported_option("redis", options.redis_mode, &SUPPORTED_REDIS_MODES)?;
    let mail_mode =
        normalize_supported_option("mail-mode", options.mail_mode, &SUPPORTED_MAIL_MODES)?;
    let smtp_encryption = normalize_supported_option(
        "smtp-encryption",
        options.smtp_encryption,
        &SUPPORTED_SMTP_ENCRYPTION,
    )?;
    let security_profile = normalize_supported_option(
        "security-profile",
        options.security_profile,
        &SUPPORTED_SECURITY_PROFILES,
    )?;
    let ssh_policy =
        normalize_supported_option("ssh-policy", options.ssh_policy, &SUPPORTED_SSH_POLICIES)?;
    validate_mail_options(
        &mail_mode,
        options.smtp_host.as_deref(),
        options.smtp_from.as_deref(),
    )?;
    let smtp_port = smtp_port_for_mode(&mail_mode, options.smtp_port);
    let database_name = database_name_for_domain(&domain, app_profile.id);
    let database_user = database_user_for_site_user(&site_user, app_profile.id);

    let dns_check_required = options.dns_check && !options.local_test;
    let deployment_mode = if options.local_test {
        "local-test"
    } else {
        "public"
    }
    .to_string();
    let packages = packages(
        &web_server,
        &php_version,
        &php_source,
        &database_engine,
        &redis_mode,
        &mail_mode,
        options.local_test,
    );
    let files = files(
        app_profile,
        &web_server,
        &web_root,
        &redis_mode,
        &mail_mode,
        options.local_test,
    );
    let services = services(
        app_profile,
        &web_server,
        &php_version,
        &database_engine,
        &redis_mode,
        &mail_mode,
        options.local_test,
    );
    let ports = ports(&redis_mode, &mail_mode, smtp_port, options.local_test);
    let security_checks = security_checks(
        &redis_mode,
        &database_engine,
        &security_profile,
        &ssh_policy,
        options.local_test,
    );
    let app_requirements = app_requirements(
        app_profile,
        &php_version,
        &database_engine,
        &redis_mode,
        options.local_test,
    );
    let app_followup_steps = app_profile.followup_steps();
    let provisioning = provisioning_sections(ProvisioningInput {
        domain: &domain,
        app_profile: app_profile.id,
        app_document_root: &app_document_root,
        web_server: &web_server,
        php_version: &php_version,
        php_source: &php_source,
        database_engine: &database_engine,
        database_name: &database_name,
        database_user: &database_user,
        site_user: &site_user,
        web_root: &web_root,
        www_mode: &www_mode,
        redis_mode: &redis_mode,
        mail_mode: &mail_mode,
        smtp_port,
        security_profile: &security_profile,
        ssh_policy: &ssh_policy,
        local_test: options.local_test,
    });
    let stop_conditions = stop_conditions(&web_server, &web_root, options.local_test);

    Ok(InstallPlan {
        domain,
        deployment_mode,
        app_profile: app_profile.id.to_string(),
        app_profile_label: app_profile.label,
        app_summary: app_profile.summary,
        app_document_root,
        web_server,
        php_version: php_version.clone(),
        php_source,
        database_engine,
        site_user,
        web_root_mode,
        web_root,
        www_mode,
        redis_mode,
        mail_mode: mail_mode.clone(),
        smtp_host: options.smtp_host,
        smtp_port: smtp_port_for_plan(&mail_mode, smtp_port),
        smtp_from: options.smtp_from,
        smtp_encryption: smtp_encryption_for_plan(&mail_mode, smtp_encryption),
        security_profile,
        ssh_policy,
        database_name,
        database_user,
        database_password_policy: "generate-random-store-root-only",
        rollback_enabled: options.rollback,
        preserve_config: options.preserve_config,
        dns_check_required,
        mode: "dry-run",
        fresh_server_only: true,
        changes_made: false,
        preflight_gates: preflight_gates(options.local_test),
        packages,
        files,
        services,
        ports,
        security_checks,
        app_requirements,
        app_followup_steps,
        provisioning,
        stop_conditions,
    })
}

fn preflight_gates(local_test: bool) -> Vec<PlanGate> {
    let mut gates = vec![
        PlanGate {
            name: "os",
            description: "Require Ubuntu 24.04 LTS.",
        },
        PlanGate {
            name: "privilege",
            description: "Install requires root or sudo.",
        },
        PlanGate {
            name: "fresh-server",
            description: "Abort if existing web services or unowned G7 paths are detected.",
        },
        PlanGate {
            name: "site-account",
            description: "Verify or create the selected site account before placing app files.",
        },
        PlanGate {
            name: "web-root",
            description: "Use the selected site account public_html/www/custom root; do not assume /var/www/g7.",
        },
        PlanGate {
            name: "network",
            description: if local_test {
                "Require port 80 for local HTTP setup."
            } else {
                "Require ports 80 and 443 before HTTP/HTTPS setup."
            },
        },
    ];

    if local_test {
        gates.push(PlanGate {
            name: "local-hostname",
            description: "Use a local test hostname without public DNS or Let's Encrypt.",
        });
    } else {
        gates.push(PlanGate {
            name: "dns-public-ip",
            description: "Verify domain A/AAAA records match this VPS public IP before Certbot.",
        });
    }

    gates.extend([
        PlanGate {
            name: "www-canonical",
            description: "Apply requested root/www canonical host policy.",
        },
        PlanGate {
            name: "mail-outbound",
            description: "Check selected SMTP outbound port before writing mail settings.",
        },
        PlanGate {
            name: "server-security",
            description: "Audit Redis, database, firewall, SSH, PHP, and file permissions before applying changes.",
        },
        PlanGate {
            name: "rollback",
            description: "Track created installer-owned files for rollback on failure.",
        },
        PlanGate {
            name: "config-preserve",
            description: "Preserve existing configuration instead of overwriting unowned files.",
        },
    ]);

    if !local_test {
        gates.push(PlanGate {
            name: "certbot-renewal",
            description: "Enable Let's Encrypt renewal through certbot.timer.",
        });
    }

    gates
}

fn packages(
    web_server: &str,
    php_version: &str,
    php_source: &str,
    database_engine: &str,
    redis_mode: &str,
    mail_mode: &str,
    local_test: bool,
) -> Vec<PlanPackage> {
    let mut packages = vec![
        PlanPackage {
            name: web_server_package(web_server).to_string(),
            description: "Web server and reverse proxy.",
        },
        PlanPackage {
            name: format!("php{php_version}-fpm"),
            description: "PHP-FPM runtime for the selected app.",
        },
        PlanPackage {
            name: format!("php{php_version}-mysql php{php_version}-mbstring php{php_version}-xml"),
            description: "Core PHP extensions for database, strings, and XML.",
        },
        PlanPackage {
            name: format!("php{php_version}-curl php{php_version}-gd php{php_version}-zip"),
            description: "PHP extensions for HTTP, images, and archives.",
        },
        PlanPackage {
            name: format!("php{php_version}-intl php{php_version}-bcmath"),
            description: "PHP extensions for locale and decimal math.",
        },
        PlanPackage {
            name: format!("php{php_version}-imagick"),
            description: "Image processing extension for app media support.",
        },
        PlanPackage {
            name: database_package(database_engine).to_string(),
            description: "Selected SQL database server.",
        },
        PlanPackage {
            name: "curl unzip ca-certificates".to_string(),
            description: "Release download and extraction utilities.",
        },
    ];

    if php_source == PHP_SOURCE_ONDREJ {
        packages.push(PlanPackage {
            name: "software-properties-common lsb-release".to_string(),
            description: "Required to add the Ondrej PHP PPA for non-default PHP versions.",
        });
    }

    if !local_test {
        packages.push(PlanPackage {
            name: "certbot".to_string(),
            description: "Let's Encrypt certificate issuance.",
        });
        packages.push(PlanPackage {
            name: certbot_web_plugin_package(web_server).to_string(),
            description: "Certbot web server integration.",
        });
    }

    if redis_mode == "enable" {
        packages.push(PlanPackage {
            name: "redis-server".to_string(),
            description: "Local Redis cache/session/queue backend.",
        });
        packages.push(PlanPackage {
            name: format!("php{php_version}-redis"),
            description: "PHP Redis extension.",
        });
    }

    if mail_mode == "local-postfix" {
        packages.push(PlanPackage {
            name: "postfix mailutils".to_string(),
            description: "Optional local outbound mail transport.",
        });
    }

    packages
}

fn files(
    app_profile: &crate::app_profile::AppProfile,
    web_server: &str,
    web_root: &str,
    redis_mode: &str,
    mail_mode: &str,
    local_test: bool,
) -> Vec<PlanFile> {
    let mut files = vec![
        PlanFile::new("/etc/g7-installer/config.toml", "create"),
        PlanFile::new(STATE_PATH, "create/update"),
        PlanFile::new(OWNED_FILES_PATH, "create/update"),
        PlanFile::new("/var/lib/g7-installer/rollback.json", "create/update"),
        PlanFile::new(
            "/var/backups/g7-installer",
            "create for preserved config snapshots",
        ),
        PlanFile::new("/var/log/g7-installer/install.log", "create/append"),
        PlanFile::new(
            "/var/log/g7-installer/report.json",
            "create/update problem report",
        ),
        PlanFile::new(
            web_root,
            "planned app web root; create or verify in install phase",
        ),
        PlanFile::new(
            app_config_file(app_profile, web_root),
            "create app config with DB/cache/mail settings using root-only secret handling",
        ),
        web_server_available_file(web_server),
        web_server_enabled_file(web_server),
    ];

    for service in app_profile.services {
        files.push(PlanFile::new(
            format!("/etc/systemd/system/{service}"),
            "create in app phase when enabled",
        ));
    }

    if redis_mode == "enable" {
        files.push(PlanFile::new(
            "/etc/g7-installer/redis.conf",
            "create Redis hardening overlay",
        ));
    }

    if local_test {
        files.push(PlanFile::new(
            "/etc/g7-installer/local-hosts.txt",
            "write local hosts entry suggestion",
        ));
    }

    if mail_mode != "none" {
        files.push(PlanFile::new(
            "/etc/g7-installer/mail.toml",
            "create SMTP delivery settings without secrets",
        ));
    }

    files
}

fn services(
    app_profile: &crate::app_profile::AppProfile,
    web_server: &str,
    php_version: &str,
    database_engine: &str,
    redis_mode: &str,
    mail_mode: &str,
    local_test: bool,
) -> Vec<PlanService> {
    let mut services = vec![
        PlanService {
            name: web_server_service(web_server).to_string(),
            action: "enable and reload",
        },
        PlanService {
            name: format!("php{php_version}-fpm"),
            action: "enable and restart",
        },
        PlanService {
            name: database_service(database_engine).to_string(),
            action: "bind locally, create app database/user, enable and start",
        },
    ];

    for service in app_profile.services {
        services.push(PlanService {
            name: (*service).to_string(),
            action: "create and verify in app phase",
        });
    }

    if !local_test {
        services.push(PlanService {
            name: "certbot.timer".to_string(),
            action: "enable and verify renewal timer",
        });
    }

    if redis_mode == "enable" {
        services.push(PlanService {
            name: "redis-server".to_string(),
            action: "bind to 127.0.0.1, cap memory, enable and restart",
        });
    }

    if mail_mode == "local-postfix" {
        services.push(PlanService {
            name: "postfix".to_string(),
            action: "configure outbound-only mail transport",
        });
    }

    services
}

fn ports(redis_mode: &str, mail_mode: &str, smtp_port: u16, local_test: bool) -> Vec<PlanPort> {
    let mut ports = vec![PlanPort {
        port: 80,
        protocol: "tcp",
        purpose: if local_test {
            "Inbound local HTTP traffic."
        } else {
            "Inbound HTTP and Let's Encrypt challenge."
        },
    }];

    if !local_test {
        ports.push(PlanPort {
            port: 443,
            protocol: "tcp",
            purpose: "Inbound HTTPS traffic.",
        });
    }

    ports.push(PlanPort {
        port: 3306,
        protocol: "tcp",
        purpose: "Localhost-only SQL database. Must not be open to the public internet.",
    });

    if redis_mode == "enable" {
        ports.push(PlanPort {
            port: 6379,
            protocol: "tcp",
            purpose: "Localhost-only Redis. Must not be open to the public internet.",
        });
    }

    if mail_mode == "smtp-relay" || mail_mode == "local-postfix" {
        ports.push(PlanPort {
            port: smtp_port,
            protocol: "tcp",
            purpose: "Outbound SMTP delivery check.",
        });
    }

    ports
}

fn security_checks(
    redis_mode: &str,
    database_engine: &str,
    security_profile: &str,
    ssh_policy: &str,
    local_test: bool,
) -> Vec<PlanSecurityCheck> {
    let mut checks = vec![
        PlanSecurityCheck {
            name: "filesystem-permissions",
            level: "apply",
            description: "Site files owned by the site account; web server gets read access; writable directories stay limited.",
        },
        PlanSecurityCheck {
            name: "database-credentials",
            level: "apply",
            description: "Generate a random app DB password; never use a default password or print secrets to stdout/logs.",
        },
        PlanSecurityCheck {
            name: "database-bind",
            level: "apply",
            description: if database_engine == "mysql" {
                "Keep MySQL bound to localhost/unix socket and create a least-privilege G7 app user."
            } else {
                "Keep MariaDB bound to localhost/unix socket and create a least-privilege G7 app user."
            },
        },
        PlanSecurityCheck {
            name: "ssh-config",
            level: if ssh_policy == "harden" {
                "apply"
            } else {
                "audit"
            },
            description: if ssh_policy == "harden" {
                "Harden sshd after preserving the active SSH port; do not lock out the current session."
            } else {
                "Audit SSH port, root login, and password authentication; do not change SSH automatically."
            },
        },
        PlanSecurityCheck {
            name: "firewall",
            level: if security_profile == "hardened" {
                "apply"
            } else {
                "audit"
            },
            description: "Allow the active SSH port plus 80/443; keep database and Redis ports closed externally.",
        },
        PlanSecurityCheck {
            name: "php-runtime",
            level: "apply",
            description: "Apply PHP-FPM pool limits, opcache settings, upload limits, and per-site runtime isolation.",
        },
    ];

    if redis_mode == "enable" {
        checks.push(PlanSecurityCheck {
            name: "redis-local-only",
            level: "apply",
            description: "Bind Redis to 127.0.0.1/::1 or unix socket, keep protected-mode enabled, and never expose 6379 publicly.",
        });
    }

    if !local_test {
        checks.push(PlanSecurityCheck {
            name: "tls-headers",
            level: "apply",
            description: "Issue HTTPS certificates and apply sane TLS/security headers after domain ownership checks pass.",
        });
    }

    checks
}

fn app_requirements(
    profile: &crate::app_profile::AppProfile,
    php_version: &str,
    database_engine: &str,
    redis_mode: &str,
    local_test: bool,
) -> Vec<AppRequirement> {
    let mut requirements = vec![
        php_version_requirement(profile.min_php, php_version),
        AppRequirement {
            name: "database-version".to_string(),
            status: "deferred",
            message: format!(
                "{} selected; app requires {}. Exact server version is verified in the database phase.",
                database_engine, profile.database_requirement
            ),
        },
        AppRequirement {
            name: "document-root".to_string(),
            status: "planned",
            message: match profile.document_root {
                crate::app_profile::DocumentRootStrategy::SiteRoot => {
                    "web server document root uses the selected site root".to_string()
                }
                crate::app_profile::DocumentRootStrategy::PublicSubdir => {
                    "web server document root must point to the app public/ directory".to_string()
                }
            },
        },
    ];

    for extension in profile.php_extensions {
        requirements.push(php_extension_requirement(
            extension,
            php_version,
            redis_mode,
        ));
    }

    for package in profile.system_packages {
        requirements.push(AppRequirement {
            name: format!("system-package:{package}"),
            status: "deferred",
            message: "required by the selected app profile; install in the app phase".to_string(),
        });
    }

    for service in profile.services {
        requirements.push(AppRequirement {
            name: format!("service:{service}"),
            status: "deferred",
            message: "required by the selected app profile; create and verify in the app phase"
                .to_string(),
        });
    }

    for path in profile.writable_paths {
        requirements.push(AppRequirement {
            name: format!("writable:{path}"),
            status: "deferred",
            message: "must be owned by the site account with limited write permissions".to_string(),
        });
    }

    for check in profile.health_checks {
        requirements.push(AppRequirement {
            name: format!("health:{check}"),
            status: "deferred",
            message: "run after app files, vhost, and database settings are applied".to_string(),
        });
    }

    requirements.push(AppRequirement {
        name: "https".to_string(),
        status: if local_test { "skipped" } else { "deferred" },
        message: if local_test {
            "local-test mode skips public TLS issuance".to_string()
        } else {
            "issue and renew Let's Encrypt certificate after app/vhost phase".to_string()
        },
    });

    requirements
}

fn php_version_requirement(min_php: &str, selected_php: &str) -> AppRequirement {
    if php_version_at_least(selected_php, min_php) {
        AppRequirement {
            name: "php-version".to_string(),
            status: "pass",
            message: format!("PHP {selected_php} satisfies app minimum PHP {min_php}."),
        }
    } else {
        AppRequirement {
            name: "php-version".to_string(),
            status: "fail",
            message: format!("PHP {selected_php} is lower than app minimum PHP {min_php}."),
        }
    }
}

fn php_extension_requirement(
    extension: &str,
    php_version: &str,
    redis_mode: &str,
) -> AppRequirement {
    if extension == "redis" && redis_mode == "disable" {
        return AppRequirement {
            name: "php-extension:redis".to_string(),
            status: "fail",
            message: "selected app profile requires Redis, but Redis is disabled".to_string(),
        };
    }

    match package_phase_php_extension_package(extension, php_version) {
        Some(package) => AppRequirement {
            name: format!("php-extension:{extension}"),
            status: "planned",
            message: format!(
                "{package} is included in the package phase; php -m verification belongs to the runtime phase"
            ),
        },
        None => AppRequirement {
            name: format!("php-extension:{extension}"),
            status: "deferred",
            message: "verify with php -m in the runtime/app compatibility phase".to_string(),
        },
    }
}

fn package_phase_php_extension_package(extension: &str, php_version: &str) -> Option<String> {
    let package = match extension {
        "bcmath" => "bcmath",
        "curl" => "curl",
        "dom" | "simplexml" | "xml" | "xmlwriter" => "xml",
        "gd" => "gd",
        "imagick" => "imagick",
        "intl" => "intl",
        "mbstring" => "mbstring",
        "mysqli" | "mysqlnd" | "pdo_mysql" => "mysql",
        "redis" => "redis",
        "zip" => "zip",
        _ => return None,
    };

    Some(format!("php{php_version}-{package}"))
}

fn memory_sizing_settings() -> Vec<ProvisioningSetting> {
    let mut settings = vec![
        ProvisioningSetting::new("preset_tiers", "1GB, 2GB, 4GB, 8GB, 16GB, 32GB, >32GB"),
        ProvisioningSetting::new("swap_by_ram", preset_matrix(|preset| preset.swap)),
        ProvisioningSetting::new(
            "os_reserve_by_ram",
            preset_matrix(|preset| preset.os_reserve),
        ),
        ProvisioningSetting::new(
            "php_cpu_guard_by_ram",
            preset_matrix(|preset| preset.php_cpu_guard),
        ),
        ProvisioningSetting::new(
            "nginx_worker_processes_by_cpu_ram",
            preset_matrix(|preset| preset.nginx_worker_processes),
        ),
        ProvisioningSetting::new(
            "apache_max_request_workers_by_ram",
            preset_matrix(|preset| preset.apache_max_request_workers),
        ),
    ];

    settings.extend(MEMORY_SIZING_PRESETS.iter().map(|preset| {
        ProvisioningSetting::new(
            preset.key,
            format!(
                "ram={}, swap={}, os_reserve={}, php_max_children={}, php_pool={}, php_cpu_guard={}, php_memory_limit={}, upload={}, opcache={}, db_buffer_pool={}, db_max_connections={}, db_tmp_table_size={}, redis_maxmemory={}, nginx_worker_processes={}, nginx_worker_connections={}, nginx_rlimit_nofile={}, nginx_keepalive_timeout={}, nginx_fastcgi_buffers={}, apache_mpm={}, apache_start_servers={}, apache_server_limit={}, apache_threads_per_child={}, apache_max_request_workers={}, apache_spare_threads={}, apache_max_connections_per_child={}, note={}",
                preset.ram,
                preset.swap,
                preset.os_reserve,
                preset.php_max_children,
                preset.php_processes,
                preset.php_cpu_guard,
                preset.php_memory_limit,
                preset.php_upload_limit,
                preset.opcache_memory,
                preset.db_buffer_pool,
                preset.db_max_connections,
                preset.db_tmp_table_size,
                preset.redis_maxmemory,
                preset.nginx_worker_processes,
                preset.nginx_worker_connections,
                preset.nginx_worker_rlimit_nofile,
                preset.nginx_keepalive_timeout,
                preset.nginx_fastcgi_buffers,
                preset.apache_mpm,
                preset.apache_start_servers,
                preset.apache_server_limit,
                preset.apache_threads_per_child,
                preset.apache_max_request_workers,
                preset.apache_spare_threads,
                preset.apache_max_connections_per_child,
                preset.note
            ),
        )
    }));

    settings
}

fn preset_matrix<F>(value: F) -> String
where
    F: Fn(&MemorySizingPreset) -> &'static str,
{
    MEMORY_SIZING_PRESETS
        .iter()
        .map(|preset| format!("{}={}", preset.label, value(preset)))
        .collect::<Vec<_>>()
        .join(", ")
}

struct ProvisioningInput<'a> {
    domain: &'a str,
    app_profile: &'a str,
    app_document_root: &'a str,
    web_server: &'a str,
    php_version: &'a str,
    php_source: &'a str,
    database_engine: &'a str,
    database_name: &'a str,
    database_user: &'a str,
    site_user: &'a str,
    web_root: &'a str,
    www_mode: &'a str,
    redis_mode: &'a str,
    mail_mode: &'a str,
    smtp_port: u16,
    security_profile: &'a str,
    ssh_policy: &'a str,
    local_test: bool,
}

fn provisioning_sections(input: ProvisioningInput<'_>) -> Vec<ProvisioningSection> {
    let mut server_sizing_settings = vec![
        ProvisioningSetting::new("size_probe", "RAM, vCPU, disk, swap 상태를 먼저 감지"),
        ProvisioningSetting::new(
            "tier_selection",
            "MemTotal GiB를 가장 가까운 보수 등급으로 내림 선택하고, 32GB 초과는 공식 적용",
        ),
        ProvisioningSetting::new(
            "memory_budget",
            "OS reserve, DB, Redis, PHP-FPM, web server 순서로 메모리 예산 분배",
        ),
        ProvisioningSetting::new(
            "profile_floor",
            "1GB RAM / 2 vCPU / 40GB SSD 기준에서도 과부하를 피하는 값 우선",
        ),
    ];
    server_sizing_settings.extend(memory_sizing_settings());

    let mut sections = vec![
        ProvisioningSection {
            name: "server-sizing",
            title: "서버 사양 기반 튜닝",
            summary: "1/2/4/8/16/32GB 프리셋과 32GB 초과 공식으로 메모리 중심 값을 선택합니다."
                .to_string(),
            settings: server_sizing_settings,
        },
        ProvisioningSection {
            name: "web-server",
            title: "웹서버 호스트 설정",
            summary: format!(
                "{} vhost를 {} 문서 루트에 맞춰 생성하고 root/www 정책을 적용합니다.",
                runtime_label(input.web_server),
                input.app_document_root
            ),
            settings: vec![
                ProvisioningSetting::new("server_name", server_names(input.domain, input.www_mode)),
                ProvisioningSetting::new(
                    "redirect_source",
                    redirect_source(input.domain, input.www_mode),
                ),
                ProvisioningSetting::new("document_root", input.app_document_root),
                ProvisioningSetting::new("site_root", input.web_root),
                ProvisioningSetting::new(
                    "php_socket",
                    format!("/run/php/php{}-fpm.sock", input.php_version),
                ),
                ProvisioningSetting::new("rewrite_policy", rewrite_policy(input.app_profile)),
                ProvisioningSetting::new("selected_runtime", web_runtime_model(input.web_server)),
                ProvisioningSetting::new(
                    "nginx_worker_processes_by_cpu_ram",
                    preset_matrix(|preset| preset.nginx_worker_processes),
                ),
                ProvisioningSetting::new(
                    "nginx_worker_connections_by_ram",
                    preset_matrix(|preset| preset.nginx_worker_connections),
                ),
                ProvisioningSetting::new(
                    "nginx_worker_rlimit_nofile_by_ram",
                    preset_matrix(|preset| preset.nginx_worker_rlimit_nofile),
                ),
                ProvisioningSetting::new(
                    "nginx_keepalive_timeout_by_ram",
                    preset_matrix(|preset| preset.nginx_keepalive_timeout),
                ),
                ProvisioningSetting::new(
                    "nginx_fastcgi_buffers_by_ram",
                    preset_matrix(|preset| preset.nginx_fastcgi_buffers),
                ),
                ProvisioningSetting::new("apache_mpm", "event + proxy_fcgi + PHP-FPM"),
                ProvisioningSetting::new(
                    "apache_start_servers_by_ram",
                    preset_matrix(|preset| preset.apache_start_servers),
                ),
                ProvisioningSetting::new(
                    "apache_server_limit_by_ram",
                    preset_matrix(|preset| preset.apache_server_limit),
                ),
                ProvisioningSetting::new(
                    "apache_threads_per_child",
                    preset_matrix(|preset| preset.apache_threads_per_child),
                ),
                ProvisioningSetting::new(
                    "apache_max_request_workers_by_ram",
                    preset_matrix(|preset| preset.apache_max_request_workers),
                ),
                ProvisioningSetting::new(
                    "apache_spare_threads_by_ram",
                    preset_matrix(|preset| preset.apache_spare_threads),
                ),
                ProvisioningSetting::new(
                    "apache_max_connections_per_child_by_ram",
                    preset_matrix(|preset| preset.apache_max_connections_per_child),
                ),
                ProvisioningSetting::new(
                    "apache_php_fpm_boundary",
                    "Apache worker 수는 정적/keepalive 처리 여유이고 PHP 동시 실행 상한은 PHP-FPM max_children",
                ),
                ProvisioningSetting::new(
                    "security_headers",
                    "HTTPS 적용 후 HSTS, nosniff, frame deny, referrer policy 후보 적용",
                ),
            ],
        },
        ProvisioningSection {
            name: "php-runtime",
            title: "PHP 런타임 설정",
            summary: format!(
                "PHP {} FPM pool, php.ini, opcache를 앱과 서버 사양 기준으로 조정합니다.",
                input.php_version
            ),
            settings: vec![
                ProvisioningSetting::new("package_source", input.php_source),
                ProvisioningSetting::new("pool_user", input.site_user),
                ProvisioningSetting::new(
                    "pm_policy",
                    "dynamic; max_children은 감지 RAM과 vCPU로 계산",
                ),
                ProvisioningSetting::new(
                    "max_children_by_ram",
                    preset_matrix(|preset| preset.php_max_children),
                ),
                ProvisioningSetting::new(
                    "cpu_guard_by_ram",
                    preset_matrix(|preset| preset.php_cpu_guard),
                ),
                ProvisioningSetting::new(
                    "process_pool_by_ram",
                    preset_matrix(|preset| preset.php_processes),
                ),
                ProvisioningSetting::new(
                    "web_server_boundary",
                    "Nginx/Apache worker는 요청 수용 계층이고 PHP 동시 실행은 max_children으로 제한",
                ),
                ProvisioningSetting::new(
                    "memory_limit_by_ram",
                    preset_matrix(|preset| preset.php_memory_limit),
                ),
                ProvisioningSetting::new(
                    "upload_max_filesize_by_ram",
                    preset_matrix(|preset| preset.php_upload_limit),
                ),
                ProvisioningSetting::new(
                    "post_max_size_by_ram",
                    preset_matrix(|preset| preset.php_upload_limit),
                ),
                ProvisioningSetting::new("max_execution_time", "120초 기본 후보"),
                ProvisioningSetting::new(
                    "opcache_memory_by_ram",
                    preset_matrix(|preset| preset.opcache_memory),
                ),
            ],
        },
        ProvisioningSection {
            name: "database",
            title: "DB 생성 및 계정 설정",
            summary: format!(
                "{}에 앱 전용 DB와 최소 권한 계정을 만들고 localhost 전용으로 묶습니다.",
                database_label(input.database_engine)
            ),
            settings: vec![
                ProvisioningSetting::new("database", input.database_name),
                ProvisioningSetting::new("user", input.database_user),
                ProvisioningSetting::new(
                    "password_policy",
                    "무작위 생성 후 root-only 파일에 저장, 화면/로그 출력 금지",
                ),
                ProvisioningSetting::new("bind", "127.0.0.1 또는 unix socket 전용"),
                ProvisioningSetting::new(
                    "buffer_pool_by_ram",
                    preset_matrix(|preset| preset.db_buffer_pool),
                ),
                ProvisioningSetting::new(
                    "max_connections_by_ram",
                    preset_matrix(|preset| preset.db_max_connections),
                ),
                ProvisioningSetting::new(
                    "tmp_table_size_by_ram",
                    preset_matrix(|preset| preset.db_tmp_table_size),
                ),
                ProvisioningSetting::new(
                    "backup_note",
                    "앱 설치 후 DB 백업/복구 경로를 리포트에 표시",
                ),
            ],
        },
        ProvisioningSection {
            name: "firewall",
            title: "방화벽 및 포트 정책",
            summary:
                "SSH, HTTP, HTTPS만 외부 공개하고 DB/Redis/설치 UI 포트는 외부 공개를 차단합니다."
                    .to_string(),
            settings: vec![
                ProvisioningSetting::new("allow", "active SSH port, 80/tcp, 443/tcp"),
                ProvisioningSetting::new("deny", "7717/tcp, 3306/tcp, 6379/tcp inbound"),
                ProvisioningSetting::new(
                    "owner",
                    "Lightsail 방화벽을 1차 기준으로 보고 UFW는 서버 내부 보조 정책으로 적용",
                ),
                ProvisioningSetting::new(
                    "verify",
                    "적용 후 ss/ufw/외부 포트 검사 결과를 리포트에 기록",
                ),
            ],
        },
        ProvisioningSection {
            name: "ssl",
            title: "SSL 인증서 및 자동 갱신",
            summary: if input.local_test {
                "공개 도메인 설치가 아니면 인증서 발급은 건너뜁니다.".to_string()
            } else {
                "도메인 IP 일치 확인 후 Let's Encrypt 인증서를 발급하고 certbot.timer를 검증합니다."
                    .to_string()
            },
            settings: vec![
                ProvisioningSetting::new(
                    "domain_check",
                    "A/AAAA와 www 대상이 현재 VPS 공인 IP와 일치해야 진행",
                ),
                ProvisioningSetting::new("issuer", "Let's Encrypt / Certbot"),
                ProvisioningSetting::new("renewal", "certbot.timer enable + renew dry-run 검증"),
                ProvisioningSetting::new(
                    "fallback",
                    "DNS 불일치 시 HTTP vhost까지만 유지하고 인증서 단계 중단",
                ),
            ],
        },
    ];

    if input.redis_mode == "enable" {
        sections.push(ProvisioningSection {
            name: "redis",
            title: "Redis 캐시 설정",
            summary: "Redis를 로컬 전용 캐시/세션 저장소로 구성하고 서버 RAM에 맞춰 maxmemory를 제한합니다."
                .to_string(),
            settings: vec![
                ProvisioningSetting::new("bind", "127.0.0.1/::1 또는 unix socket 전용"),
                ProvisioningSetting::new("protected_mode", "yes"),
                ProvisioningSetting::new(
                    "maxmemory_by_ram",
                    preset_matrix(|preset| preset.redis_maxmemory),
                ),
                ProvisioningSetting::new("policy", "allkeys-lru 기본 후보"),
            ],
        });
    } else {
        sections.push(ProvisioningSection {
            name: "redis",
            title: "Redis 캐시 설정",
            summary: "Redis 비활성 선택에 따라 설치와 앱 연결 설정을 생략합니다.".to_string(),
            settings: vec![ProvisioningSetting::new("status", "disabled")],
        });
    }

    if input.mail_mode != "none" {
        sections.push(ProvisioningSection {
            name: "mail",
            title: "메일 발송 설정",
            summary:
                "회원 인증/알림 메일 발송만 설정하고 수신 메일 서버는 기본 범위에서 제외합니다."
                    .to_string(),
            settings: vec![
                ProvisioningSetting::new("mode", input.mail_mode),
                ProvisioningSetting::new("smtp_port", input.smtp_port.to_string()),
                ProvisioningSetting::new("inbound_mail", "25/465/587 inbound는 열지 않음"),
                ProvisioningSetting::new(
                    "dns_note",
                    "SPF/DKIM/DMARC/PTR은 발송 방식에 따라 리포트에서 안내",
                ),
            ],
        });
    }

    sections.push(ProvisioningSection {
        name: "security-baseline",
        title: "사이트 보안 기본값",
        summary: format!(
            "{} 보안 수준과 {} SSH 정책 기준으로 변경 전 점검, 적용, 검증을 나눕니다.",
            input.security_profile, input.ssh_policy
        ),
        settings: vec![
            ProvisioningSetting::new(
                "file_ownership",
                "웹파일은 사이트 계정 소유, 쓰기 디렉터리만 제한적으로 허용",
            ),
            ProvisioningSetting::new(
                "fail2ban",
                "SSH jail 상태 점검 후 standard/hardened 정책에서 적용 후보",
            ),
            ProvisioningSetting::new(
                "ssh",
                "audit-only는 리포트만, harden은 현재 세션 보존 후 적용",
            ),
            ProvisioningSetting::new(
                "config_preserve",
                "기존 설정은 백업 후 installer-owned 범위만 변경",
            ),
        ],
    });

    sections
}

fn runtime_label(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "Apache"
    } else {
        "Nginx"
    }
}

fn web_runtime_model(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "Apache mpm_event/worker + proxy_fcgi + PHP-FPM"
    } else {
        "Nginx event worker + FastCGI PHP-FPM"
    }
}

fn database_label(database_engine: &str) -> &'static str {
    if database_engine == "mariadb" {
        "MariaDB"
    } else {
        "MySQL"
    }
}

fn server_names(domain: &str, www_mode: &str) -> String {
    match www_mode {
        "redirect-to-www" => format!("www.{domain}"),
        "include" => format!("{domain} www.{domain}"),
        "none" => domain.to_string(),
        _ => domain.to_string(),
    }
}

fn redirect_source(domain: &str, www_mode: &str) -> String {
    match www_mode {
        "redirect-to-root" if !domain.starts_with("www.") => format!("www.{domain} -> {domain}"),
        "redirect-to-www" if !domain.starts_with("www.") => format!("{domain} -> www.{domain}"),
        _ => "none".to_string(),
    }
}

fn rewrite_policy(app_profile: &str) -> &'static str {
    match app_profile {
        "wordpress" => "WordPress permalink rewrite to /index.php",
        "laravel" => "Laravel public/ front controller rewrite",
        _ => "Gnuboard public/ front controller and PHP path handling",
    }
}

fn php_version_at_least(selected: &str, minimum: &str) -> bool {
    let selected = php_version_tuple(selected);
    let minimum = php_version_tuple(minimum);

    selected >= minimum
}

fn php_version_tuple(version: &str) -> (u16, u16) {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_default();
    let minor = parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_default();

    (major, minor)
}

fn stop_conditions(web_server: &str, web_root: &str, local_test: bool) -> Vec<PlanStopCondition> {
    let other_web_server = if web_server == "nginx" {
        "Apache is running."
    } else {
        "Nginx is running."
    };
    let selected_web_config = if web_server == "nginx" {
        "Nginx site configs already exist."
    } else {
        "Apache site configs already exist."
    };

    let port_stop_condition = if local_test {
        "TCP port 80 is already in use."
    } else {
        "TCP port 80 or 443 is already in use."
    };

    let mut conditions = vec![
        PlanStopCondition::new(other_web_server),
        PlanStopCondition::new(selected_web_config),
        PlanStopCondition::new(port_stop_condition),
        PlanStopCondition::new(format!(
            "{web_root} exists but is not empty or not owned by the selected site account."
        )),
        PlanStopCondition::new("/var/www/g7 legacy test root exists without installer ownership."),
        PlanStopCondition::new("G7-related paths exist without owned-files metadata."),
        PlanStopCondition::new("A previous installer state exists for another install."),
        PlanStopCondition::new("Selected SMTP outbound port cannot be reached."),
        PlanStopCondition::new("Redis is configured to bind publicly."),
        PlanStopCondition::new("Database is reachable from a non-local interface."),
        PlanStopCondition::new("SSH hardening would risk locking out the active session."),
    ];

    if !local_test {
        conditions.push(PlanStopCondition::new(
            "Domain A/AAAA records do not match this VPS public IP.",
        ));
        conditions.push(PlanStopCondition::new(
            "Requested www host does not resolve to this VPS public IP.",
        ));
        conditions.push(PlanStopCondition::new(
            "Existing Let's Encrypt certificate conflicts with installer ownership.",
        ));
    }

    conditions
}

fn web_server_package(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "apache2"
    } else {
        "nginx"
    }
}

fn web_server_service(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "apache2"
    } else {
        "nginx"
    }
}

fn certbot_web_plugin_package(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "python3-certbot-apache"
    } else {
        "python3-certbot-nginx"
    }
}

fn database_package(database_engine: &str) -> &'static str {
    if database_engine == "mysql" {
        "mysql-server"
    } else {
        "mariadb-server"
    }
}

fn database_service(database_engine: &str) -> &'static str {
    if database_engine == "mysql" {
        "mysql"
    } else {
        "mariadb"
    }
}

fn web_server_available_file(web_server: &str) -> PlanFile {
    if web_server == "apache" {
        PlanFile::new("/etc/apache2/sites-available/g7.conf", "create")
    } else {
        PlanFile::new("/etc/nginx/sites-available/g7.conf", "create")
    }
}

fn web_server_enabled_file(web_server: &str) -> PlanFile {
    if web_server == "apache" {
        PlanFile::new("/etc/apache2/sites-enabled/g7.conf", "create symlink")
    } else {
        PlanFile::new("/etc/nginx/sites-enabled/g7.conf", "create symlink")
    }
}

fn app_config_file(app_profile: &crate::app_profile::AppProfile, web_root: &str) -> String {
    if app_profile.id == "wordpress" {
        format!("{web_root}/wp-config.php")
    } else {
        format!("{web_root}/.env")
    }
}

fn smtp_port_for_plan(mail_mode: &str, port: u16) -> Option<u16> {
    if mail_mode == "none" {
        None
    } else {
        Some(port)
    }
}

fn smtp_port_for_mode(mail_mode: &str, port: u16) -> u16 {
    if mail_mode == "local-postfix" && port == DEFAULT_SMTP_PORT {
        25
    } else {
        port
    }
}

fn smtp_encryption_for_plan(mail_mode: &str, encryption: String) -> Option<String> {
    if mail_mode == "none" {
        None
    } else {
        Some(encryption)
    }
}

fn normalize_php_version(version: String) -> Result<String> {
    let version = version.trim().to_string();

    if SUPPORTED_FPM_VERSIONS.contains(&version.as_str()) {
        Ok(version)
    } else {
        Err(Error::InvalidPhpVersion {
            version,
            supported: SUPPORTED_FPM_VERSIONS.join(", "),
        })
    }
}

fn normalize_php_source(php_version: &str, source: String) -> Result<String> {
    let source = normalize_supported_option("php-source", source, &SUPPORTED_PHP_SOURCES)?;
    let source = if source == PHP_SOURCE_AUTO {
        if php_version == DEFAULT_PHP_VERSION {
            PHP_SOURCE_UBUNTU
        } else {
            PHP_SOURCE_ONDREJ
        }
    } else {
        source.as_str()
    };

    if source == PHP_SOURCE_UBUNTU && php_version != DEFAULT_PHP_VERSION {
        return Err(Error::InvalidOption {
            field: "php-source",
            value: format!("{source}+php{php_version}"),
            supported: format!(
                "Ubuntu 24.04 기본 apt는 PHP {DEFAULT_PHP_VERSION} 기준입니다. PHP {php_version}은 php-source=ondrej로 Ondrej PHP PPA를 추가해야 합니다."
            ),
        });
    }

    Ok(source.to_string())
}

fn normalize_site_user(site_user: String) -> Result<String> {
    let site_user = site_user.trim().to_string();

    if site_user.is_empty() {
        return Err(Error::MissingInput { field: "site-user" });
    }

    let valid = site_user
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        && !site_user.starts_with('-');

    if valid {
        Ok(site_user)
    } else {
        Err(Error::InvalidOption {
            field: "site-user",
            value: site_user,
            supported: "Linux account name using letters, digits, underscore, or dash".to_string(),
        })
    }
}

fn validate_site_user_password(password: Option<&str>) -> Result<()> {
    let Some(password) = password else {
        return Ok(());
    };

    if password.len() < 8 {
        return Err(Error::InvalidOption {
            field: "site-password",
            value: "<redacted>".to_string(),
            supported: "at least 8 characters".to_string(),
        });
    }

    let unsupported = password
        .chars()
        .any(|ch| ch == ':' || ch == '\n' || ch == '\r' || ch.is_control());
    if unsupported {
        return Err(Error::InvalidOption {
            field: "site-password",
            value: "<redacted>".to_string(),
            supported: "no colon, newline, or control characters".to_string(),
        });
    }

    Ok(())
}

fn normalize_web_root_mode(mode: String, custom_web_root: &Option<String>) -> Result<String> {
    let mode = if custom_web_root.is_some() && mode == DEFAULT_WEB_ROOT_MODE {
        "custom".to_string()
    } else {
        mode
    };

    normalize_supported_option("web-root-mode", mode, &SUPPORTED_WEB_ROOT_MODES)
}

fn web_root_for(
    domain: &str,
    site_user: &str,
    mode: &str,
    custom_web_root: Option<&str>,
) -> Result<String> {
    match mode {
        "public-html" => Ok(format!("/home/{site_user}/public_html")),
        "www" => Ok(format!("/home/{site_user}/www")),
        "system" => Ok(format!("/var/www/{domain}")),
        "custom" => match custom_web_root {
            Some(path) => normalize_custom_web_root(path),
            None => Err(Error::MissingInput { field: "web-root" }),
        },
        _ => Err(Error::InvalidOption {
            field: "web-root-mode",
            value: mode.to_string(),
            supported: SUPPORTED_WEB_ROOT_MODES.join(", "),
        }),
    }
}

fn normalize_custom_web_root(path: &str) -> Result<String> {
    let path = path.trim().trim_end_matches('/').to_string();

    if path.is_empty() {
        return Err(Error::MissingInput { field: "web-root" });
    }

    if !path.starts_with('/')
        || path == "/"
        || path.contains('\n')
        || path.contains('\r')
        || path.contains('"')
        || path.split('/').any(|segment| segment == "..")
    {
        return Err(Error::InvalidOption {
            field: "web-root",
            value: path,
            supported: "absolute path without quotes, newlines, or parent traversal".to_string(),
        });
    }

    Ok(path)
}

fn database_name_for_domain(domain: &str, app_profile: &str) -> String {
    let mut name = format!("{}_", database_prefix(app_profile));
    for ch in domain.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch);
        } else {
            name.push('_');
        }
    }
    name.truncate(48);
    name.trim_end_matches('_').to_string()
}

fn database_user_for_site_user(site_user: &str, app_profile: &str) -> String {
    let prefix = database_prefix(app_profile);
    let mut user = if site_user == DEFAULT_SITE_USER {
        format!("{prefix}_app")
    } else {
        format!("{prefix}_{site_user}")
    };
    user.truncate(32);
    user
}

fn database_prefix(app_profile: &str) -> &'static str {
    match app_profile {
        "wordpress" => "wp",
        "laravel" => "laravel",
        _ => "g7",
    }
}

fn normalize_supported_option(
    field: &'static str,
    value: String,
    supported: &[&str],
) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();

    if supported.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(Error::InvalidOption {
            field,
            value,
            supported: supported.join(", "),
        })
    }
}

fn validate_mail_options(
    mail_mode: &str,
    smtp_host: Option<&str>,
    smtp_from: Option<&str>,
) -> Result<()> {
    if mail_mode != "smtp-relay" {
        return Ok(());
    }

    if optional_trimmed_is_empty(smtp_host) {
        return Err(Error::MissingInput { field: "smtp-host" });
    }

    if optional_trimmed_is_empty(smtp_from) {
        return Err(Error::MissingInput { field: "smtp-from" });
    }

    if let Some(host) = smtp_host {
        validate_config_safe_value("smtp-host", host)?;
    }

    if let Some(from) = smtp_from {
        validate_config_safe_value("smtp-from", from)?;
    }

    Ok(())
}

fn optional_trimmed_is_empty(value: Option<&str>) -> bool {
    match value {
        Some(value) => value.trim().is_empty(),
        None => true,
    }
}

fn validate_config_safe_value(field: &'static str, value: &str) -> Result<()> {
    if value.contains('"') || value.contains('\n') || value.contains('\r') {
        return Err(Error::InvalidOption {
            field,
            value: value.to_string(),
            supported: "plain value without quotes or newlines".to_string(),
        });
    }

    Ok(())
}

fn normalize_domain(domain: String) -> Result<String> {
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();

    if domain.is_empty() {
        return Err(Error::MissingInput { field: "domain" });
    }

    if domain.contains('/') || domain.contains(':') || domain.chars().any(char::is_whitespace) {
        return Err(Error::InvalidDomain { domain });
    }

    if domain.len() > 253 || !domain.contains('.') {
        return Err(Error::InvalidDomain { domain });
    }

    if !domain
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '.')
    {
        return Err(Error::InvalidDomain { domain });
    }

    if domain.split('.').any(|label| {
        label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
    }) {
        return Err(Error::InvalidDomain { domain });
    }

    Ok(domain)
}

#[cfg(test)]
mod tests {
    use super::{PlanOptions, build, build_with_options};
    use crate::Error;

    #[test]
    fn plan_normalizes_domain() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build(" Example.COM. ".to_string())?;

        assert_eq!(plan.domain, "example.com");
        assert_eq!(plan.deployment_mode, "public");
        assert_eq!(plan.web_server, "nginx");
        assert_eq!(plan.php_version, "8.3");
        assert_eq!(plan.database_engine, "mysql");
        assert_eq!(plan.site_user, "g7");
        assert_eq!(plan.web_root_mode, "public-html");
        assert_eq!(plan.web_root, "/home/g7/public_html");
        assert_eq!(plan.security_profile, "standard");
        assert_eq!(plan.ssh_policy, "audit-only");
        assert_eq!(plan.www_mode, "redirect-to-root");
        assert_eq!(plan.redis_mode, "enable");
        assert_eq!(plan.mail_mode, "none");
        assert_eq!(plan.mode, "dry-run");
        assert!(!plan.changes_made);
        Ok(())
    }

    #[test]
    fn plan_describes_install_contract() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build("example.com".to_string())?;

        assert!(plan.fresh_server_only);
        assert!(plan.packages.iter().any(|package| package.name == "nginx"));
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "redis-server")
        );
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "certbot.timer")
        );
        assert!(
            plan.files
                .iter()
                .any(|file| file.path == "/home/g7/public_html")
        );
        assert!(plan.services.iter().any(|service| service.name == "nginx"));
        assert!(plan.services.iter().any(|service| service.name == "mysql"));
        assert!(plan.ports.iter().any(|port| port.port == 443));
        assert!(plan.ports.iter().any(|port| port.port == 3306));
        assert!(plan.ports.iter().any(|port| port.port == 6379));
        assert!(
            plan.security_checks
                .iter()
                .any(|check| check.name == "database-credentials")
        );
        assert!(
            plan.security_checks
                .iter()
                .any(|check| check.name == "ssh-config" && check.level == "audit")
        );
        assert!(
            plan.stop_conditions
                .iter()
                .any(|condition| condition.reason.contains("public IP"))
        );
        Ok(())
    }

    #[test]
    fn plan_includes_memory_sizing_presets() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let plan = build("example.com".to_string())?;
        let sizing = plan
            .provisioning
            .iter()
            .find(|section| section.name == "server-sizing")
            .expect("server sizing section");
        let php = plan
            .provisioning
            .iter()
            .find(|section| section.name == "php-runtime")
            .expect("php runtime section");
        let web = plan
            .provisioning
            .iter()
            .find(|section| section.name == "web-server")
            .expect("web server section");
        let database = plan
            .provisioning
            .iter()
            .find(|section| section.name == "database")
            .expect("database section");
        let redis = plan
            .provisioning
            .iter()
            .find(|section| section.name == "redis")
            .expect("redis section");

        assert!(sizing.summary.contains("1/2/4/8/16/32GB"));
        assert!(sizing.settings.iter().any(|setting| {
            setting.key == "tier_32gb"
                && setting.value.contains("db_buffer_pool=10G")
                && setting.value.contains("php_max_children=96")
                && setting.value.contains("apache_max_request_workers=400")
                && setting.value.contains("nginx_worker_processes=min(vCPU,8)")
        }));
        assert!(sizing.settings.iter().any(|setting| {
            setting.key == "tier_gt32gb"
                && setting.value.contains("db_buffer_pool=min(RAM*40%, 24G)")
                && setting.value.contains("redis_maxmemory=min(RAM*6%, 4G)")
                && setting
                    .value
                    .contains("apache_max_request_workers=min(vCPU*64, 800 per site)")
        }));
        assert!(web.settings.iter().any(|setting| {
            setting.key == "nginx_worker_processes_by_cpu_ram"
                && setting.value.contains("32GB=min(vCPU,8)")
                && setting.value.contains(">32GB=min(vCPU,16)")
        }));
        assert!(web.settings.iter().any(|setting| {
            setting.key == "apache_max_request_workers_by_ram"
                && setting.value.contains("16GB=300")
                && setting.value.contains("32GB=400")
                && setting.value.contains(">32GB=min(vCPU*64, 800 per site)")
        }));
        assert!(php.settings.iter().any(|setting| {
            setting.key == "max_children_by_ram"
                && setting.value.contains("32GB=96")
                && setting
                    .value
                    .contains(">32GB=min(floor(php_budget/128M), 192 per site)")
        }));
        assert!(php.settings.iter().any(|setting| {
            setting.key == "cpu_guard_by_ram"
                && setting
                    .value
                    .contains("32GB=min(memory_budget, vCPU*12, 96)")
        }));
        assert!(database.settings.iter().any(|setting| {
            setting.key == "buffer_pool_by_ram"
                && setting.value.contains("16GB=5G")
                && setting.value.contains("32GB=10G")
        }));
        assert!(redis.settings.iter().any(|setting| {
            setting.key == "maxmemory_by_ram"
                && setting.value.contains("32GB=2G")
                && setting.value.contains(">32GB=min(RAM*6%, 4G)")
        }));
        Ok(())
    }

    #[test]
    fn plan_supports_local_test_domain_without_dns_or_certbot()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            local_test: true,
            dns_check: true,
            www_mode: "none".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("g7-test.local".to_string(), options)?;

        assert_eq!(plan.deployment_mode, "local-test");
        assert!(!plan.dns_check_required);
        assert!(
            !plan
                .packages
                .iter()
                .any(|package| package.name == "certbot")
        );
        assert!(
            !plan
                .services
                .iter()
                .any(|service| service.name == "certbot.timer")
        );
        assert!(!plan.ports.iter().any(|port| port.port == 443));
        assert!(
            plan.files
                .iter()
                .any(|file| file.path == "/etc/g7-installer/local-hosts.txt")
        );
        assert!(
            !plan
                .stop_conditions
                .iter()
                .any(|condition| condition.reason.contains("public IP"))
        );
        Ok(())
    }

    #[test]
    fn plan_supports_apache_and_mysql_choices()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            web_server: "apache".to_string(),
            database_engine: "mysql".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.web_server, "apache");
        assert_eq!(plan.database_engine, "mysql");
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "apache2")
        );
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "mysql-server")
        );
        assert!(
            plan.files
                .iter()
                .any(|file| file.path == "/etc/apache2/sites-available/g7.conf")
        );
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "apache2")
        );
        assert!(plan.services.iter().any(|service| service.name == "mysql"));
        Ok(())
    }

    #[test]
    fn plan_allows_php_85_next_option() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            php_version: "8.5".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.php_version, "8.5");
        assert_eq!(plan.php_source, "ondrej");
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "php8.5-fpm")
        );
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name.contains("software-properties-common"))
        );
        assert!(
            plan.packages
                .iter()
                .all(|package| !package.name.contains("php8.5-opcache"))
        );
        Ok(())
    }

    #[test]
    fn plan_rejects_php_85_with_ubuntu_source()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            php_version: "8.5".to_string(),
            php_source: "ubuntu".to_string(),
            ..PlanOptions::default()
        };

        let err = match build_with_options("example.com".to_string(), options) {
            Ok(_) => return Err(std::io::Error::other("ubuntu php source should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            Error::InvalidOption {
                field: "php-source",
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn plan_rejects_unsupported_php_version() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let options = PlanOptions {
            php_version: "8.4".to_string(),
            ..PlanOptions::default()
        };

        let err = match build_with_options("example.com".to_string(), options) {
            Ok(_) => return Err(std::io::Error::other("unsupported PHP should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::InvalidPhpVersion { .. }));
        Ok(())
    }

    #[test]
    fn plan_requires_smtp_relay_details() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            mail_mode: "smtp-relay".to_string(),
            ..PlanOptions::default()
        };

        let err = match build_with_options("example.com".to_string(), options) {
            Ok(_) => return Err(std::io::Error::other("smtp host should be required").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::MissingInput { field: "smtp-host" }));
        Ok(())
    }

    #[test]
    fn plan_accepts_smtp_relay_details() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            mail_mode: "smtp-relay".to_string(),
            smtp_host: Some("smtp.example.com".to_string()),
            smtp_from: Some("no-reply@example.com".to_string()),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.mail_mode, "smtp-relay");
        assert_eq!(plan.smtp_port, Some(587));
        assert!(plan.ports.iter().any(|port| port.port == 587));
        Ok(())
    }

    #[test]
    fn plan_rejects_empty_domain() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let err = match build(" ".to_string()) {
            Ok(_) => return Err(std::io::Error::other("empty domain should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::MissingInput { field: "domain" }));
        Ok(())
    }

    #[test]
    fn plan_rejects_url_like_domain() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let err = match build("https://example.com".to_string()) {
            Ok(_) => return Err(std::io::Error::other("URL should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::InvalidDomain { .. }));
        Ok(())
    }

    #[test]
    fn plan_rejects_invalid_domain_labels() -> std::result::Result<(), Box<dyn std::error::Error>> {
        for domain in ["example", "-example.com", "example-.com", "exa_mple.com"] {
            let err = match build(domain.to_string()) {
                Ok(_) => {
                    return Err(std::io::Error::other("invalid domain should fail").into());
                }
                Err(err) => err,
            };

            assert!(matches!(err, Error::InvalidDomain { .. }));
        }
        Ok(())
    }
}
