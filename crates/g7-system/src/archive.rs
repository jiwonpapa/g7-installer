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

/// Computes a file SHA-256 digest with the platform coreutils binary.
pub fn sha256_file<R: CommandRunner>(
    runner: &R,
    path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("sha256sum").arg("--").arg(path))
}

pub fn fetch_text<R: CommandRunner>(runner: &R, url: &str) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("curl")
            .arg("-fsSL")
            .arg("--max-time")
            .arg("30")
            .arg("--header")
            .arg("Accept: application/vnd.github+json")
            .arg("--user-agent")
            .arg("g7inst")
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

pub fn unzip_test<R: CommandRunner>(
    runner: &R,
    archive_path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("unzip").arg("-tq").arg(archive_path))
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

pub fn git_rev_parse_head<R: CommandRunner>(
    runner: &R,
    repo_dir: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &git_repo_command(repo_dir)
            .arg("rev-parse")
            .arg("--verify")
            .arg("HEAD"),
    )
}

pub fn git_fsck_full<R: CommandRunner>(
    runner: &R,
    repo_dir: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&git_repo_command(repo_dir).arg("fsck").arg("--full"))
}

pub fn git_tracked_files_status<R: CommandRunner>(
    runner: &R,
    repo_dir: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("git")
            .arg("--no-optional-locks")
            .arg("-c")
            .arg(format!("safe.directory={repo_dir}"))
            .arg("-C")
            .arg(repo_dir)
            .arg("status")
            .arg("--porcelain=v1")
            .arg("--untracked-files=no"),
    )
}

pub fn git_ls_files_error_unmatch<R: CommandRunner>(
    runner: &R,
    repo_dir: &str,
    path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &git_repo_command(repo_dir)
            .arg("ls-files")
            .arg("--error-unmatch")
            .arg(path),
    )
}

fn git_repo_command(repo_dir: &str) -> CommandSpec {
    CommandSpec::new("git")
        .arg("-c")
        .arg(format!("safe.directory={repo_dir}"))
        .arg("-C")
        .arg(repo_dir)
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

pub fn copy_file<R: CommandRunner>(
    runner: &R,
    source_path: &str,
    destination_path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("cp")
            .arg("--")
            .arg(source_path)
            .arg(destination_path),
    )
}

pub fn test_file<R: CommandRunner>(runner: &R, path: &str) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("test").arg("-f").arg(path))
}

pub fn test_dir<R: CommandRunner>(runner: &R, path: &str) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("test").arg("-d").arg(path))
}

#[cfg(test)]
mod tests {
    use super::{
        copy_dir_contents, copy_file, download_file, fetch_text, git_clone, git_fsck_full,
        git_ls_files_error_unmatch, git_rev_parse_head, git_tracked_files_status, sha256_file,
        test_dir, test_file, unzip_archive, unzip_test,
    };
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
    fn sha256_file_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("abc  /tmp/package.deb\n"));

        sha256_file(&runner, "/tmp/package.deb")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("sha256sum"));
        assert_eq!(
            recorded[0].args,
            vec![OsString::from("--"), OsString::from("/tmp/package.deb")]
        );
        Ok(())
    }

    #[test]
    fn fetch_text_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(r#"{"tag_name":"7.0.2"}"#));

        fetch_text(
            &runner,
            "https://api.github.com/repos/gnuboard/g7/releases/latest",
        )?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("curl"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("-fsSL"),
                OsString::from("--max-time"),
                OsString::from("30"),
                OsString::from("--header"),
                OsString::from("Accept: application/vnd.github+json"),
                OsString::from("--user-agent"),
                OsString::from("g7inst"),
                OsString::from("https://api.github.com/repos/gnuboard/g7/releases/latest"),
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
    fn unzip_test_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        unzip_test(&runner, "/tmp/app.zip")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("unzip"));
        assert_eq!(
            recorded[0].args,
            vec![OsString::from("-tq"), OsString::from("/tmp/app.zip")]
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
    fn git_validation_commands_are_shell_free()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        for _ in 0..4 {
            runner.push_output(CommandOutput::success(""));
        }

        git_rev_parse_head(&runner, "/tmp/g7")?;
        git_fsck_full(&runner, "/tmp/g7")?;
        git_tracked_files_status(&runner, "/tmp/g7")?;
        git_ls_files_error_unmatch(&runner, "/tmp/g7", "public/index.php")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("git"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("-c"),
                OsString::from("safe.directory=/tmp/g7"),
                OsString::from("-C"),
                OsString::from("/tmp/g7"),
                OsString::from("rev-parse"),
                OsString::from("--verify"),
                OsString::from("HEAD"),
            ]
        );
        assert_eq!(
            recorded[1].args,
            vec![
                OsString::from("-c"),
                OsString::from("safe.directory=/tmp/g7"),
                OsString::from("-C"),
                OsString::from("/tmp/g7"),
                OsString::from("fsck"),
                OsString::from("--full"),
            ]
        );
        assert_eq!(
            recorded[2].args,
            vec![
                OsString::from("--no-optional-locks"),
                OsString::from("-c"),
                OsString::from("safe.directory=/tmp/g7"),
                OsString::from("-C"),
                OsString::from("/tmp/g7"),
                OsString::from("status"),
                OsString::from("--porcelain=v1"),
                OsString::from("--untracked-files=no"),
            ]
        );
        assert_eq!(
            recorded[3].args,
            vec![
                OsString::from("-c"),
                OsString::from("safe.directory=/tmp/g7"),
                OsString::from("-C"),
                OsString::from("/tmp/g7"),
                OsString::from("ls-files"),
                OsString::from("--error-unmatch"),
                OsString::from("public/index.php"),
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

    #[test]
    fn copy_file_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        copy_file(&runner, "/tmp/source/.env.example", "/srv/app/.env")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("cp"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("--"),
                OsString::from("/tmp/source/.env.example"),
                OsString::from("/srv/app/.env"),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_path_commands_are_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));

        test_file(&runner, "/srv/app/index.php")?;
        test_dir(&runner, "/srv/app/wp-content")?;
        let recorded = runner.recorded();

        assert_eq!(
            recorded[0].args,
            vec![OsString::from("-f"), OsString::from("/srv/app/index.php")]
        );
        assert_eq!(
            recorded[1].args,
            vec![OsString::from("-d"), OsString::from("/srv/app/wp-content")]
        );
        Ok(())
    }
}
