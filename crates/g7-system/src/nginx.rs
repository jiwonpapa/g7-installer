//! Nginx command helpers.
//!
//! Web-server mutations must go through this module so command shapes are
//! shell-free and testable.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub const SERVICE_NAME: &str = "nginx";
pub const G7_SITE_AVAILABLE: &str = "/etc/nginx/sites-available/g7.conf";
pub const G7_SITE_ENABLED: &str = "/etc/nginx/sites-enabled/g7.conf";
pub const G7_DEFAULT_DENY_AVAILABLE: &str = "/etc/nginx/sites-available/g7-default-deny.conf";
pub const G7_DEFAULT_DENY_ENABLED: &str = "/etc/nginx/sites-enabled/g7-default-deny.conf";

pub fn config_test<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("nginx").arg("-t"))
}

#[cfg(test)]
mod tests {
    use super::config_test;
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn nginx_config_test_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        config_test(&runner)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("nginx"));
        assert_eq!(recorded[0].args, vec![OsString::from("-t")]);
        Ok(())
    }
}
