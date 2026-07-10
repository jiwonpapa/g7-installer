use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub const CERTBOT: &str = "certbot";

pub fn certonly_webroot<R: CommandRunner>(
    runner: &R,
    webroot: &str,
    cert_name: &str,
    domains: &[String],
    email: &str,
) -> Result<CommandOutput, CommandError> {
    certonly_webroot_with_staging(
        runner,
        webroot,
        cert_name,
        domains,
        email,
        certbot_staging_enabled(),
    )
}

fn certonly_webroot_with_staging<R: CommandRunner>(
    runner: &R,
    webroot: &str,
    cert_name: &str,
    domains: &[String],
    email: &str,
    staging: bool,
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
    if staging {
        spec = spec.arg("--staging");
    }

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
            .arg("--no-random-sleep-on-renew")
            .arg("--cert-name")
            .arg(cert_name),
    )
}

pub fn reconfigure_webroot<R: CommandRunner>(
    runner: &R,
    webroot: &str,
    cert_name: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new(CERTBOT)
            .arg("reconfigure")
            .arg("--cert-name")
            .arg(cert_name)
            .arg("--webroot")
            .arg("--webroot-path")
            .arg(webroot)
            .arg("--non-interactive"),
    )
}

pub fn delete_cert<R: CommandRunner>(
    runner: &R,
    cert_name: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new(CERTBOT)
            .arg("delete")
            .arg("--cert-name")
            .arg(cert_name)
            .arg("--non-interactive"),
    )
}

fn certbot_staging_enabled() -> bool {
    std::env::var("G7_CERTBOT_STAGING")
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        certonly_webroot, certonly_webroot_with_staging, delete_cert, reconfigure_webroot,
        renew_dry_run,
    };
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

    #[test]
    fn certonly_webroot_supports_staging_opt_in()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        certonly_webroot_with_staging(
            &runner,
            "/home/g7/public_html/public",
            "example.com",
            &["example.com".to_string()],
            "admin@example.com",
            true,
        )?;
        let recorded = runner.recorded();

        assert!(recorded[0].args.contains(&OsString::from("--staging")));
        Ok(())
    }

    #[test]
    fn delete_cert_is_noninteractive_and_shell_free()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        delete_cert(&runner, "example.com")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("certbot"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("delete"),
                OsString::from("--cert-name"),
                OsString::from("example.com"),
                OsString::from("--non-interactive"),
            ]
        );
        Ok(())
    }

    #[test]
    fn renew_dry_run_disables_random_sleep() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        renew_dry_run(&runner, "example.com")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("certbot"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("renew"),
                OsString::from("--dry-run"),
                OsString::from("--non-interactive"),
                OsString::from("--no-random-sleep-on-renew"),
                OsString::from("--cert-name"),
                OsString::from("example.com"),
            ]
        );
        Ok(())
    }

    #[test]
    fn reconfigure_webroot_is_noninteractive_and_shell_free()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        reconfigure_webroot(&runner, "/home/g7/public_html/public", "example.com")?;
        let recorded = runner.recorded();
        assert_eq!(recorded[0].program, OsString::from("certbot"));
        assert!(recorded[0].args.contains(&OsString::from("reconfigure")));
        assert!(recorded[0].args.contains(&OsString::from("--webroot-path")));
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("--non-interactive"))
        );
        Ok(())
    }
}
