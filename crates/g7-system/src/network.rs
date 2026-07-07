use std::net::IpAddr;

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub fn public_ipv4<R: CommandRunner>(runner: &R) -> Result<Option<IpAddr>, CommandError> {
    public_ip(
        runner,
        CommandSpec::new("curl")
            .arg("-4")
            .arg("-fsS")
            .arg("--max-time")
            .arg("5")
            .arg("https://api.ipify.org"),
    )
}

pub fn public_ipv6<R: CommandRunner>(runner: &R) -> Result<Option<IpAddr>, CommandError> {
    public_ip(
        runner,
        CommandSpec::new("curl")
            .arg("-6")
            .arg("-fsS")
            .arg("--max-time")
            .arg("5")
            .arg("https://api64.ipify.org"),
    )
}

pub fn dns_ipv4_records<R: CommandRunner>(
    runner: &R,
    host: &str,
) -> Result<Vec<IpAddr>, CommandError> {
    dns_records(runner, "ahostsv4", host)
}

pub fn dns_ipv6_records<R: CommandRunner>(
    runner: &R,
    host: &str,
) -> Result<Vec<IpAddr>, CommandError> {
    dns_records(runner, "ahostsv6", host)
}

pub fn tcp_connect<R: CommandRunner>(
    runner: &R,
    host: &str,
    port: u16,
) -> Result<bool, CommandError> {
    let url = format!("telnet://{host}:{port}");
    let output = runner.run(
        &CommandSpec::new("curl")
            .arg("-fsS")
            .arg("--connect-timeout")
            .arg("5")
            .arg("--max-time")
            .arg("8")
            .arg(url),
    )?;

    Ok(output.status == 0)
}

pub fn http_host_smoke<R: CommandRunner>(runner: &R, host: &str) -> Result<bool, CommandError> {
    let host_header = format!("Host: {host}");
    let output = runner.run(
        &CommandSpec::new("curl")
            .arg("-fsS")
            .arg("--max-time")
            .arg("10")
            .arg("-H")
            .arg(host_header)
            .arg("http://127.0.0.1/")
            .arg("-o")
            .arg("/dev/null"),
    )?;

    Ok(output.status == 0)
}

fn public_ip<R: CommandRunner>(
    runner: &R,
    spec: CommandSpec,
) -> Result<Option<IpAddr>, CommandError> {
    let output = runner.run(&spec)?;
    if output.status != 0 {
        return Ok(None);
    }

    Ok(output.stdout.trim().parse().ok())
}

fn dns_records<R: CommandRunner>(
    runner: &R,
    family: &str,
    host: &str,
) -> Result<Vec<IpAddr>, CommandError> {
    let output = runner.run(&CommandSpec::new("getent").arg(family).arg(host))?;
    if output.status != 0 {
        return Ok(Vec::new());
    }

    Ok(parse_getent_ips(&output))
}

fn parse_getent_ips(output: &CommandOutput) -> Vec<IpAddr> {
    let mut addresses = Vec::new();

    for line in output.stdout.lines() {
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        let Ok(address) = first.parse::<IpAddr>() else {
            continue;
        };
        if !addresses.contains(&address) {
            addresses.push(address);
        }
    }

    addresses
}

#[cfg(test)]
mod tests {
    use super::{dns_ipv4_records, http_host_smoke, public_ipv4, tcp_connect};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn parses_public_ipv4() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("203.0.113.10\n"));

        assert_eq!(
            public_ipv4(&runner)?,
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)))
        );
        Ok(())
    }

    #[test]
    fn parses_getent_dns_records_without_duplicates()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(
            "203.0.113.10 STREAM example.com\n203.0.113.10 DGRAM example.com\n",
        ));

        assert_eq!(dns_ipv4_records(&runner, "example.com")?.len(), 1);
        Ok(())
    }

    #[test]
    fn tcp_connect_reports_status() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        assert!(tcp_connect(&runner, "smtp.example.com", 587)?);
        Ok(())
    }

    #[test]
    fn http_smoke_uses_host_header_without_shell()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        assert!(http_host_smoke(&runner, "example.com")?);
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("curl"));
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("Host: example.com"))
        );
        assert!(
            recorded[0]
                .args
                .contains(&OsString::from("http://127.0.0.1/"))
        );
        Ok(())
    }
}
