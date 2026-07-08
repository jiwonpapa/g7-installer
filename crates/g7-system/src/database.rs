use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseEngine {
    MariaDb,
    MySql,
}

impl DatabaseEngine {
    pub fn from_id(value: &str) -> Self {
        if value == "mariadb" {
            Self::MariaDb
        } else {
            Self::MySql
        }
    }

    fn client(self) -> &'static str {
        match self {
            Self::MariaDb => "mariadb",
            Self::MySql => "mysql",
        }
    }
}

pub fn apply_sql<R: CommandRunner>(
    runner: &R,
    engine: DatabaseEngine,
    sql: &str,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new(engine.client())
            .arg("--protocol=socket")
            .arg("-uroot")
            .stdin_bytes(sql.as_bytes().to_vec()),
    )
}

#[cfg(test)]
mod tests {
    use super::{DatabaseEngine, apply_sql};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;

    #[test]
    fn database_sql_uses_stdin_without_shell() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        apply_sql(&runner, DatabaseEngine::MySql, "SELECT 1;")?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("mysql"));
        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("--protocol=socket"),
                OsString::from("-uroot")
            ]
        );
        assert_eq!(recorded[0].stdin, Some(b"SELECT 1;".to_vec()));
        Ok(())
    }
}
