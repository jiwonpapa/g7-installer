use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};
use std::path::PathBuf;

pub const QUEUE_SERVICE: &str = "/etc/systemd/system/g7-queue.service";
pub const REVERB_SERVICE: &str = "/etc/systemd/system/g7-reverb.service";

pub fn daemon_reload<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("systemctl").arg("daemon-reload"))
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
    use super::{daemon_reload, verify_units};
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
