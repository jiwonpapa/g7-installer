use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceActivity {
    Active,
    Inactive,
    NotFound,
    Unknown,
}

pub fn is_active<R: CommandRunner>(
    runner: &R,
    service: &str,
) -> Result<ServiceActivity, CommandError> {
    let output = runner.run(&CommandSpec::new("systemctl").arg("is-active").arg(service))?;

    match (output.status, output.stdout.trim(), output.stderr.trim()) {
        (0, "active", _) => Ok(ServiceActivity::Active),
        (_, "inactive" | "failed" | "deactivating" | "activating", _) => {
            Ok(ServiceActivity::Inactive)
        }
        (_, "unknown", _) => Ok(ServiceActivity::NotFound),
        (_, _, stderr) if stderr.contains("could not be found") => Ok(ServiceActivity::NotFound),
        _ => Ok(ServiceActivity::Unknown),
    }
}

pub fn enable_now<R: CommandRunner>(
    runner: &R,
    service: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("systemctl")
            .arg("enable")
            .arg("--now")
            .arg(service),
    )
}

pub fn disable_now<R: CommandRunner>(
    runner: &R,
    service: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("systemctl")
            .arg("disable")
            .arg("--now")
            .arg(service),
    )
}

#[cfg(test)]
mod tests {
    use super::{ServiceActivity, disable_now, is_active};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn detects_active_service() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("active\n"));

        let activity = is_active(&runner, "nginx")?;

        assert_eq!(activity, ServiceActivity::Active);
        Ok(())
    }

    #[test]
    fn disables_service_now_without_shell() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        disable_now(&runner, "nginx")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("systemctl"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("disable"),
                OsString::from("--now"),
                OsString::from("nginx")
            ]
        );
        Ok(())
    }
}
