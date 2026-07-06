use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub const CERTBOT: &str = "certbot";

pub fn renew_dry_run<R: CommandRunner>(
    runner: &R,
    cert_name: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new(CERTBOT)
            .arg("renew")
            .arg("--dry-run")
            .arg("--non-interactive")
            .arg("--cert-name")
            .arg(cert_name),
    )
}
