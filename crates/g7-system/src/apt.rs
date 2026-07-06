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

pub fn apt_candidate_available<R: CommandRunner>(
    runner: &R,
    package: &str,
) -> Result<bool, CommandError> {
    let output = runner.run(&CommandSpec::new("apt-cache").arg("policy").arg(package))?;

    if output.status != 0 {
        return Ok(false);
    }

    Ok(output
        .stdout
        .lines()
        .any(|line| line.trim().starts_with("Candidate:") && !line.contains("(none)")))
}

fn apt_env() -> CommandSpec {
    CommandSpec::new("env")
        .arg("DEBIAN_FRONTEND=noninteractive")
        .arg("APT_LISTCHANGES_FRONTEND=none")
}

#[cfg(test)]
mod tests {
    use super::{apt_candidate_available, apt_install, apt_update};
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
    fn apt_candidate_detects_missing_package() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("php8.5-fpm:\n  Candidate: (none)\n"));

        assert!(!apt_candidate_available(&runner, "php8.5-fpm")?);
        Ok(())
    }
}
