use clap::{Parser, Subcommand};
use g7_core::commands::{
    DoctorCheckStatus, doctor, install, logs, plan, self_update, status, update,
};
use miette::Result;

#[derive(Debug, Parser)]
#[command(name = "g7")]
#[command(version)]
#[command(about = "G7 Installer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Diagnose whether this server can be used for a G7 install.
    Doctor,
    /// Show the installation plan before making changes.
    Plan {
        /// Domain that will serve the G7 site.
        #[arg(long)]
        domain: String,
    },
    /// Install G7 on a fresh Ubuntu VPS.
    Install {
        /// Domain that will serve the G7 site.
        #[arg(long)]
        domain: String,
    },
    /// Show installed G7 and service status.
    Status,
    /// Show installer log location.
    Logs,
    /// Update the installed G7 application.
    Update,
    /// Update the installer binary.
    SelfUpdate,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Doctor => print_doctor(doctor::run()),
        Command::Plan { domain } => print_plan(plan::build(domain).map_err(miette::Report::new)?),
        Command::Install { domain } => {
            print_install(install::run(domain).map_err(miette::Report::new)?);
        }
        Command::Status => print_status(status::read()),
        Command::Logs => print_logs(logs::location()),
        Command::Update => {
            update::run().map_err(miette::Report::new)?;
        }
        Command::SelfUpdate => {
            self_update::run().map_err(miette::Report::new)?;
        }
    }

    Ok(())
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
        assert!(output.contains("- /var/www/g7 (create)"));
        assert!(output.contains("- 443/tcp: HTTPS traffic."));
        assert!(output.contains("- Apache is running."));
        Ok(())
    }
}
