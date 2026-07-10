use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEvent {
    pub event: &'static str,
    pub command: String,
    pub stream: Option<&'static str>,
    pub line: Option<String>,
    pub status: Option<i32>,
    pub elapsed_ms: Option<u128>,
}

pub trait CommandObserver: Send + Sync {
    fn on_event(&self, event: CommandEvent);
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("failed to execute command: {program}")]
    Execute { program: String, message: String },

    #[error("fake command runner has no response for: {program}")]
    MissingFakeResponse { program: String },
}

#[derive(Clone, Default)]
pub struct RealCommandRunner {
    observer: Option<Arc<dyn CommandObserver>>,
}

impl std::fmt::Debug for RealCommandRunner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RealCommandRunner")
            .field("observer", &self.observer.is_some())
            .finish()
    }
}

impl RealCommandRunner {
    pub fn with_observer(observer: Arc<dyn CommandObserver>) -> Self {
        Self {
            observer: Some(observer),
        }
    }

    fn emit(&self, event: CommandEvent) {
        if let Some(observer) = &self.observer {
            observer.on_event(event);
        }
    }
}

impl CommandRunner for RealCommandRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError> {
        let started = Instant::now();
        append_audit_entry(AuditEntry::started(spec));
        let display = redacted_command(spec);
        self.emit(CommandEvent {
            event: "start",
            command: display.clone(),
            stream: None,
            line: None,
            status: None,
            elapsed_ms: None,
        });
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if spec.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }

        let mut child = command.spawn().map_err(|err| CommandError::Execute {
            program: display_os(&spec.program),
            message: err.to_string(),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| CommandError::Execute {
            program: display_os(&spec.program),
            message: "failed to open command stdout".to_string(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| CommandError::Execute {
            program: display_os(&spec.program),
            message: "failed to open command stderr".to_string(),
        })?;

        let stdout_observer = self.observer.clone();
        let stderr_observer = self.observer.clone();
        let stdout_command = display.clone();
        let stderr_command = display.clone();
        let (status, stdout, stderr) = std::thread::scope(|scope| {
            let stdout_reader = scope
                .spawn(move || read_stream(stdout, "stdout", &stdout_command, stdout_observer));
            let stderr_reader = scope
                .spawn(move || read_stream(stderr, "stderr", &stderr_command, stderr_observer));

            let stdin_result = if let Some(stdin) = &spec.stdin {
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
                Ok(())
            } else {
                Ok(())
            };
            stdin_result?;

            let status = child.wait().map_err(|err| CommandError::Execute {
                program: display_os(&spec.program),
                message: err.to_string(),
            })?;
            let stdout = join_stream(stdout_reader, &spec.program)?;
            let stderr = join_stream(stderr_reader, &spec.program)?;
            Ok::<_, CommandError>((status, stdout, stderr))
        })?;

        let output = CommandOutput {
            status: status.code().map_or(128, |code| code),
            stdout,
            stderr,
        };
        append_audit_entry(AuditEntry::finished(
            spec,
            &output,
            started.elapsed().as_millis(),
        ));
        self.emit(CommandEvent {
            event: "finish",
            command: display,
            stream: None,
            line: None,
            status: Some(output.status),
            elapsed_ms: Some(started.elapsed().as_millis()),
        });
        Ok(output)
    }
}

fn read_stream<R: Read>(
    reader: R,
    stream: &'static str,
    command: &str,
    observer: Option<Arc<dyn CommandObserver>>,
) -> std::io::Result<String> {
    let mut reader = BufReader::new(reader);
    let mut output = Vec::new();
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let read = reader.read_until(b'\n', &mut buffer)?;
        if read == 0 {
            break;
        }
        output.extend_from_slice(&buffer);
        if let Some(observer) = &observer {
            let line = String::from_utf8_lossy(&buffer)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            if !line.is_empty() {
                observer.on_event(CommandEvent {
                    event: "line",
                    command: command.to_string(),
                    stream: Some(stream),
                    line: Some(redacted_excerpt(&line)),
                    status: None,
                    elapsed_ms: None,
                });
            }
        }
    }
    Ok(String::from_utf8_lossy(&output).into_owned())
}

fn join_stream(
    handle: std::thread::ScopedJoinHandle<'_, std::io::Result<String>>,
    program: &OsString,
) -> Result<String, CommandError> {
    handle
        .join()
        .map_err(|_| CommandError::Execute {
            program: display_os(program),
            message: "command output reader panicked".to_string(),
        })?
        .map_err(|err| CommandError::Execute {
            program: display_os(program),
            message: err.to_string(),
        })
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

fn redacted_command(spec: &CommandSpec) -> String {
    let mut parts = vec![display_os(&spec.program)];
    parts.extend(redacted_args(&spec.args));
    parts.join(" ")
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
        CommandEvent, CommandObserver, CommandOutput, CommandRunner, CommandSpec,
        FakeCommandRunner, RealCommandRunner, redacted_args, redacted_excerpt,
    };
    use std::ffi::OsString;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingObserver(Mutex<Vec<CommandEvent>>);

    impl CommandObserver for RecordingObserver {
        fn on_event(&self, event: CommandEvent) {
            self.0.lock().expect("observer lock").push(event);
        }
    }

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

    #[test]
    fn real_runner_streams_start_lines_and_finish() {
        let observer = Arc::new(RecordingObserver::default());
        let runner = RealCommandRunner::with_observer(observer.clone());

        let output = runner
            .run(&CommandSpec::new("sh").args(["-c", "printf 'first\\n'; printf 'second\\n' >&2"]))
            .expect("command succeeds");

        assert_eq!(output.status, 0);
        let events = observer.0.lock().expect("observer lock");
        assert_eq!(events.first().map(|event| event.event), Some("start"));
        assert!(
            events
                .iter()
                .any(|event| event.line.as_deref() == Some("first"))
        );
        assert!(
            events
                .iter()
                .any(|event| event.line.as_deref() == Some("second"))
        );
        assert_eq!(events.last().map(|event| event.event), Some("finish"));
    }
}
