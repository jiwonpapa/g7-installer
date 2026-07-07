//! `g7inst` command-line surface.
//!
//! CLI flags and output are adapters over the canonical policy in
//! `g7_core::commands::plan`. Do not add CLI-only defaults here; every server
//! installation default must live in `plan.rs` first.

use clap::{Parser, Subcommand};
use g7_core::commands::{
    DoctorCheckStatus, doctor, install, logs, plan, reset, rollback, self_update, status, update,
};
use miette::Result;

mod web_setup;

#[derive(Debug, Parser)]
#[command(name = "g7inst")]
#[command(version)]
#[command(about = "G7 Installer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the web setup controller.
    Setup {
        /// Domain that will serve the G7 site.
        #[arg(long)]
        domain: Option<String>,
        /// Use local test mode without public DNS or Let's Encrypt.
        #[arg(long, default_value_t = false)]
        local_test: bool,
        /// Web controller bind address.
        #[arg(long, default_value = web_setup::DEFAULT_BIND)]
        bind: String,
        /// Allow binding the web controller to a non-loopback address.
        #[arg(long, default_value_t = false)]
        allow_remote: bool,
    },
    /// Diagnose whether this server can be used for a G7 install.
    Doctor,
    /// Show the installation plan before making changes.
    Plan {
        /// Domain that will serve the G7 site.
        #[arg(long)]
        domain: String,
        /// Use local test mode without public DNS or Let's Encrypt.
        #[arg(long, default_value_t = false)]
        local_test: bool,
        /// App profile: gnuboard7, wordpress, or laravel.
        #[arg(long = "app", visible_alias = "app-package", default_value_t = plan::DEFAULT_APP_PROFILE.to_string())]
        app_profile: String,
        /// Web server: nginx or apache.
        #[arg(long, default_value_t = plan::DEFAULT_WEB_SERVER.to_string())]
        web_server: String,
        /// PHP-FPM version. Default is 8.3. Use 8.5 only when available from apt sources.
        #[arg(long, default_value_t = plan::DEFAULT_PHP_VERSION.to_string())]
        php_version: String,
        /// Database engine: mysql or mariadb.
        #[arg(long, default_value_t = plan::DEFAULT_DATABASE_ENGINE.to_string())]
        database: String,
        /// Linux account that owns the G7 site files.
        #[arg(long, default_value_t = plan::DEFAULT_SITE_USER.to_string())]
        site_user: String,
        /// Web root mode: public-html, www, system, or custom.
        #[arg(long, default_value_t = plan::DEFAULT_WEB_ROOT_MODE.to_string())]
        web_root_mode: String,
        /// Custom absolute web root. Implies --web-root-mode custom.
        #[arg(long)]
        web_root: Option<String>,
        /// Canonical host policy: redirect-to-root, redirect-to-www, include, none.
        #[arg(long, default_value_t = plan::DEFAULT_WWW_MODE.to_string())]
        www_mode: String,
        /// Redis mode: enable or disable.
        #[arg(long, default_value_t = plan::DEFAULT_REDIS_MODE.to_string())]
        redis: String,
        /// Mail mode: none, smtp-relay, local-postfix.
        #[arg(long, default_value_t = plan::DEFAULT_MAIL_MODE.to_string())]
        mail_mode: String,
        /// SMTP relay host. Required when --mail-mode smtp-relay.
        #[arg(long)]
        smtp_host: Option<String>,
        /// SMTP relay port.
        #[arg(long, default_value_t = plan::DEFAULT_SMTP_PORT)]
        smtp_port: u16,
        /// SMTP sender address. Required when --mail-mode smtp-relay.
        #[arg(long)]
        smtp_from: Option<String>,
        /// SMTP encryption: none, starttls, tls.
        #[arg(long, default_value_t = plan::DEFAULT_SMTP_ENCRYPTION.to_string())]
        smtp_encryption: String,
        /// Security profile: audit-only, standard, or hardened.
        #[arg(long, default_value_t = plan::DEFAULT_SECURITY_PROFILE.to_string())]
        security_profile: String,
        /// SSH policy: audit-only or harden.
        #[arg(long, default_value_t = plan::DEFAULT_SSH_POLICY.to_string())]
        ssh_policy: String,
        /// Enable rollback tracking.
        #[arg(long, default_value_t = true)]
        rollback: bool,
        /// Preserve existing configuration instead of overwriting unowned files.
        #[arg(long, default_value_t = true)]
        preserve_config: bool,
        /// Require domain DNS to match this VPS public IP.
        #[arg(long, default_value_t = true)]
        dns_check: bool,
    },
    /// Install G7 on a fresh Ubuntu VPS.
    Install {
        /// Domain that will serve the G7 site.
        #[arg(long)]
        domain: String,
        /// Use local test mode without public DNS or Let's Encrypt.
        #[arg(long, default_value_t = false)]
        local_test: bool,
        /// App profile: gnuboard7, wordpress, or laravel.
        #[arg(long = "app", visible_alias = "app-package", default_value_t = plan::DEFAULT_APP_PROFILE.to_string())]
        app_profile: String,
        /// Web server: nginx or apache.
        #[arg(long, default_value_t = plan::DEFAULT_WEB_SERVER.to_string())]
        web_server: String,
        /// PHP-FPM version. Default is 8.3. Use 8.5 only when available from apt sources.
        #[arg(long, default_value_t = plan::DEFAULT_PHP_VERSION.to_string())]
        php_version: String,
        /// Database engine: mysql or mariadb.
        #[arg(long, default_value_t = plan::DEFAULT_DATABASE_ENGINE.to_string())]
        database: String,
        /// Linux account that owns the G7 site files.
        #[arg(long, default_value_t = plan::DEFAULT_SITE_USER.to_string())]
        site_user: String,
        /// Web root mode: public-html, www, system, or custom.
        #[arg(long, default_value_t = plan::DEFAULT_WEB_ROOT_MODE.to_string())]
        web_root_mode: String,
        /// Custom absolute web root. Implies --web-root-mode custom.
        #[arg(long)]
        web_root: Option<String>,
        /// Canonical host policy: redirect-to-root, redirect-to-www, include, none.
        #[arg(long, default_value_t = plan::DEFAULT_WWW_MODE.to_string())]
        www_mode: String,
        /// Redis mode: enable or disable.
        #[arg(long, default_value_t = plan::DEFAULT_REDIS_MODE.to_string())]
        redis: String,
        /// Mail mode: none, smtp-relay, local-postfix.
        #[arg(long, default_value_t = plan::DEFAULT_MAIL_MODE.to_string())]
        mail_mode: String,
        /// SMTP relay host. Required when --mail-mode smtp-relay.
        #[arg(long)]
        smtp_host: Option<String>,
        /// SMTP relay port.
        #[arg(long, default_value_t = plan::DEFAULT_SMTP_PORT)]
        smtp_port: u16,
        /// SMTP sender address. Required when --mail-mode smtp-relay.
        #[arg(long)]
        smtp_from: Option<String>,
        /// SMTP encryption: none, starttls, tls.
        #[arg(long, default_value_t = plan::DEFAULT_SMTP_ENCRYPTION.to_string())]
        smtp_encryption: String,
        /// Security profile: audit-only, standard, or hardened.
        #[arg(long, default_value_t = plan::DEFAULT_SECURITY_PROFILE.to_string())]
        security_profile: String,
        /// SSH policy: audit-only or harden.
        #[arg(long, default_value_t = plan::DEFAULT_SSH_POLICY.to_string())]
        ssh_policy: String,
        /// Enable rollback tracking.
        #[arg(long, default_value_t = true)]
        rollback: bool,
        /// Preserve existing configuration instead of overwriting unowned files.
        #[arg(long, default_value_t = true)]
        preserve_config: bool,
        /// Require domain DNS to match this VPS public IP.
        #[arg(long, default_value_t = true)]
        dns_check: bool,
    },
    /// Show installed G7 and service status.
    Status,
    /// Show installer log location.
    Logs,
    /// Remove installer-owned files and legacy g7 binary for test VM reset.
    Reset {
        /// Confirm removal of installer-owned files.
        #[arg(long, default_value_t = false)]
        yes: bool,
        /// Preview paths without removing files.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Roll back package install before app/site content is created.
    Rollback {
        /// Confirm service disable, apt purge, and metadata reset.
        #[arg(long, default_value_t = false)]
        yes: bool,
        /// Preview services, packages, and metadata without changing them.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Update the installed G7 application.
    Update,
    /// Update the installer binary.
    SelfUpdate,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Setup {
            domain,
            local_test,
            bind,
            allow_remote,
        } => {
            web_setup::run(web_setup::WebSetupConfig {
                domain,
                local_test,
                bind,
                allow_remote,
            })
            .await?;
        }
        Command::Doctor => print_doctor(doctor::run()),
        Command::Plan {
            domain,
            local_test,
            app_profile,
            web_server,
            php_version,
            database,
            site_user,
            web_root_mode,
            web_root,
            www_mode,
            redis,
            mail_mode,
            smtp_host,
            smtp_port,
            smtp_from,
            smtp_encryption,
            security_profile,
            ssh_policy,
            rollback,
            preserve_config,
            dns_check,
        } => print_plan(
            plan::build_with_options(
                domain,
                plan_options(
                    local_test,
                    app_profile,
                    web_server,
                    php_version,
                    database,
                    site_user,
                    web_root_mode,
                    web_root,
                    www_mode,
                    redis,
                    mail_mode,
                    smtp_host,
                    smtp_port,
                    smtp_from,
                    smtp_encryption,
                    security_profile,
                    ssh_policy,
                    rollback,
                    preserve_config,
                    dns_check,
                ),
            )
            .map_err(miette::Report::new)?,
        ),
        Command::Install {
            domain,
            local_test,
            app_profile,
            web_server,
            php_version,
            database,
            site_user,
            web_root_mode,
            web_root,
            www_mode,
            redis,
            mail_mode,
            smtp_host,
            smtp_port,
            smtp_from,
            smtp_encryption,
            security_profile,
            ssh_policy,
            rollback,
            preserve_config,
            dns_check,
        } => {
            print_install(
                install::run(
                    domain,
                    plan_options(
                        local_test,
                        app_profile,
                        web_server,
                        php_version,
                        database,
                        site_user,
                        web_root_mode,
                        web_root,
                        www_mode,
                        redis,
                        mail_mode,
                        smtp_host,
                        smtp_port,
                        smtp_from,
                        smtp_encryption,
                        security_profile,
                        ssh_policy,
                        rollback,
                        preserve_config,
                        dns_check,
                    ),
                )
                .map_err(miette::Report::new)?,
            );
        }
        Command::Status => print_status(status::read()),
        Command::Logs => print_logs(logs::location()),
        Command::Reset { yes, dry_run } => {
            print_reset(reset::run(yes, dry_run).map_err(miette::Report::new)?);
        }
        Command::Rollback { yes, dry_run } => {
            print_rollback(rollback::run(yes, dry_run).map_err(miette::Report::new)?);
        }
        Command::Update => {
            update::run().map_err(miette::Report::new)?;
        }
        Command::SelfUpdate => {
            self_update::run().map_err(miette::Report::new)?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_options(
    local_test: bool,
    app_profile: String,
    web_server: String,
    php_version: String,
    database: String,
    site_user: String,
    web_root_mode: String,
    web_root: Option<String>,
    www_mode: String,
    redis: String,
    mail_mode: String,
    smtp_host: Option<String>,
    smtp_port: u16,
    smtp_from: Option<String>,
    smtp_encryption: String,
    security_profile: String,
    ssh_policy: String,
    rollback: bool,
    preserve_config: bool,
    dns_check: bool,
) -> plan::PlanOptions {
    plan::PlanOptions {
        local_test,
        app_profile,
        web_server,
        php_version,
        database_engine: database,
        site_user,
        web_root_mode,
        custom_web_root: web_root,
        www_mode,
        redis_mode: redis,
        mail_mode,
        smtp_host,
        smtp_port,
        smtp_from,
        smtp_encryption,
        security_profile,
        ssh_policy,
        rollback,
        preserve_config,
        dns_check,
    }
}

fn print_doctor(report: doctor::DoctorReport) {
    println!("G7 Installer Doctor");
    println!("install_allowed: {}", report.install_allowed);
    println!();

    for check in report.checks {
        println!(
            "[{}] {} - {}",
            check_status_label(check.status),
            check.name,
            check.message
        );
    }
}

fn print_plan(plan: plan::InstallPlan) {
    print!("{}", format_plan(&plan));
}

pub(crate) fn format_plan(plan: &plan::InstallPlan) -> String {
    let mut output = String::new();

    output.push_str("G7 Installer Plan\n");
    output.push_str(&format!("domain: {}\n", plan.domain));
    output.push_str(&format!("deployment_mode: {}\n", plan.deployment_mode));
    output.push_str(&format!("app_profile: {}\n", plan.app_profile));
    output.push_str(&format!("app_profile_label: {}\n", plan.app_profile_label));
    output.push_str(&format!("app_document_root: {}\n", plan.app_document_root));
    output.push_str(&format!("web_server: {}\n", plan.web_server));
    output.push_str(&format!("php_version: {}\n", plan.php_version));
    output.push_str(&format!("database: {}\n", plan.database_engine));
    output.push_str(&format!("site_user: {}\n", plan.site_user));
    output.push_str(&format!("web_root_mode: {}\n", plan.web_root_mode));
    output.push_str(&format!("web_root: {}\n", plan.web_root));
    output.push_str(&format!("database_name: {}\n", plan.database_name));
    output.push_str(&format!("database_user: {}\n", plan.database_user));
    output.push_str(&format!(
        "database_password_policy: {}\n",
        plan.database_password_policy
    ));
    output.push_str(&format!("www_mode: {}\n", plan.www_mode));
    output.push_str(&format!("redis: {}\n", plan.redis_mode));
    output.push_str(&format!("mail_mode: {}\n", plan.mail_mode));
    if let Some(host) = &plan.smtp_host {
        output.push_str(&format!("smtp_host: {host}\n"));
    }
    if let Some(port) = plan.smtp_port {
        output.push_str(&format!("smtp_port: {port}\n"));
    }
    if let Some(from) = &plan.smtp_from {
        output.push_str(&format!("smtp_from: {from}\n"));
    }
    if let Some(encryption) = &plan.smtp_encryption {
        output.push_str(&format!("smtp_encryption: {encryption}\n"));
    }
    output.push_str(&format!("rollback: {}\n", plan.rollback_enabled));
    output.push_str(&format!("preserve_config: {}\n", plan.preserve_config));
    output.push_str(&format!("dns_check: {}\n", plan.dns_check_required));
    output.push_str(&format!("security_profile: {}\n", plan.security_profile));
    output.push_str(&format!("ssh_policy: {}\n", plan.ssh_policy));
    output.push_str(&format!("mode: {}\n", plan.mode));
    output.push_str(&format!("fresh_server_only: {}\n", plan.fresh_server_only));
    output.push_str(&format!("changes_made: {}\n\n", plan.changes_made));

    output.push_str("Preflight gates:\n");
    for gate in &plan.preflight_gates {
        output.push_str(&format!("- {}: {}\n", gate.name, gate.description));
    }

    output.push_str("\nPackages:\n");
    for package in &plan.packages {
        output.push_str(&format!("- {}: {}\n", package.name, package.description));
    }

    output.push_str("\nFiles:\n");
    for file in &plan.files {
        output.push_str(&format!("- {} ({})\n", file.path, file.action));
    }

    output.push_str("\nServices:\n");
    for service in &plan.services {
        output.push_str(&format!("- {} ({})\n", service.name, service.action));
    }

    output.push_str("\nPorts:\n");
    for port in &plan.ports {
        output.push_str(&format!(
            "- {}/{}: {}\n",
            port.port, port.protocol, port.purpose
        ));
    }

    output.push_str("\nSecurity checks:\n");
    for check in &plan.security_checks {
        output.push_str(&format!(
            "- {} [{}]: {}\n",
            check.name, check.level, check.description
        ));
    }

    output.push_str("\nApp requirements:\n");
    for requirement in &plan.app_requirements {
        output.push_str(&format!(
            "- [{}] {}: {}\n",
            requirement.status, requirement.name, requirement.message
        ));
    }

    output.push_str("\nApp follow-up steps:\n");
    for step in &plan.app_followup_steps {
        output.push_str(&format!("- {}: {}\n", step.name, step.description));
    }

    output.push_str("\nInstall stops if:\n");
    for condition in &plan.stop_conditions {
        output.push_str(&format!("- {}\n", condition.reason));
    }

    output
}

fn print_status(status: status::InstallerStatus) {
    println!("G7 Installer Status");
    println!("installed: {}", status.installed);

    for component in status.components {
        println!("- {}: {}", component.name, component.state);
    }
}

fn print_install(report: install::InstallReport) {
    println!("G7 Installer Install");
    println!("domain: {}", report.domain);
    println!("deployment_mode: {}", report.deployment_mode);
    println!("app_profile: {}", report.app_profile);
    println!("app_profile_label: {}", report.app_profile_label);
    println!("app_document_root: {}", report.app_document_root);
    println!("web_server: {}", report.web_server);
    println!("php_version: {}", report.php_version);
    println!("database: {}", report.database_engine);
    println!("site_user: {}", report.site_user);
    println!("web_root_mode: {}", report.web_root_mode);
    println!("web_root: {}", report.web_root);
    println!("www_mode: {}", report.www_mode);
    println!("redis: {}", report.redis_mode);
    println!("mail_mode: {}", report.mail_mode);
    if let Some(host) = &report.smtp_host {
        println!("smtp_host: {host}");
    }
    if let Some(port) = report.smtp_port {
        println!("smtp_port: {port}");
    }
    if let Some(from) = &report.smtp_from {
        println!("smtp_from: {from}");
    }
    if let Some(encryption) = &report.smtp_encryption {
        println!("smtp_encryption: {encryption}");
    }
    println!("dns_check: {}", report.dns_check);
    println!("security_profile: {}", report.security_profile);
    println!("ssh_policy: {}", report.ssh_policy);
    println!("phase: {}", report.phase);
    println!("state: {}", report.state_path.display());
    println!("owned_files: {}", report.owned_files_path.display());
    println!();
    println!("Completed steps:");

    for step in report.completed_steps {
        println!("- {step}");
    }

    print_install_checks("Safety checks", &report.safety_checks);
    print_install_checks(
        "Preinstall package checks",
        &report.preinstall_package_checks,
    );
    print_install_checks("Package checks", &report.package_checks);
    print_install_checks("Service checks", &report.service_checks);
    print_install_checks("Port checks", &report.port_checks);
    print_install_checks("Network checks", &report.network_checks);
    print_install_checks("Vhost checks", &report.vhost_checks);
    print_install_checks("Mail checks", &report.mail_checks);
    print_install_checks("Certbot checks", &report.certbot_checks);
    print_install_checks("App requirements", &report.app_requirements);
}

fn print_install_checks(title: &str, checks: &[install::InstallCheck]) {
    println!();
    println!("{title}:");

    if checks.is_empty() {
        println!("- none");
        return;
    }

    for check in checks {
        println!("- [{}] {} - {}", check.status, check.name, check.message);
    }
}

fn print_logs(location: logs::LogLocation) {
    println!("{}", location.path.display());
}

fn print_reset(report: reset::ResetReport) {
    println!("G7 Installer Reset");
    println!("dry_run: {}", report.dry_run);
    println!("removed:");
    for path in report.removed {
        println!("- {path}");
    }

    if !report.missing.is_empty() {
        println!("missing:");
        for path in report.missing {
            println!("- {path}");
        }
    }
}

fn print_rollback(report: rollback::RollbackReport) {
    println!("G7 Installer Rollback");
    println!("dry_run: {}", report.dry_run);
    println!("phase: {}", report.phase);

    println!();
    println!("Service actions:");
    if report.service_actions.is_empty() {
        println!("- none");
    } else {
        for action in report.service_actions {
            println!("- [{}] {} - {}", action.status, action.name, action.message);
        }
    }

    println!();
    println!("Package actions:");
    if report.package_actions.is_empty() {
        println!("- none");
    } else {
        for action in report.package_actions {
            println!("- [{}] {} - {}", action.status, action.name, action.message);
        }
    }

    println!();
    println!("Metadata reset:");
    println!("dry_run: {}", report.metadata_reset.dry_run);
    println!("removed:");
    for path in report.metadata_reset.removed {
        println!("- {path}");
    }
    if !report.metadata_reset.missing.is_empty() {
        println!("missing:");
        for path in report.metadata_reset.missing {
            println!("- {path}");
        }
    }
}

fn check_status_label(status: DoctorCheckStatus) -> &'static str {
    match status {
        DoctorCheckStatus::Pass => "pass",
        DoctorCheckStatus::Warn => "warn",
        DoctorCheckStatus::Fail => "fail",
        DoctorCheckStatus::Pending => "pending",
    }
}

#[cfg(test)]
mod tests {
    use super::format_plan;
    use g7_core::commands::plan;

    #[test]
    fn plan_output_is_a_dry_run_contract() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = plan::build("example.com".to_string())?;
        let output = format_plan(&plan);

        assert!(output.contains("mode: dry-run"));
        assert!(output.contains("changes_made: false"));
        assert!(output.contains("- nginx: Web server and reverse proxy."));
        assert!(output.contains("site_user: g7"));
        assert!(output.contains("web_root: /home/g7/public_html"));
        assert!(output.contains("database_password_policy: generate-random-store-root-only"));
        assert!(output.contains("- /home/g7/public_html (planned app web root;"));
        assert!(output.contains("- redis-local-only [apply]:"));
        assert!(output.contains("- 443/tcp: Inbound HTTPS traffic."));
        assert!(output.contains("- 3306/tcp: Localhost-only SQL database."));
        assert!(output.contains("deployment_mode: public"));
        assert!(output.contains("web_server: nginx"));
        assert!(output.contains("php_version: 8.3"));
        assert!(output.contains("database: mysql"));
        assert!(output.contains("redis: enable"));
        assert!(output.contains("rollback: true"));
        assert!(output.contains("- Apache is running."));
        Ok(())
    }
}
