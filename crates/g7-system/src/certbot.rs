use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub const CERTBOT: &str = "certbot";

pub fn certonly_webroot<R: CommandRunner>(
    runner: &R,
    webroot: &str,
    cert_name: &str,
    domains: &[String],
    email: &str,
) -> Result<CommandOutput, CommandError> {
    let mut spec = CommandSpec::new(CERTBOT)
        .arg("certonly")
        .arg("--webroot")
        .arg("-w")
        .arg(webroot)
        .arg("--cert-name")
        .arg(cert_name)
        .arg("--non-interactive")
        .arg("--agree-tos")
        .arg("--email")
        .arg(email)
        .arg("--keep-until-expiring");

    for domain in domains {
        spec = spec.arg("-d").arg(domain);
    }

    runner.run(&spec)
}

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

#[cfg(test)]
mod tests {
    use super::certonly_webroot;
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn certonly_webroot_is_noninteractive_and_shell_free()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        certonly_webroot(
            &runner,
            "/home/g7/public_html/public",
            "example.com",
            &["example.com".to_string(), "www.example.com".to_string()],
            "admin@example.com",
        )?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("certbot"));
        assert!(recorded[0].args.contains(&OsString::from("certonly")));
        assert!(recorded[0].args.contains(&OsString::from("--webroot")));
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("--non-interactive"))
        );
        assert!(recorded[0].args.contains(&OsString::from("-d")));
        Ok(())
    }
}
