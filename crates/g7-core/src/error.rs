use miette::Diagnostic;
use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Diagnostic, ThisError)]
pub enum Error {
    #[error("missing required input: {field}")]
    #[diagnostic(
        code(g7::input::missing),
        help("Provide the required option and run the command again.")
    )]
    MissingInput { field: &'static str },

    #[error("invalid domain: {domain}")]
    #[diagnostic(
        code(g7::input::invalid_domain),
        help("Use a plain domain such as example.com.")
    )]
    InvalidDomain { domain: String },

    #[error("invalid PHP version: {version}")]
    #[diagnostic(
        code(g7::input::invalid_php_version),
        help("Use one of the supported PHP versions.")
    )]
    InvalidPhpVersion { version: String, supported: String },

    #[error("invalid value for {field}: {value}")]
    #[diagnostic(
        code(g7::input::invalid_option),
        help("Use one of the supported values.")
    )]
    InvalidOption {
        field: &'static str,
        value: String,
        supported: String,
    },

    #[error("{command} is not implemented yet")]
    #[diagnostic(
        code(g7::command::not_implemented),
        help("This command is reserved by the MVP spec and will be implemented in a later batch.")
    )]
    NotImplemented { command: &'static str },

    #[error("install requires root or sudo")]
    #[diagnostic(
        code(g7::install::privilege_required),
        help(
            "Run the command again with sudo, for example: sudo g7inst install --domain example.com"
        )
    )]
    PrivilegeRequired,

    #[error("install blocked by preflight checks: {checks}")]
    #[diagnostic(
        code(g7::install::blocked),
        help("Run g7inst doctor, resolve the failing checks, and retry on a fresh Ubuntu VPS.")
    )]
    InstallBlocked { checks: String },

    #[error("install command failed during {step}: {command} exited with status {status}")]
    #[diagnostic(
        code(g7::install::command_failed),
        help(
            "Check /var/log/g7-installer/report.json and retry after fixing the reported package or service failure."
        )
    )]
    InstallCommandFailed {
        step: &'static str,
        command: String,
        status: i32,
        stdout: String,
        stderr: String,
    },

    #[error("package is not available from current apt sources: {package}")]
    #[diagnostic(
        code(g7::install::package_unavailable),
        help(
            "Run with a package version available on this Ubuntu release, or add the required apt source before retrying."
        )
    )]
    PackageUnavailable { package: String },

    #[error("install verification failed: {checks}")]
    #[diagnostic(
        code(g7::install::verification_failed),
        help("Review the failed package, service, or port checks in the install report.")
    )]
    InstallVerificationFailed { checks: String },

    #[error("failed to write installer file: {path}")]
    #[diagnostic(
        code(g7::install::file_write_failed),
        help("Check filesystem permissions and whether the path already exists.")
    )]
    FileWriteFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read installer file: {path}")]
    #[diagnostic(
        code(g7::install::file_read_failed),
        help("Check that the installer metadata exists and is readable.")
    )]
    FileReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to remove installer file: {path}")]
    #[diagnostic(
        code(g7::reset::file_remove_failed),
        help("Check filesystem permissions and retry the reset command.")
    )]
    FileRemoveFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("reset requires explicit confirmation")]
    #[diagnostic(
        code(g7::reset::confirmation_required),
        help("Run reset with --yes, or use --dry-run to preview the paths.")
    )]
    ResetConfirmationRequired,

    #[error("unsafe reset path refused: {path}")]
    #[diagnostic(
        code(g7::reset::unsafe_path),
        help("Reset only removes paths tracked by installer ownership metadata.")
    )]
    UnsafeResetPath { path: String },

    #[error("rollback requires explicit confirmation")]
    #[diagnostic(
        code(g7::rollback::confirmation_required),
        help("Run rollback with --yes, or use --dry-run to preview package and metadata changes.")
    )]
    RollbackConfirmationRequired,

    #[error("rollback blocked: {reason}")]
    #[diagnostic(
        code(g7::rollback::blocked),
        help(
            "Rollback only runs before app/database/certificate content is created and when web-root contents are installer-owned."
        )
    )]
    RollbackBlocked { reason: String },

    #[error("rollback command failed during {step}: {command} exited with status {status}")]
    #[diagnostic(
        code(g7::rollback::command_failed),
        help("Review the rollback output, fix the failed service or package state, and retry.")
    )]
    RollbackCommandFailed {
        step: &'static str,
        command: String,
        status: i32,
        stdout: String,
        stderr: String,
    },

    #[error("rollback verification failed: {checks}")]
    #[diagnostic(
        code(g7::rollback::verification_failed),
        help(
            "Some packages still appear installed after rollback. Review apt output and remove them manually if needed."
        )
    )]
    RollbackVerificationFailed { checks: String },
}
