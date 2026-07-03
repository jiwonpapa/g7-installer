use crate::command::{CommandError, CommandRunner, CommandSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    Free,
    InUse,
    Unknown,
}

pub fn tcp_port_status<R: CommandRunner>(
    runner: &R,
    port: u16,
) -> Result<PortStatus, CommandError> {
    let output = runner.run(&CommandSpec::new("ss").arg("-H").arg("-tulpn"))?;

    if output.status != 0 {
        return Ok(PortStatus::Unknown);
    }

    if output.stdout.lines().any(|line| line_has_port(line, port)) {
        Ok(PortStatus::InUse)
    } else {
        Ok(PortStatus::Free)
    }
}

fn line_has_port(line: &str, port: u16) -> bool {
    line.split_whitespace()
        .any(|part| part_port(part).is_some_and(|part_port| part_port == port))
}

fn part_port(part: &str) -> Option<u16> {
    let (_, suffix) = part.rsplit_once(':')?;
    let digits: String = suffix
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{PortStatus, tcp_port_status};
    use crate::command::{CommandOutput, FakeCommandRunner};

    #[test]
    fn detects_port_in_use() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(
            "tcp LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:((\"nginx\"))\n",
        ));

        let status = tcp_port_status(&runner, 80)?;

        assert_eq!(status, PortStatus::InUse);
        Ok(())
    }
}
