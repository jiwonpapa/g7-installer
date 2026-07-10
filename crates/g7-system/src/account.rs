//! Linux account and filesystem ownership helpers.
//!
//! Installer user/account changes are intentionally explicit and shell-free so
//! tests can prove the exact commands before they touch a real VPS.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub fn user_exists<R: CommandRunner>(runner: &R, user: &str) -> Result<bool, CommandError> {
    let output = runner.run(&CommandSpec::new("id").arg("-u").arg(user))?;
    Ok(output.status == 0)
}

pub fn create_login_user<R: CommandRunner>(
    runner: &R,
    user: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("useradd")
            .arg("--create-home")
            .arg("--shell")
            .arg("/bin/bash")
            .arg(user),
    )
}

pub fn set_login_password<R: CommandRunner>(
    runner: &R,
    user: &str,
    password: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("chpasswd").stdin_bytes(format!("{user}:{password}\n")))
}

pub fn delete_login_user<R: CommandRunner>(
    runner: &R,
    user: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("userdel").arg("-r").arg(user))
}

pub fn chown_recursive<R: CommandRunner>(
    runner: &R,
    owner_group: &str,
    path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("chown")
            .arg("-R")
            .arg(owner_group)
            .arg(path),
    )
}

pub fn chmod_recursive<R: CommandRunner>(
    runner: &R,
    mode: &str,
    path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("chmod").arg("-R").arg(mode).arg(path))
}

pub fn chmod_path<R: CommandRunner>(
    runner: &R,
    mode: &str,
    path: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("chmod").arg(mode).arg(path))
}

#[cfg(test)]
mod tests {
    use super::{
        chmod_path, chmod_recursive, chown_recursive, create_login_user, delete_login_user,
        set_login_password, user_exists,
    };
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn account_commands_are_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::failure(1, "missing"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));

        assert!(!user_exists(&runner, "g7")?);
        create_login_user(&runner, "g7")?;
        set_login_password(&runner, "g7", "Test-only_9x!")?;
        chown_recursive(&runner, "g7:www-data", "/home/g7/public_html")?;
        chmod_recursive(&runner, "0755", "/home/g7/public_html")?;
        chmod_path(&runner, "0711", "/home/g7")?;
        delete_login_user(&runner, "g7")?;

        let recorded = runner.recorded();
        assert_eq!(recorded[0].program, OsString::from("id"));
        assert_eq!(recorded[1].program, OsString::from("useradd"));
        assert_eq!(recorded[2].program, OsString::from("chpasswd"));
        assert_eq!(recorded[2].args, Vec::<OsString>::new());
        assert_eq!(recorded[2].stdin, Some(b"g7:Test-only_9x!\n".to_vec()));
        assert_eq!(recorded[3].program, OsString::from("chown"));
        assert_eq!(recorded[4].program, OsString::from("chmod"));
        assert_eq!(recorded[5].program, OsString::from("chmod"));
        assert_eq!(
            recorded[5].args,
            vec![OsString::from("0711"), OsString::from("/home/g7")]
        );
        assert_eq!(recorded[6].program, OsString::from("userdel"));
        assert_eq!(
            recorded[6].args,
            vec![OsString::from("-r"), OsString::from("g7")]
        );
        Ok(())
    }
}
