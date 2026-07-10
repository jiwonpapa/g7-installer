use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub const COMMAND_AUDIT_LOG_PATH: &str = "/var/log/g7-installer/commands.jsonl";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub stdin: Option<Vec<u8>>,
    pub cwd: Option<PathBuf>,
}

impl CommandSpec {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            stdin: None,
            cwd: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn stdin_bytes(mut self, stdin: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    pub fn current_dir(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn display(&self) -> String {
        let mut parts = vec![display_os(&self.program)];
        parts.extend(self.args.iter().map(display_os));
        parts.join(" ")
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
        let started = Instant::now();
        append_audit_entry(AuditEntry::started(spec));
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }

        let output = if let Some(stdin) = &spec.stdin {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|err| CommandError::Execute {
                    program: display_os(&spec.program),
                    message: err.to_string(),
                })?;

            let mut child_stdin = child.stdin.take().ok_or_else(|| CommandError::Execute {
                program: display_os(&spec.program),
                message: "failed to open command stdin".to_string(),
            })?;
            child_stdin
                .write_all(stdin)
                .map_err(|err| CommandError::Execute {
                    program: display_os(&spec.program),
                    message: err.to_string(),
                })?;
            drop(child_stdin);

            child
                .wait_with_output()
                .map_err(|err| CommandError::Execute {
                    program: display_os(&spec.program),
                    message: err.to_string(),
                })?
        } else {
            command.output().map_err(|err| CommandError::Execute {
                program: display_os(&spec.program),
                message: err.to_string(),
            })?
        };

        let output = CommandOutput {
            status: output.status.code().map_or(128, |code| code),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        append_audit_entry(AuditEntry::finished(
            spec,
            &output,
            started.elapsed().as_millis(),
        ));
        Ok(output)
    }
}

#[derive(Debug, Serialize)]
struct AuditEntry {
    timestamp_unix_ms: u128,
    event: &'static str,
    program: String,
    args: Vec<String>,
    cwd: Option<String>,
    status: Option<i32>,
    elapsed_ms: Option<u128>,
    stdout: Option<String>,
    stderr: Option<String>,
}

impl AuditEntry {
    fn started(spec: &CommandSpec) -> Self {
        Self::new(spec, "start", None, None, None, None)
    }

    fn finished(spec: &CommandSpec, output: &CommandOutput, elapsed_ms: u128) -> Self {
        Self::new(
            spec,
            "finish",
            Some(output.status),
            Some(elapsed_ms),
            Some(&output.stdout),
            Some(&output.stderr),
        )
    }

    fn new(
        spec: &CommandSpec,
        event: &'static str,
        status: Option<i32>,
        elapsed_ms: Option<u128>,
        stdout: Option<&str>,
        stderr: Option<&str>,
    ) -> Self {
        Self {
            timestamp_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_millis()),
            event,
            program: spec.program.to_string_lossy().into_owned(),
            args: redacted_args(&spec.args),
            cwd: spec
                .cwd
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            status,
            elapsed_ms,
            stdout: stdout.map(redacted_excerpt),
            stderr: stderr.map(redacted_excerpt),
        }
    }
}

fn append_audit_entry(entry: AuditEntry) {
    let path = std::path::Path::new(COMMAND_AUDIT_LOG_PATH);
    let Some(parent) = path.parent() else {
        return;
    };
    if !parent.exists() {
        return;
    }

    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600);
    let Ok(mut file) = options.open(path) else {
        return;
    };
    let Ok(payload) = serde_json::to_vec(&entry) else {
        return;
    };
    if file.write_all(&payload).is_ok() && file.write_all(b"\n").is_ok() {
        let _ = file.sync_data();
    }
}

fn redacted_args(args: &[OsString]) -> Vec<String> {
    let mut redact_next = false;
    args.iter()
        .map(|arg| {
            let value = arg.to_string_lossy();
            if redact_next {
                redact_next = false;
                return "******".to_string();
            }
            let lower = value.to_ascii_lowercase();
            if is_sensitive_key(&lower) {
                if let Some((key, _)) = value.split_once('=') {
                    return format!("{key}=******");
                }
                redact_next = true;
            }
            value.into_owned()
        })
        .collect()
}

fn redacted_excerpt(value: &str) -> String {
    let mut output = String::new();
    for line in value.lines() {
        let lower = line.to_ascii_lowercase();
        let sanitized = if is_sensitive_key(&lower) {
            line.split_once('=')
                .or_else(|| line.split_once(':'))
                .map_or_else(
                    || "[redacted]".to_string(),
                    |(key, _)| format!("{key}=******"),
                )
        } else {
            line.to_string()
        };
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&sanitized);
        if output.chars().count() >= 2_000 {
            output = output.chars().take(2_000).collect();
            output.push_str("...");
            break;
        }
    }
    output
}

fn is_sensitive_key(value: &str) -> bool {
    [
        "password",
        "passwd",
        "secret",
        "token",
        "private_key",
        "smtp_password",
        "db_password",
    ]
    .iter()
    .any(|key| value.contains(key))
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
    use super::{
        CommandOutput, CommandRunner, CommandSpec, FakeCommandRunner, redacted_args,
        redacted_excerpt,
    };
    use std::ffi::OsString;

    #[test]
    fn command_spec_collects_argv_without_shell() {
        let spec = CommandSpec::new("apt-get").arg("update").arg("-y");

        assert_eq!(spec.program, OsString::from("apt-get"));
        assert_eq!(
            spec.args,
            vec![OsString::from("update"), OsString::from("-y")]
        );
        assert_eq!(spec.stdin, None);
    }

    #[test]
    fn command_spec_keeps_stdin_out_of_display() {
        let spec = CommandSpec::new("chpasswd").stdin_bytes(b"g7:secret\n".to_vec());

        assert_eq!(spec.display(), "chpasswd");
        assert_eq!(spec.stdin, Some(b"g7:secret\n".to_vec()));
    }

    #[test]
    fn command_spec_records_current_dir_separately() {
        let spec = CommandSpec::new("composer")
            .arg("install")
            .current_dir("/srv/example");

        assert_eq!(spec.display(), "composer install");
        assert_eq!(
            spec.cwd.as_deref(),
            Some(std::path::Path::new("/srv/example"))
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

    #[test]
    fn audit_redacts_secret_arguments_and_output() {
        let args = vec![
            OsString::from("--token"),
            OsString::from("plain-secret"),
            OsString::from("DB_PASSWORD=value"),
        ];
        assert_eq!(
            redacted_args(&args),
            vec!["--token", "******", "DB_PASSWORD=******"]
        );
        assert_eq!(
            redacted_excerpt("ok\npassword=hunter2\nnext"),
            "ok\npassword=******\nnext"
        );
    }
}
