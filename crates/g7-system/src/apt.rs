//! Non-interactive apt command helpers.
//!
//! Installer package changes must go through this module so the exact command
//! shape is testable and shell-free.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub const APT_GET: &str = "apt-get";

pub fn apt_update<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(
        &apt_env()
            .arg(APT_GET)
            .arg("update")
            .arg("-o")
            .arg("Dpkg::Use-Pty=0"),
    )
}

pub fn apt_install<R: CommandRunner>(
    runner: &R,
    packages: &[String],
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &apt_env()
            .arg(APT_GET)
            .arg("install")
            .arg("-y")
            .arg("--no-install-recommends")
            .arg("-o")
            .arg("Dpkg::Use-Pty=0")
            .args(packages.iter().cloned()),
    )
}

/// Installs Oracle's MySQL APT configuration package without interactive prompts.
pub fn apt_install_mysql_repo_config<R: CommandRunner>(
    runner: &R,
    package_path: &str,
    server_component: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &apt_env()
            .arg(format!("MYSQL_SERVER_VERSION={server_component}"))
            .arg("MYSQL_CONNECTORS=Disabled")
            .arg(APT_GET)
            .arg("install")
            .arg("-y")
            .arg("--no-install-recommends")
            .arg("-o")
            .arg("Dpkg::Use-Pty=0")
            .arg(package_path),
    )
}

pub fn apt_purge<R: CommandRunner>(
    runner: &R,
    packages: &[String],
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &apt_env()
            .arg(APT_GET)
            .arg("purge")
            .arg("-y")
            .arg("--auto-remove")
            .arg("-o")
            .arg("Dpkg::Use-Pty=0")
            .args(packages.iter().cloned()),
    )
}

pub fn apt_mark_manual<R: CommandRunner>(
    runner: &R,
    packages: &[String],
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("apt-mark")
            .arg("manual")
            .args(packages.iter().cloned()),
    )
}

pub fn apt_add_repository<R: CommandRunner>(
    runner: &R,
    repository: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("env")
            .arg("LC_ALL=C.UTF-8")
            .arg("add-apt-repository")
            .arg("-y")
            .arg(repository),
    )
}

pub fn apt_candidate_available<R: CommandRunner>(
    runner: &R,
    package: &str,
) -> Result<bool, CommandError> {
    Ok(apt_candidate_version(runner, package)?.is_some())
}

/// Returns the apt candidate version, or `None` when the package has no candidate.
pub fn apt_candidate_version<R: CommandRunner>(
    runner: &R,
    package: &str,
) -> Result<Option<String>, CommandError> {
    let output = runner.run(&CommandSpec::new("apt-cache").arg("policy").arg(package))?;

    if output.status != 0 {
        return Ok(None);
    }

    Ok(output
        .stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Candidate:"))
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty() && *candidate != "(none)")
        .map(str::to_string))
}

fn apt_env() -> CommandSpec {
    CommandSpec::new("env")
        .arg("DEBIAN_FRONTEND=noninteractive")
        .arg("APT_LISTCHANGES_FRONTEND=none")
}

#[cfg(test)]
mod tests {
    use super::{
        apt_add_repository, apt_candidate_available, apt_candidate_version, apt_install,
        apt_install_mysql_repo_config, apt_mark_manual, apt_purge, apt_update,
    };
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn apt_install_is_noninteractive_and_shell_free()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        let packages = vec!["nginx".to_string(), "php8.3-fpm".to_string()];
        apt_install(&runner, &packages)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("env"));
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("DEBIAN_FRONTEND=noninteractive"))
        );
        assert!(recorded[0].args.contains(&OsString::from("apt-get")));
        assert!(recorded[0].args.contains(&OsString::from("install")));
        assert!(recorded[0].args.contains(&OsString::from("nginx")));
        assert!(recorded[0].args.contains(&OsString::from("php8.3-fpm")));
        Ok(())
    }

    #[test]
    fn mysql_repo_config_install_selects_lts_noninteractively()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        apt_install_mysql_repo_config(&runner, "/tmp/mysql-apt-config.deb", "mysql-8.4-lts")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("env"));
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("MYSQL_SERVER_VERSION=mysql-8.4-lts"))
        );
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("MYSQL_CONNECTORS=Disabled"))
        );
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("/tmp/mysql-apt-config.deb"))
        );
        Ok(())
    }

    #[test]
    fn apt_update_is_noninteractive() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        apt_update(&runner)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("env"));
        assert!(recorded[0].args.contains(&OsString::from("apt-get")));
        assert!(recorded[0].args.contains(&OsString::from("update")));
        Ok(())
    }

    #[test]
    fn apt_purge_is_noninteractive_and_auto_removes()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        let packages = vec!["nginx".to_string(), "php8.3-fpm".to_string()];
        apt_purge(&runner, &packages)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("env"));
        assert!(recorded[0].args.contains(&OsString::from("apt-get")));
        assert!(recorded[0].args.contains(&OsString::from("purge")));
        assert!(recorded[0].args.contains(&OsString::from("--auto-remove")));
        assert!(recorded[0].args.contains(&OsString::from("nginx")));
        assert!(recorded[0].args.contains(&OsString::from("php8.3-fpm")));
        Ok(())
    }

    #[test]
    fn apt_mark_manual_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("certbot set to manually installed"));

        apt_mark_manual(
            &runner,
            &["certbot".to_string(), "python3-certbot-nginx".to_string()],
        )?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("apt-mark"));
        assert_eq!(recorded[0].args[0], OsString::from("manual"));
        assert!(recorded[0].args.contains(&OsString::from("certbot")));
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("python3-certbot-nginx"))
        );
        Ok(())
    }

    #[test]
    fn apt_candidate_detects_missing_package() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("php8.5-fpm:\n  Candidate: (none)\n"));

        assert!(!apt_candidate_available(&runner, "php8.5-fpm")?);
        Ok(())
    }

    #[test]
    fn apt_candidate_returns_available_version()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(
            "mysql-server:\n  Installed: (none)\n  Candidate: 8.4.6-0ubuntu0.26.04.1\n",
        ));

        assert_eq!(
            apt_candidate_version(&runner, "mysql-server")?.as_deref(),
            Some("8.4.6-0ubuntu0.26.04.1")
        );
        Ok(())
    }

    #[test]
    fn apt_candidate_treats_failed_policy_as_unavailable()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::failure(100, "package lookup failed"));

        assert_eq!(apt_candidate_version(&runner, "missing")?, None);
        Ok(())
    }

    #[test]
    fn apt_add_repository_is_noninteractive_and_shell_free()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        apt_add_repository(&runner, "ppa:ondrej/php")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("env"));
        assert_eq!(recorded[0].args[0], OsString::from("LC_ALL=C.UTF-8"));
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("add-apt-repository"))
        );
        assert!(recorded[0].args.contains(&OsString::from("-y")));
        assert!(recorded[0].args.contains(&OsString::from("ppa:ondrej/php")));
        Ok(())
    }
}
