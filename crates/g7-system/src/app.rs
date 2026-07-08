//! Application runtime command helpers.
//!
//! These wrappers run Composer, NPM, and Artisan without a shell and with an
//! explicit working directory, so install steps remain auditable.

use std::path::Path;

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub fn composer_install<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("composer")
            .arg("install")
            .arg("--no-dev")
            .arg("--prefer-dist")
            .arg("--optimize-autoloader")
            .arg("--no-interaction")
            .current_dir(cwd),
    )
}

pub fn npm_install<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("npm").arg("install").current_dir(cwd))
}

pub fn npm_run_build<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("npm")
            .arg("run")
            .arg("build")
            .current_dir(cwd),
    )
}

pub fn artisan<I, S, R>(runner: &R, cwd: &Path, args: I) -> Result<CommandOutput, CommandError>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString>,
    R: CommandRunner,
{
    runner.run(
        &CommandSpec::new("php")
            .arg("artisan")
            .args(args)
            .current_dir(cwd),
    )
}

#[cfg(test)]
mod tests {
    use super::{artisan, composer_install, npm_install, npm_run_build};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;
    use std::path::Path;

    #[test]
    fn composer_install_runs_in_app_directory()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        composer_install(&runner, Path::new("/home/site/public_html"))?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("composer"));
        assert_eq!(
            recorded[0].cwd.as_deref(),
            Some(Path::new("/home/site/public_html"))
        );
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("install"),
                OsString::from("--no-dev"),
                OsString::from("--prefer-dist"),
                OsString::from("--optimize-autoloader"),
                OsString::from("--no-interaction"),
            ]
        );
        Ok(())
    }

    #[test]
    fn npm_commands_run_in_app_directory() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));

        npm_install(&runner, Path::new("/srv/app"))?;
        npm_run_build(&runner, Path::new("/srv/app"))?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("npm"));
        assert_eq!(recorded[0].args, vec![OsString::from("install")]);
        assert_eq!(recorded[0].cwd.as_deref(), Some(Path::new("/srv/app")));
        assert_eq!(recorded[1].program, OsString::from("npm"));
        assert_eq!(
            recorded[1].args,
            vec![OsString::from("run"), OsString::from("build")]
        );
        assert_eq!(recorded[1].cwd.as_deref(), Some(Path::new("/srv/app")));
        Ok(())
    }

    #[test]
    fn artisan_runs_without_shell() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        artisan(&runner, Path::new("/srv/app"), ["migrate", "--force"])?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("php"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("artisan"),
                OsString::from("migrate"),
                OsString::from("--force"),
            ]
        );
        assert_eq!(recorded[0].cwd.as_deref(), Some(Path::new("/srv/app")));
        Ok(())
    }
}
