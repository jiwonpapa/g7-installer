use crate::command::{CommandError, CommandOutput, CommandRunner, CommandSpec};
use std::path::Path;

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

    fn server(self) -> &'static str {
        match self {
            Self::MariaDb => "mariadbd",
            Self::MySql => "mysqld",
        }
    }
}

/// Reads the installed database server version without starting a new process instance.
pub fn server_version<R: CommandRunner>(
    runner: &R,
    engine: DatabaseEngine,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new(engine.server()).arg("--version"))
}

/// Validates every active server option file without starting the database.
pub fn config_test<R: CommandRunner>(
    runner: &R,
    engine: DatabaseEngine,
) -> Result<CommandOutput, CommandError> {
    runner.run(&CommandSpec::new(engine.server()).arg("--validate-config"))
}

/// Validates one candidate option file before it replaces an active file.
pub fn candidate_config_test<R: CommandRunner>(
    runner: &R,
    engine: DatabaseEngine,
    candidate: &Path,
) -> Result<CommandOutput, CommandError> {
    runner.run(
        &CommandSpec::new(engine.server())
            .arg(format!("--defaults-file={}", candidate.display()))
            .arg("--validate-config"),
    )
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
    use super::{DatabaseEngine, apply_sql, candidate_config_test, config_test, server_version};
    use crate::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;
    use std::path::Path;

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

    #[test]
    fn database_config_test_uses_engine_server_binary()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));

        config_test(&runner, DatabaseEngine::MySql)?;
        config_test(&runner, DatabaseEngine::MariaDb)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("mysqld"));
        assert_eq!(recorded[1].program, OsString::from("mariadbd"));
        assert!(
            recorded
                .iter()
                .all(|spec| spec.args == vec![OsString::from("--validate-config")])
        );
        Ok(())
    }

    #[test]
    fn database_server_version_is_shell_free() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("mysqld Ver 8.4.9"));

        server_version(&runner, DatabaseEngine::MySql)?;
        let recorded = runner.recorded();

        assert_eq!(recorded[0].program, OsString::from("mysqld"));
        assert_eq!(recorded[0].args, vec![OsString::from("--version")]);
        Ok(())
    }

    #[test]
    fn candidate_config_test_places_defaults_file_first()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));

        candidate_config_test(
            &runner,
            DatabaseEngine::MySql,
            Path::new("/tmp/g7-candidate.cnf"),
        )?;
        let recorded = runner.recorded();

        assert_eq!(
            recorded[0].args,
            vec![
                OsString::from("--defaults-file=/tmp/g7-candidate.cnf"),
                OsString::from("--validate-config"),
            ]
        );
        Ok(())
    }
}
