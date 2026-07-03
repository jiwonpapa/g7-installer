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

    #[error("{command} is not implemented yet")]
    #[diagnostic(
        code(g7::command::not_implemented),
        help("This command is reserved by the MVP spec and will be implemented in a later batch.")
    )]
    NotImplemented { command: &'static str },

    #[error("install requires root or sudo")]
    #[diagnostic(
        code(g7::install::privilege_required),
        help("Run the command again with sudo, for example: sudo g7 install --domain example.com")
    )]
    PrivilegeRequired,

    #[error("install blocked by preflight checks: {checks}")]
    #[diagnostic(
        code(g7::install::blocked),
        help("Run g7 doctor, resolve the failing checks, and retry on a fresh Ubuntu VPS.")
    )]
    InstallBlocked { checks: String },

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
}
