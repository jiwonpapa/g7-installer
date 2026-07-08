use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub const QUEUE_SERVICE: &str = "/etc/systemd/system/g7-queue.service";
pub const REVERB_SERVICE: &str = "/etc/systemd/system/g7-reverb.service";

pub fn daemon_reload<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("systemctl").arg("daemon-reload"))
}

#[cfg(test)]
mod tests {
    use super::daemon_reload;
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
}
