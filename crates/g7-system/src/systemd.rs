use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};
use std::path::PathBuf;

pub const QUEUE_SERVICE: &str = "/etc/systemd/system/g7-queue.service";
pub const REVERB_SERVICE: &str = "/etc/systemd/system/g7-reverb.service";

pub fn daemon_reload<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("systemctl").arg("daemon-reload"))
}

/// Clears stale failed-state records after installer-owned units are removed.
pub fn reset_failed<R: CommandRunner>(
    runner: &R,
    units: &[String],
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("systemctl")
            .arg("reset-failed")
            .args(units.iter()),
    )
}

/// Validates unit files without loading or starting them.
pub fn verify_units<R: CommandRunner>(
    runner: &R,
    units: &[PathBuf],
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("systemd-analyze")
            .arg("verify")
            .args(units.iter().map(|path| path.as_os_str())),
    )
}

#[cfg(test)]
mod tests {
    use super::{daemon_reload, reset_failed, verify_units};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn reloads_systemd_without_shell() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        daemon_reload(&runner)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("systemctl"));
        assert_eq!(recorded[0].args, vec![OsString::from("daemon-reload")]);
        Ok(())
    }

    #[test]
    fn resets_failed_units_without_shell() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        reset_failed(
            &runner,
            &[
                "g7-queue.service".to_string(),
                "g7-scheduler.service".to_string(),
            ],
        )?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("systemctl"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("reset-failed"),
                OsString::from("g7-queue.service"),
                OsString::from("g7-scheduler.service"),
            ]
        );
        Ok(())
    }

    #[test]
    fn verifies_units_without_shell() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        verify_units(
            &runner,
            &[
                "/etc/systemd/system/g7-a.service".into(),
                "/etc/systemd/system/g7-b.timer".into(),
            ],
        )?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("systemd-analyze"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("verify"),
                OsString::from("/etc/systemd/system/g7-a.service"),
                OsString::from("/etc/systemd/system/g7-b.timer"),
            ]
        );
        Ok(())
    }
}
