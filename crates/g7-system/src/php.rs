//! PHP runtime constants and configuration validation helpers.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};
use std::path::Path;

pub const UBUNTU_FPM_VERSION: &str = "8.3";
pub const DEFAULT_FPM_VERSION: &str = "8.5";
pub const NEXT_FPM_VERSION: &str = DEFAULT_FPM_VERSION;
pub const SUPPORTED_FPM_VERSIONS: [&str; 2] = [UBUNTU_FPM_VERSION, DEFAULT_FPM_VERSION];
pub const PHP_SOURCE_AUTO: &str = "auto";
pub const PHP_SOURCE_UBUNTU: &str = "ubuntu";
pub const PHP_SOURCE_ONDREJ: &str = "ondrej";
pub const SUPPORTED_PHP_SOURCES: [&str; 3] =
    [PHP_SOURCE_AUTO, PHP_SOURCE_UBUNTU, PHP_SOURCE_ONDREJ];

/// Asks the selected PHP-FPM binary to parse its active global and pool files.
pub fn fpm_config_test<R: CommandRunner>(
    runner: &R,
    version: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new(format!("php-fpm{version}")).arg("-t"))
}

/// Parses candidate FPM and PHP ini files without reading the active files.
pub fn fpm_candidate_config_test<R: CommandRunner>(
    runner: &R,
    version: &str,
    fpm_config: &Path,
    php_ini: &Path,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new(format!("php-fpm{version}"))
            .arg("-y")
            .arg(fpm_config.as_os_str())
            .arg("-c")
            .arg(php_ini.as_os_str())
            .arg("-t"),
    )
}

#[cfg(test)]
mod tests {
    use super::{fpm_candidate_config_test, fpm_config_test};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn fpm_config_test_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(
            "configuration file test is successful",
        ));

        fpm_config_test(&runner, "8.5")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("php-fpm8.5"));
        assert_eq!(recorded[0].args, vec![OsString::from("-t")]);
        Ok(())
    }

    #[test]
    fn candidate_test_uses_explicit_config_files()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(
            "configuration file test is successful",
        ));

        fpm_candidate_config_test(
            &runner,
            "8.3",
            std::path::Path::new("/tmp/php-fpm.conf"),
            std::path::Path::new("/tmp/php.ini"),
        )?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("php-fpm8.3"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("-y"),
                OsString::from("/tmp/php-fpm.conf"),
                OsString::from("-c"),
                OsString::from("/tmp/php.ini"),
                OsString::from("-t"),
            ]
        );
        Ok(())
    }
}
