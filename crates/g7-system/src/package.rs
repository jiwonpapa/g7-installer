use crate::command::{CommandError, CommandRunner, CommandSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageStatus {
    Installed,
    NotInstalled,
    Unknown,
}

pub fn package_status<R: CommandRunner>(
    runner: &R,
    package: &str,
) -> Result<PackageStatus, CommandError> {
    let output = runner.run(
        &CommandSpec::new("dpkg-query")
            .arg("-W")
            .arg("-f=${Status}")
            .arg(package),
    )?;

    if output.status == 0 && output.stdout.trim() == "install ok installed" {
        Ok(PackageStatus::Installed)
    } else if output.status != 0 {
        Ok(PackageStatus::NotInstalled)
    } else {
        Ok(PackageStatus::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::{PackageStatus, package_status};
    use crate::command::{CommandOutput, FakeCommandRunner};

    #[test]
    fn detects_installed_package() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("install ok installed"));

        let status = package_status(&runner, "nginx")?;

        assert_eq!(status, PackageStatus::Installed);
        Ok(())
    }
}
