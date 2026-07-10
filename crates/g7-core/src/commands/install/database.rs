use super::*;

pub(super) fn apply_database_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    database_password: Option<&str>,
    smtp_password: Option<&str>,
) -> Result<Vec<InstallCheck>> {
    let sizing = detected_memory_sizing(probe);
    let db_config_path = database_config_path(plan);
    write_new_file(
        paths,
        db_config_path,
        &database_runtime_content(&sizing),
        owned,
    )?;

    let db_service = database_service_name(plan);
    let command = format!("systemctl restart {db_service}");
    let output = probe
        .restart_service(db_service)
        .map_err(|err| command_error("database-restart", &command, err))?;
    require_success("database-restart", command, output)?;

    let effective_output = probe
        .database_apply_sql(
            DatabaseEngine::from_id(&plan.database_engine),
            database_effective_sql(),
        )
        .map_err(|err| {
            command_error("database-effective-values", "read DB global variables", err)
        })?;
    require_success(
        "database-effective-values",
        "read DB global variables",
        effective_output.clone(),
    )?;
    verify_database_effective_values(&effective_output.stdout, &sizing)?;
    let database_version = fact(&parse_key_value_lines(&effective_output.stdout), "version");

    let password = match database_password {
        Some(value) => value.to_string(),
        None => random_hex_secret()?,
    };
    write_secret_file(
        paths,
        SECRETS_PATH,
        &secrets_content(plan, &password, smtp_password),
        owned,
    )?;

    let sql = database_sql(plan, &password);
    let engine = DatabaseEngine::from_id(&plan.database_engine);
    let output = probe.database_apply_sql(engine, &sql).map_err(|err| {
        command_error("database-provision", "mysql --protocol=socket -uroot", err)
    })?;
    require_success(
        "database-provision",
        "mysql --protocol=socket -uroot",
        output,
    )?;

    Ok(vec![
        InstallCheck::pass(
            "database-runtime",
            format!(
                "Created {db_config_path}; innodb_buffer_pool_size={}, max_connections={}.",
                sizing.db_buffer_pool, sizing.db_max_connections
            ),
        ),
        InstallCheck::pass(
            "database-restart",
            format!("Restarted {db_service} after DB runtime tuning."),
        ),
        InstallCheck::pass(
            "database-effective-values",
            format!(
                "Verified DB {} effective settings after restart: buffer_pool={}, max_connections={}, tmp_table_size={}.",
                database_version,
                sizing.db_buffer_pool,
                sizing.db_max_connections,
                sizing.db_tmp_table_size
            ),
        ),
        InstallCheck::pass(
            "database-secret",
            format!(
                "{} DB password and stored it root-only at {SECRETS_PATH}.",
                if database_password.is_some() {
                    "Stored user-provided"
                } else {
                    "Generated"
                }
            ),
        ),
        InstallCheck::pass(
            "smtp-secret",
            if plan.mail_mode == "smtp-relay" {
                format!("Stored SMTP credentials root-only at {SECRETS_PATH}.")
            } else {
                "SMTP credentials are not used for this install.".to_string()
            },
        ),
        InstallCheck::pass(
            "database-created",
            format!("Ensured database `{}` exists.", plan.database_name),
        ),
        InstallCheck::pass(
            "database-user-created",
            format!(
                "Ensured app DB user `{}`@`localhost` has privileges only for `{}`.",
                plan.database_user, plan.database_name
            ),
        ),
    ])
}

pub(super) fn database_effective_sql() -> &'static str {
    "SELECT CONCAT('version=', VERSION()) AS g7_value;\n\
     SELECT CONCAT('innodb_buffer_pool_size=', @@GLOBAL.innodb_buffer_pool_size) AS g7_value;\n\
     SELECT CONCAT('max_connections=', @@GLOBAL.max_connections) AS g7_value;\n\
     SELECT CONCAT('tmp_table_size=', @@GLOBAL.tmp_table_size) AS g7_value;\n\
     SELECT CONCAT('max_heap_table_size=', @@GLOBAL.max_heap_table_size) AS g7_value;\n\
     SELECT CONCAT('bind_address=', @@GLOBAL.bind_address) AS g7_value;\n"
}

pub(super) fn verify_database_effective_values(
    output: &str,
    sizing: &plan::ResolvedMemorySizing,
) -> Result<()> {
    let facts = parse_key_value_lines(output);
    let expected = [
        (
            "innodb_buffer_pool_size",
            memory_value_bytes(&sizing.db_buffer_pool)
                .unwrap_or_default()
                .to_string(),
        ),
        ("max_connections", sizing.db_max_connections.to_string()),
        (
            "tmp_table_size",
            memory_value_bytes(&sizing.db_tmp_table_size)
                .unwrap_or_default()
                .to_string(),
        ),
        (
            "max_heap_table_size",
            memory_value_bytes(&sizing.db_tmp_table_size)
                .unwrap_or_default()
                .to_string(),
        ),
        ("bind_address", "127.0.0.1".to_string()),
    ];
    let mismatches = expected
        .iter()
        .filter_map(|(key, expected)| {
            let actual = fact(&facts, key);
            if actual == *expected {
                None
            } else {
                Some(format!("{key}: expected {expected}, actual {actual}"))
            }
        })
        .collect::<Vec<_>>();
    if mismatches.is_empty() && fact(&facts, "version") != "-" {
        Ok(())
    } else {
        let version_problem = if fact(&facts, "version") == "-" {
            "version: missing; ".to_string()
        } else {
            String::new()
        };
        Err(Error::InstallVerificationFailed {
            checks: format!(
                "DB effective settings differ after restart: {version_problem}{}",
                mismatches.join("; ")
            ),
        })
    }
}

pub(super) fn apply_post_database_guidance(
    plan: &plan::InstallPlan,
) -> (
    Vec<InstallCheck>,
    Vec<InstallCheck>,
    Vec<InstallCheck>,
    Vec<InstallCheck>,
) {
    let firewall_checks = vec![InstallCheck {
        name: "network-boundary".to_string(),
        status: "manual".to_string(),
        message: "Firewall management is outside this installer. Allow the active SSH port plus 80/443 in the VPS provider or a separate maintenance app; do not expose 7717/3306/6379.".to_string(),
    }];
    let mail_checks = if plan.mail_mode == "none" {
        vec![InstallCheck {
            name: "mail-delivery".to_string(),
            status: "skipped".to_string(),
            message: "Mail delivery is disabled for this install.".to_string(),
        }]
    } else {
        vec![InstallCheck {
            name: "mail-config".to_string(),
            status: "deferred".to_string(),
            message: format!(
                "{} mail settings will be written into the app .env during app configuration.",
                plan.mail_mode
            ),
        }]
    };
    let certbot_checks = if plan.deployment_mode == "local-test" {
        vec![InstallCheck {
            name: "tls".to_string(),
            status: "skipped".to_string(),
            message: "Local test mode skips Let's Encrypt.".to_string(),
        }]
    } else {
        vec![InstallCheck {
            name: "tls".to_string(),
            status: "deferred".to_string(),
            message: "Let's Encrypt issuance will run after DNS and HTTP challenge checks in the TLS batch.".to_string(),
        }]
    };
    let app_checks = vec![InstallCheck {
        name: "app-fetch".to_string(),
        status: "deferred".to_string(),
        message: "Selected web app source fetch and .env generation will run after runtime and database are stable; HTTPS can remain deferred when Certbot is rate-limited.".to_string(),
    }];
    (firewall_checks, mail_checks, certbot_checks, app_checks)
}
