use crate::command::{CommandError, CommandRunner, CommandSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privilege {
    Root,
    User,
    Unknown,
}

pub fn current_privilege<R: CommandRunner>(runner: &R) -> Result<Privilege, CommandError> {
    let output = runner.run(&CommandSpec::new("id").arg("-u"))?;

    if output.status != 0 {
        return Ok(Privilege::Unknown);
    }

    match output.stdout.trim() {
        "0" => Ok(Privilege::Root),
        _ => Ok(Privilege::User),
    }
}

#[cfg(test)]
mod tests {
    use super::{Privilege, current_privilege};
    use crate::command::{CommandOutput, FakeCommandRunner};

    #[test]
    fn detects_root_user() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));

        let privilege = current_privilege(&runner)?;

        assert_eq!(privilege, Privilege::Root);
        Ok(())
    }
}
