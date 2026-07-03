use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
}

impl CommandSpec {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("failed to execute command: {program}")]
    Execute { program: String, message: String },

    #[error("fake command runner has no response for: {program}")]
    MissingFakeResponse { program: String },
}

#[derive(Debug, Default)]
pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError> {
        let output = Command::new(&spec.program)
            .args(&spec.args)
            .output()
            .map_err(|err| CommandError::Execute {
                program: display_os(&spec.program),
                message: err.to_string(),
            })?;

        Ok(CommandOutput {
            status: output.status.code().map_or(128, |code| code),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Debug, Default)]
pub struct FakeCommandRunner {
    responses: RefCell<VecDeque<Result<CommandOutput, CommandError>>>,
    recorded: RefCell<Vec<CommandSpec>>,
}

impl FakeCommandRunner {
    pub fn push_output(&self, output: CommandOutput) {
        self.responses.borrow_mut().push_back(Ok(output));
    }

    pub fn push_error(&self, error: CommandError) {
        self.responses.borrow_mut().push_back(Err(error));
    }

    pub fn recorded(&self) -> Vec<CommandSpec> {
        self.recorded.borrow().clone()
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError> {
        self.recorded.borrow_mut().push(spec.clone());

        match self.responses.borrow_mut().pop_front() {
            Some(response) => response,
            None => Err(CommandError::MissingFakeResponse {
                program: display_os(&spec.program),
            }),
        }
    }
}

impl CommandOutput {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            status: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    pub fn failure(status: i32, stderr: impl Into<String>) -> Self {
        Self {
            status,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

fn display_os(value: &OsString) -> String {
    value.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{CommandOutput, CommandRunner, CommandSpec, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn command_spec_collects_argv_without_shell() {
        let spec = CommandSpec::new("apt-get").arg("update").arg("-y");

        assert_eq!(spec.program, OsString::from("apt-get"));
        assert_eq!(
            spec.args,
            vec![OsString::from("update"), OsString::from("-y")]
        );
    }

    #[test]
    fn fake_runner_records_commands_in_order() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("ok\n"));

        let output = runner.run(&CommandSpec::new("true"))?;

        assert_eq!(output.stdout, "ok\n");
        assert_eq!(runner.recorded(), vec![CommandSpec::new("true")]);
        Ok(())
    }
}
