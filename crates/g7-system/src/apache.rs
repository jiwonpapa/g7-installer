//! Apache command helpers.
//!
//! Apache mutations must go through this module so command shapes are
//! shell-free and testable.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub const SERVICE_NAME: &str = "apache2";
pub const G7_SITE_AVAILABLE: &str = "/etc/apache2/sites-available/g7.conf";
pub const G7_SITE_ENABLED: &str = "/etc/apache2/sites-enabled/g7.conf";

pub fn config_test<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("apache2ctl").arg("configtest"))
}

pub fn enable_module<R: CommandRunner>(
    runner: &R,
    module: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("a2enmod").arg(module))
}

#[cfg(test)]
mod tests {
    use super::{config_test, enable_module};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn apache_config_test_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        config_test(&runner)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("apache2ctl"));
        assert_eq!(recorded[0].args, vec![OsString::from("configtest")]);
        Ok(())
    }

    #[test]
    fn apache_module_enable_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        enable_module(&runner, "proxy_fcgi")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("a2enmod"));
        assert_eq!(recorded[0].args, vec![OsString::from("proxy_fcgi")]);
        Ok(())
    }
}
