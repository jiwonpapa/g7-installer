//! Download, archive, and source-copy helpers.
//!
//! These wrappers keep app source preparation shell-free and testable.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub fn download_file<R: CommandRunner>(
    runner: &R,
    url: &str,
    output_path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("curl")
            .arg("-fsSL")
            .arg("--max-time")
            .arg("120")
            .arg("-o")
            .arg(output_path)
            .arg(url),
    )
}

pub fn unzip_archive<R: CommandRunner>(
    runner: &R,
    archive_path: &str,
    destination: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("unzip")
            .arg("-q")
            .arg(archive_path)
            .arg("-d")
            .arg(destination),
    )
}

pub fn git_clone<R: CommandRunner>(
    runner: &R,
    repo_url: &str,
    reference: &str,
    destination: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("git")
            .arg("clone")
            .arg("--depth")
            .arg("1")
            .arg("--branch")
            .arg(reference)
            .arg(repo_url)
            .arg(destination),
    )
}

pub fn copy_dir_contents<R: CommandRunner>(
    runner: &R,
    source_dir: &str,
    destination_dir: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("cp")
            .arg("-a")
            .arg(format!("{source_dir}/."))
            .arg(destination_dir),
    )
}

#[cfg(test)]
mod tests {
    use super::{copy_dir_contents, download_file, git_clone, unzip_archive};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn download_file_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        download_file(&runner, "https://example.com/app.zip", "/tmp/app.zip")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("curl"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("-fsSL"),
                OsString::from("--max-time"),
                OsString::from("120"),
                OsString::from("-o"),
                OsString::from("/tmp/app.zip"),
                OsString::from("https://example.com/app.zip"),
            ]
        );
        Ok(())
    }

    #[test]
    fn unzip_archive_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        unzip_archive(&runner, "/tmp/app.zip", "/tmp/app")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("unzip"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("-q"),
                OsString::from("/tmp/app.zip"),
                OsString::from("-d"),
                OsString::from("/tmp/app"),
            ]
        );
        Ok(())
    }

    #[test]
    fn git_clone_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        git_clone(
            &runner,
            "https://github.com/gnuboard/g7.git",
            "7.0.0",
            "/tmp/g7",
        )?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("git"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("clone"),
                OsString::from("--depth"),
                OsString::from("1"),
                OsString::from("--branch"),
                OsString::from("7.0.0"),
                OsString::from("https://github.com/gnuboard/g7.git"),
                OsString::from("/tmp/g7"),
            ]
        );
        Ok(())
    }

    #[test]
    fn copy_dir_contents_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        copy_dir_contents(&runner, "/tmp/app", "/home/g7/public_html")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("cp"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("-a"),
                OsString::from("/tmp/app/."),
                OsString::from("/home/g7/public_html"),
            ]
        );
        Ok(())
    }
}
