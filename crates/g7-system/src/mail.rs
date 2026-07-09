//! Postfix command helpers.
//!
//! Local mail delivery must be configured without interactive package prompts.
//! Keep Postfix command construction here so installer behavior is shell-free
//! and testable.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub fn postfix_preseed<R: CommandRunner>(
    runner: &R,
    mailname: &str,
) -> Result<CommandOutput, CommandError> {
    let selections = format!(
        "postfix postfix/mailname string {mailname}\npostfix postfix/main_mailer_type select Internet Site\n"
    );

    runner.run(&CommandSpec::new("debconf-set-selections").stdin_bytes(selections.into_bytes()))
}

pub fn postconf_set<R: CommandRunner>(
    runner: &R,
    key: &str,
    value: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("postconf")
            .arg("-e")
            .arg(format!("{key} = {value}")),
    )
}

#[cfg(test)]
mod tests {
    use super::{postconf_set, postfix_preseed};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn postfix_preseed_uses_stdin_without_shell()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        postfix_preseed(&runner, "example.com")?;
        let recorded = runner.recorded();

        assert_eq!(
            recorded[0].program,
            OsString::from("debconf-set-selections")
        );
        assert_eq!(
            String::from_utf8(recorded[0].stdin.clone().unwrap())?,
            "postfix postfix/mailname string example.com\npostfix postfix/main_mailer_type select Internet Site\n"
        );
        Ok(())
    }

    #[test]
    fn postconf_set_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        postconf_set(&runner, "inet_interfaces", "loopback-only")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("postconf"));
        assert_eq!(recorded[0].args[0], OsString::from("-e"));
        assert_eq!(
            recorded[0].args[1],
            OsString::from("inet_interfaces = loopback-only")
        );
        Ok(())
    }
}
