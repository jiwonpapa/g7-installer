//! Redis command helpers used by the installer runtime phase.

use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

pub fn config_set<R: CommandRunner>(
    runner: &R,
    key: &str,
    value: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("redis-cli")
            .arg("CONFIG")
            .arg("SET")
            .arg(key)
            .arg(value),
    )
}

pub fn config_rewrite<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("redis-cli").arg("CONFIG").arg("REWRITE"))
}

pub fn config_get<R: CommandRunner>(
    runner: &R,
    pattern: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new("redis-cli")
            .arg("--raw")
            .arg("CONFIG")
            .arg("GET")
            .arg(pattern),
    )
}

pub fn ping<R: CommandRunner>(runner: &R) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new("redis-cli").arg("--raw").arg("PING"))
}

#[cfg(test)]
mod tests {
    use super::{config_get, config_rewrite, config_set, ping};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn redis_config_commands_are_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("OK\n"));
        runner.push_output(CommandOutput::success("OK\n"));
        runner.push_output(CommandOutput::success("maxmemory\n134217728\n"));
        runner.push_output(CommandOutput::success("PONG\n"));

        config_set(&runner, "maxmemory", "128mb")?;
        config_rewrite(&runner)?;
        config_get(&runner, "maxmemory")?;
        ping(&runner)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("redis-cli"));
        assert_eq!(recorded[0].args[0], OsString::from("CONFIG"));
        assert_eq!(
            recorded[1].args,
            vec![OsString::from("CONFIG"), OsString::from("REWRITE")]
        );
        assert!(recorded[2].args.contains(&OsString::from("--raw")));
        assert_eq!(
            recorded[3].args,
            vec![OsString::from("--raw"), OsString::from("PING")]
        );
        Ok(())
    }
}
