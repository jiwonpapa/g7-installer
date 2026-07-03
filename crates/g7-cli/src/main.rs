//! `g7inst` command-line surface.
//!
//! CLI flags and output are adapters over the canonical policy in
//! `g7_core::commands::plan`. Do not add CLI-only defaults here; every server
//! installation default must live in `plan.rs` first.

use clap::{Parser, Subcommand};
use dialoguer::{Confirm, Input, Select};
use g7_core::commands::{
    DoctorCheckStatus, doctor, install, logs, plan, reset, self_update, status, update,
};
use miette::{Result, miette};

mod tui_setup;

const SETUP_CONTROL_HINT: &str =
    "Controls: Up/Down move, Enter select/accept, type text when prompted, Ctrl+C cancel.";
const SELECT_PROMPT_HINT: &str = "Use Up/Down, Enter";

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
    /// Run a guided setup flow.
    Setup {
        /// Domain that will serve the G7 site.
        #[arg(long)]
        domain: Option<String>,
        /// Use local test mode without public DNS or Let's Encrypt.
        #[arg(long, default_value_t = false)]
        local_test: bool,
        /// Use the legacy prompt flow instead of the full-screen TUI.
        #[arg(long, default_value_t = false)]
        plain: bool,
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
        /// Web server: nginx or apache.
        #[arg(long, default_value_t = plan::DEFAULT_WEB_SERVER.to_string())]
        web_server: String,
        /// PHP-FPM version. Default is 8.5. Use 8.3 for compatibility.
        #[arg(long, default_value_t = plan::DEFAULT_PHP_VERSION.to_string())]
        php_version: String,
        /// Database engine: mariadb or mysql.
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
        /// Web server: nginx or apache.
        #[arg(long, default_value_t = plan::DEFAULT_WEB_SERVER.to_string())]
        web_server: String,
        /// PHP-FPM version. Default is 8.5. Use 8.3 for compatibility.
        #[arg(long, default_value_t = plan::DEFAULT_PHP_VERSION.to_string())]
        php_version: String,
        /// Database engine: mariadb or mysql.
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
    /// Update the installed G7 application.
    Update,
    /// Update the installer binary.
    SelfUpdate,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Setup {
            domain,
            local_test,
            plain,
        } => {
            if plain {
                run_setup_plain(domain, local_test)?;
            } else {
                tui_setup::run(domain, local_test)?;
            }
        }
        Command::Doctor => print_doctor(doctor::run()),
        Command::Plan {
            domain,
            local_test,
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

fn run_setup_plain(domain_arg: Option<String>, local_test_arg: bool) -> Result<()> {
    println!("G7 Installer Setup");
    println!("{SETUP_CONTROL_HINT}");
    println!();
    println!("1) Server check");
    let doctor_report = doctor::run();
    print_doctor(doctor_report.clone());
    println!();

    if !doctor_report.install_allowed {
        return Err(miette!(
            "server preflight failed; fix failed checks and run g7inst setup again"
        ));
    }

    println!("2) Setup options");
    let profile_items = [
        "public domain: DNS + Let's Encrypt",
        "local test domain: no public DNS, no Let's Encrypt",
    ];
    let profile = if local_test_arg {
        1
    } else {
        Select::new()
            .with_prompt(select_prompt("Install profile"))
            .items(&profile_items)
            .default(0)
            .interact()
            .map_err(|err| miette!("setup prompt failed: {err}"))?
    };
    let local_test = profile == 1;

    let default_domain = if local_test {
        "g7-test.local"
    } else {
        "example.com"
    };
    let domain = match domain_arg {
        Some(domain) => domain,
        None => Input::<String>::new()
            .with_prompt("Domain")
            .default(default_domain.to_string())
            .interact_text()
            .map_err(|err| miette!("setup prompt failed: {err}"))?,
    };

    let web_server_items = ["nginx", "apache"];
    let web_server = select_value("Web server", &web_server_items, 0)?;

    let php_items = ["8.5", "8.3"];
    let php_version = select_value("PHP-FPM version", &php_items, 0)?;

    let database_items = ["mariadb", "mysql"];
    let database = select_value("Database", &database_items, 0)?;

    let site_user = Input::<String>::new()
        .with_prompt("Site Linux user")
        .default(plan::DEFAULT_SITE_USER.to_string())
        .interact_text()
        .map_err(|err| miette!("setup prompt failed: {err}"))?;

    let web_root_items = ["public-html", "www", "system", "custom"];
    let web_root_mode = select_value("Web root mode", &web_root_items, 0)?;
    let web_root = if web_root_mode == "custom" {
        Some(
            Input::<String>::new()
                .with_prompt("Custom absolute web root")
                .interact_text()
                .map_err(|err| miette!("setup prompt failed: {err}"))?,
        )
    } else {
        None
    };

    let www_items = ["redirect-to-root", "redirect-to-www", "include", "none"];
    let www_default = if local_test { 3 } else { 0 };
    let www_mode = select_value("www policy", &www_items, www_default)?;

    let redis_enabled = Confirm::new()
        .with_prompt("Install Redis for cache/session/queue?")
        .default(true)
        .interact()
        .map_err(|err| miette!("setup prompt failed: {err}"))?;
    let redis = if redis_enabled { "enable" } else { "disable" }.to_string();

    let mail_items = ["none", "smtp-relay", "local-postfix"];
    let mail_mode = select_value("Mail delivery", &mail_items, 0)?;
    let mut smtp_host = None;
    let mut smtp_from = None;
    let mut smtp_port = if mail_mode == "local-postfix" {
        25
    } else {
        plan::DEFAULT_SMTP_PORT
    };
    let mut smtp_encryption = plan::DEFAULT_SMTP_ENCRYPTION.to_string();

    if mail_mode == "smtp-relay" {
        smtp_host = Some(
            Input::<String>::new()
                .with_prompt("SMTP host")
                .interact_text()
                .map_err(|err| miette!("setup prompt failed: {err}"))?,
        );
        smtp_port = Input::<u16>::new()
            .with_prompt("SMTP port")
            .default(plan::DEFAULT_SMTP_PORT)
            .interact_text()
            .map_err(|err| miette!("setup prompt failed: {err}"))?;
        smtp_from = Some(
            Input::<String>::new()
                .with_prompt("SMTP from address")
                .interact_text()
                .map_err(|err| miette!("setup prompt failed: {err}"))?,
        );
        let encryption_items = ["starttls", "tls", "none"];
        smtp_encryption = select_value("SMTP encryption", &encryption_items, 0)?;
    }

    let security_items = ["standard", "hardened", "audit-only"];
    let security_profile = select_value("Security profile", &security_items, 0)?;

    let ssh_items = ["audit-only", "harden"];
    let ssh_policy = select_value("SSH policy", &ssh_items, 0)?;

    let options = plan_options(
        local_test,
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
        true,
        true,
        !local_test,
    );
    let setup_plan =
        plan::build_with_options(domain.clone(), options.clone()).map_err(miette::Report::new)?;

    println!();
    println!("3) Setup summary");
    print_setup_summary(&setup_plan);

    let proceed = Confirm::new()
        .with_prompt("Proceed with install preparation? Enter uses the default")
        .default(false)
        .interact()
        .map_err(|err| miette!("setup prompt failed: {err}"))?;

    if !proceed {
        println!("setup cancelled");
        return Ok(());
    }

    println!();
    println!("4) Install preparation");
    print_install(install::run(domain, options).map_err(miette::Report::new)?);
    Ok(())
}

fn select_value(prompt: &str, items: &[&str], default: usize) -> Result<String> {
    let selected = Select::new()
        .with_prompt(select_prompt(prompt))
        .items(items)
        .default(default)
        .interact()
        .map_err(|err| miette!("setup prompt failed: {err}"))?;

    Ok(items[selected].to_string())
}

fn select_prompt(prompt: &str) -> String {
    format!("{prompt} ({SELECT_PROMPT_HINT})")
}

fn print_setup_summary(plan: &plan::InstallPlan) {
    println!("domain: {}", plan.domain);
    println!("mode: {}", plan.deployment_mode);
    println!("web_server: {}", plan.web_server);
    println!("php_version: {}", plan.php_version);
    println!("database: {}", plan.database_engine);
    println!("site_user: {}", plan.site_user);
    println!("web_root: {}", plan.web_root);
    println!("www_mode: {}", plan.www_mode);
    println!("redis: {}", plan.redis_mode);
    println!("mail_mode: {}", plan.mail_mode);
    println!("security_profile: {}", plan.security_profile);
    println!("ssh_policy: {}", plan.ssh_policy);
    println!("dns_check: {}", plan.dns_check_required);
    println!(
        "packages: {} item(s), files: {} item(s), services: {} item(s)",
        plan.packages.len(),
        plan.files.len(),
        plan.services.len()
    );
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

fn format_plan(plan: &plan::InstallPlan) -> String {
    let mut output = String::new();

    output.push_str("G7 Installer Plan\n");
    output.push_str(&format!("domain: {}\n", plan.domain));
    output.push_str(&format!("deployment_mode: {}\n", plan.deployment_mode));
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
    println!("web_server: {}", report.web_server);
    println!("php_version: {}", report.php_version);
    println!("database: {}", report.database_engine);
    println!("site_user: {}", report.site_user);
    println!("web_root_mode: {}", report.web_root_mode);
    println!("web_root: {}", report.web_root);
    println!("www_mode: {}", report.www_mode);
    println!("redis: {}", report.redis_mode);
    println!("mail_mode: {}", report.mail_mode);
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
        assert!(output.contains("php_version: 8.5"));
        assert!(output.contains("database: mariadb"));
        assert!(output.contains("redis: enable"));
        assert!(output.contains("rollback: true"));
        assert!(output.contains("- Apache is running."));
        Ok(())
    }
}
