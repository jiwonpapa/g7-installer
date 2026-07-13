use super::*;

const APACHE_RUNTIME_AVAILABLE: &str = "/etc/apache2/conf-available/g7-runtime.conf";
const APACHE_RUNTIME_ENABLED: &str = "/etc/apache2/conf-enabled/g7-runtime.conf";

pub(super) fn apply_runtime_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    preinstall_package_checks: &[InstallCheck],
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    let sizing = detected_memory_sizing(probe);
    let ini_path = php_ini_override_path(plan);
    let ini_content = php_ini_override_content(&sizing);

    checks.push(InstallCheck::pass(
        "server-sizing",
        format!(
            "Detected {} MiB RAM, {} vCPU; selected {} sizing preset with {} swap.",
            sizing.total_memory_kib / 1024,
            sizing.vcpu_count,
            sizing.tier_label,
            sizing.swap_size
        ),
    ));

    if plan.web_server == "frankenphp" {
        checks.extend(install_frankenphp_app_server(probe, paths, plan, owned)?);
    }

    if plan.web_server != "frankenphp" {
        checks.push(validate_php_runtime_candidates(
            probe,
            paths,
            plan,
            &sizing,
            &ini_content,
        )?);
        if package_was_absent(
            preinstall_package_checks,
            &format!("php{}-fpm", plan.php_version),
        ) {
            checks.push(disable_default_php_fpm_pool(paths, plan)?);
        }
    }

    write_owned_file(paths, &ini_path, &ini_content, owned)?;
    checks.push(InstallCheck::pass(
        "php-runtime-ini",
        format!("Created PHP runtime override at {ini_path}."),
    ));

    if matches!(plan.web_server.as_str(), "nginx" | "frankenphp") {
        checks.push(apply_nginx_worker_tuning(paths, &sizing, owned)?);
    } else if plan.web_server == "apache" {
        write_owned_file(
            paths,
            APACHE_RUNTIME_AVAILABLE,
            &apache_mpm_runtime_content(&sizing),
            owned,
        )?;
        if paths.resolve(APACHE_RUNTIME_ENABLED).exists() {
            if !owned.iter().any(|path| path == APACHE_RUNTIME_ENABLED) {
                return Err(Error::InstallVerificationFailed {
                    checks: format!(
                        "{APACHE_RUNTIME_ENABLED} already exists and is not installer-owned"
                    ),
                });
            }
        } else {
            create_owned_symlink(
                paths,
                APACHE_RUNTIME_AVAILABLE,
                APACHE_RUNTIME_ENABLED,
                owned,
            )?;
        }
        checks.push(InstallCheck::pass(
            "apache-mpm-event-runtime",
            format!(
                "Applied Apache event MPM tuning at {APACHE_RUNTIME_AVAILABLE}; MaxRequestWorkers={}, ThreadsPerChild={}.",
                sizing.apache_max_request_workers, sizing.apache_threads_per_child
            ),
        ));
    }

    if plan.web_server == "frankenphp" {
        write_owned_file(
            paths,
            g7_system::nginx::G7_SITE_AVAILABLE,
            &nginx_frankenphp_vhost_content(plan),
            owned,
        )?;
        checks.push(InstallCheck::pass(
            "frankenphp-runtime",
            format!(
                "FrankenPHP runs PHP requests on {} behind the Nginx edge vhost.",
                FRANKENPHP_LISTEN
            ),
        ));
    } else {
        let session_path = php_session_path(plan);
        create_owned_dir_if_absent(paths, &session_path, owned)?;
        let session_owner = format!("{}:www-data", plan.site_user);
        let output = probe
            .chown_recursive(&session_owner, &session_path)
            .map_err(|err| command_error("php-session-owner", &session_path, err))?;
        require_success("php-session-owner", &session_path, output)?;
        let output = probe
            .chmod_path("0700", &session_path)
            .map_err(|err| command_error("php-session-permissions", &session_path, err))?;
        require_success("php-session-permissions", &session_path, output)?;
        checks.push(InstallCheck::pass(
            "php-session-path",
            format!("Created site-only PHP session directory at {session_path}."),
        ));

        let pool_path = php_pool_path(plan);
        write_owned_file(paths, &pool_path, &php_pool_content(plan, &sizing), owned)?;
        checks.push(InstallCheck::pass(
            "php-fpm-pool",
            format!(
                "Created PHP-FPM pool config at {pool_path}; pm={}, max_children={}, memory_limit={}.",
                sizing.php_process_manager, sizing.php_max_children, sizing.php_memory_limit
            ),
        ));
    }

    if plan.web_server == "nginx" {
        write_owned_file(
            paths,
            g7_system::nginx::G7_SITE_AVAILABLE,
            &nginx_vhost_content_with_socket_and_sizing(
                plan,
                &php_fpm_site_socket(plan),
                Some(&sizing),
            ),
            owned,
        )?;
        checks.push(InstallCheck::pass(
            "nginx-fastcgi-runtime",
            format!(
                "Updated Nginx vhost to use site PHP-FPM socket {}.",
                php_fpm_site_socket(plan)
            ),
        ));
        checks.push(InstallCheck {
            name: "nginx-worker-mode".to_string(),
            status: "pass".to_string(),
            message: format!(
                "Applied nginx.conf values: worker_processes={}, worker_connections={}, rlimit_nofile={} with original backup at {}.",
                sizing.nginx_worker_processes,
                sizing.nginx_worker_connections,
                sizing.nginx_worker_rlimit_nofile,
                NGINX_MAIN_BACKUP_PATH
            ),
        });
    } else if plan.web_server == "apache" {
        write_owned_file(
            paths,
            g7_system::apache::G7_SITE_AVAILABLE,
            &apache_vhost_content_with_socket(plan, &php_fpm_site_socket(plan)),
            owned,
        )?;
        checks.push(InstallCheck::pass(
            "apache-proxy-fcgi-runtime",
            format!(
                "Updated Apache vhost to use site PHP-FPM socket {}.",
                php_fpm_site_socket(plan)
            ),
        ));
        checks.push(InstallCheck {
            name: "apache-worker-mode".to_string(),
            status: "pass".to_string(),
            message: format!(
                "Applied Apache mpm_event MaxRequestWorkers={} with PHP-FPM pool max_children={}.",
                sizing.apache_max_request_workers, sizing.php_max_children
            ),
        });
    }

    if plan.web_server == "frankenphp" {
        let output = probe
            .restart_service(FRANKENPHP_SERVICE_NAME)
            .map_err(|err| {
                command_error(
                    "frankenphp-restart",
                    format!("systemctl restart {FRANKENPHP_SERVICE_NAME}"),
                    err,
                )
            })?;
        require_success(
            "frankenphp-restart",
            format!("systemctl restart {FRANKENPHP_SERVICE_NAME}"),
            output,
        )?;
        checks.push(InstallCheck::pass(
            "frankenphp-restart",
            format!("Restarted {FRANKENPHP_SERVICE_NAME}."),
        ));
    } else {
        let fpm_service = format!("php{}-fpm", plan.php_version);
        let test_command = format!("php-fpm{} -t", plan.php_version);
        let output = probe
            .php_fpm_config_test(&plan.php_version)
            .map_err(|err| command_error("php-fpm-configtest", &test_command, err))?;
        require_success("php-fpm-configtest", &test_command, output)?;
        checks.push(InstallCheck::pass(
            "php-fpm-configtest",
            format!("{test_command}로 PHP-FPM 설정 문법을 검증했습니다."),
        ));
        let output = probe.reload_service(&fpm_service).map_err(|err| {
            command_error(
                "php-fpm-reload",
                format!("systemctl reload {fpm_service}"),
                err,
            )
        })?;
        require_success(
            "php-fpm-reload",
            format!("systemctl reload {fpm_service}"),
            output,
        )?;
        checks.push(InstallCheck::pass(
            "php-fpm-reload",
            format!("Reloaded {fpm_service}."),
        ));
    }

    if matches!(plan.web_server.as_str(), "nginx" | "frankenphp") {
        let output = probe
            .nginx_config_test()
            .map_err(|err| command_error("nginx-configtest", "nginx -t", err))?;
        require_success("nginx-configtest", "nginx -t", output)?;
        let output = probe
            .reload_service(g7_system::nginx::SERVICE_NAME)
            .map_err(|err| command_error("nginx-reload", "systemctl reload nginx", err))?;
        require_success("nginx-reload", "systemctl reload nginx", output)?;
        checks.push(InstallCheck::pass(
            if plan.web_server == "frankenphp" {
                "frankenphp-edge-runtime-reload"
            } else {
                "nginx-runtime-reload"
            },
            if plan.web_server == "frankenphp" {
                "Validated and reloaded Nginx edge after FrankenPHP runtime tuning."
            } else {
                "Validated and reloaded Nginx after runtime tuning."
            },
        ));
    } else {
        let output = probe
            .apache_config_test()
            .map_err(|err| command_error("apache-configtest", "apache2ctl configtest", err))?;
        require_success("apache-configtest", "apache2ctl configtest", output)?;
        let output = probe
            .reload_service(g7_system::apache::SERVICE_NAME)
            .map_err(|err| command_error("apache-reload", "systemctl reload apache2", err))?;
        require_success("apache-reload", "systemctl reload apache2", output)?;
        checks.push(InstallCheck::pass(
            "apache-runtime-reload",
            "Validated and reloaded Apache after runtime tuning.",
        ));
    }

    if plan.redis_mode == "enable" {
        checks.extend(apply_redis_runtime(probe, &sizing)?);
    }

    checks.extend(php_runtime_diagnostic_checks(probe, paths, plan, &sizing));

    Ok(checks)
}

fn default_php_fpm_pool_path(plan: &plan::InstallPlan) -> String {
    format!("/etc/php/{}/fpm/pool.d/www.conf", plan.php_version)
}

pub(super) fn disable_default_php_fpm_pool(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
) -> Result<InstallCheck> {
    let path = default_php_fpm_pool_path(plan);
    let target = paths.resolve(&path);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(&target).map_err(|source| Error::FileRemoveFailed {
                path: path.clone(),
                source,
            })?;
            Ok(InstallCheck::pass(
                "php-fpm-default-pool-disabled",
                format!(
                    "신규 PHP-FPM 설치의 기본 www 풀 `{path}`을 비활성화하고 사이트 전용 풀만 사용합니다."
                ),
            ))
        }
        Ok(_) => Err(Error::InstallVerificationFailed {
            checks: format!("기본 PHP-FPM 풀 경로가 일반 파일이 아닙니다: {path}"),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(InstallCheck::pass(
            "php-fpm-default-pool-disabled",
            "기본 PHP-FPM www 풀은 이미 비활성화되어 있습니다.",
        )),
        Err(source) => Err(Error::FileReadFailed { path, source }),
    }
}

fn validate_php_runtime_candidates<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    sizing: &plan::ResolvedMemorySizing,
    ini_content: &str,
) -> Result<InstallCheck> {
    let ini_candidate = format!("{CANDIDATE_DIR}/php-{}.ini", plan.site_user);
    let pool_candidate = format!("{CANDIDATE_DIR}/php-{}-pool.conf", plan.site_user);
    let fpm_candidate = format!("{CANDIDATE_DIR}/php-{}-fpm.conf", plan.site_user);
    let ini_path = write_validation_candidate(paths, &ini_candidate, ini_content)?;
    let pool_path =
        write_validation_candidate(paths, &pool_candidate, &php_pool_content(plan, sizing))?;
    let global_content = format!(
        "[global]\nerror_log = /dev/stderr\ninclude = {}\n",
        pool_path.display()
    );
    let fpm_path = write_validation_candidate(paths, &fpm_candidate, &global_content)?;
    let command = format!(
        "php-fpm{} -y {} -c {} -t",
        plan.php_version,
        fpm_path.display(),
        ini_path.display()
    );
    let validation = probe.php_fpm_candidate_config_test(&plan.php_version, &fpm_path, &ini_path);
    remove_validation_candidates(paths, &[&ini_candidate, &pool_candidate, &fpm_candidate])?;
    let output =
        validation.map_err(|err| command_error("php-fpm-candidate-test", &command, err))?;
    require_success("php-fpm-candidate-test", &command, output)?;

    Ok(InstallCheck::pass(
        "php-fpm-candidate-test",
        "PHP ini와 FPM pool 후보 파일을 활성 설정 교체 전에 검증했습니다.",
    ))
}

pub(super) fn php_runtime_diagnostic_checks<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    sizing: &plan::ResolvedMemorySizing,
) -> Vec<InstallCheck> {
    let mut checks = Vec::new();
    let runtime_command = if plan.web_server == "frankenphp" {
        php_runtime_probe_command(plan)
    } else {
        php_fpm_info_command(plan)
    };
    let output = match probe.runner().run(&runtime_command) {
        Ok(output) => output,
        Err(error) => {
            checks.push(InstallCheck::fail(
                "php-runtime-probe",
                format!("PHP 런타임 정보를 실행하지 못했습니다: {error}"),
            ));
            return checks;
        }
    };

    if output.status != 0 {
        checks.push(InstallCheck::fail(
            "php-runtime-probe",
            format!(
                "PHP 런타임 정보 수집 실패: status={} stdout={} stderr={}",
                output.status,
                short_text(&output.stdout),
                short_text(&output.stderr)
            ),
        ));
        return checks;
    }

    let mut facts = if plan.web_server == "frankenphp" {
        parse_key_value_lines(&output.stdout)
    } else {
        parse_php_fpm_info(&output.stdout)
    };
    if plan.web_server != "frankenphp" {
        let extensions = match probe.runner().run(&php_runtime_probe_command(plan)) {
            Ok(output) if output.status == 0 => {
                fact(&parse_key_value_lines(&output.stdout), "extensions")
            }
            Ok(output) => {
                checks.push(InstallCheck::fail(
                    "php-runtime-probe",
                    format!(
                        "PHP 확장 정보 수집 실패: status={} stderr={}",
                        output.status,
                        short_text(&output.stderr)
                    ),
                ));
                return checks;
            }
            Err(error) => {
                checks.push(InstallCheck::fail(
                    "php-runtime-probe",
                    format!("PHP 확장 정보를 실행하지 못했습니다: {error}"),
                ));
                return checks;
            }
        };
        facts.push(("extensions".to_string(), extensions));
    }
    checks.push(InstallCheck::pass(
        "phpinfo-summary",
        format!(
            "{} 기준 PHP 정보를 파싱했습니다: PHP {}, SAPI={}, ini={}, scan_dir={}, timezone={}.",
            if plan.web_server == "frankenphp" {
                "CLI ini"
            } else {
                "FPM ini"
            },
            fact(&facts, "php_version"),
            fact(&facts, "sapi"),
            fact(&facts, "loaded_ini"),
            fact(&facts, "scan_dir"),
            fact(&facts, "date.timezone")
        ),
    ));

    let limits = [
        ("memory_limit", sizing.php_memory_limit.as_str()),
        ("upload_max_filesize", sizing.php_upload_limit.as_str()),
        ("post_max_size", sizing.php_post_limit.as_str()),
        (
            "opcache.memory_consumption",
            sizing.opcache_memory.trim_end_matches('M'),
        ),
        ("opcache.validate_timestamps", "1"),
        ("opcache.enable_file_override", "0"),
    ];
    let mismatches = limits
        .iter()
        .filter_map(|(key, expected)| {
            let actual = fact(&facts, key);
            if normalize_php_value(&actual) == normalize_php_value(expected) {
                None
            } else {
                Some(format!("{key}: expected {expected}, actual {actual}"))
            }
        })
        .collect::<Vec<_>>();
    checks.push(if mismatches.is_empty() {
        InstallCheck::pass(
            "php-runtime-limits",
            format!(
                "PHP 한도 적용 확인: memory_limit={}, upload_max_filesize={}, post_max_size={}, max_execution_time={}, max_input_vars={}, opcache.memory_consumption={}.",
                fact(&facts, "memory_limit"),
                fact(&facts, "upload_max_filesize"),
                fact(&facts, "post_max_size"),
                fact(&facts, "max_execution_time"),
                fact(&facts, "max_input_vars"),
                fact(&facts, "opcache.memory_consumption")
            ),
        )
    } else {
        InstallCheck::fail(
            "php-runtime-limits",
            format!("PHP 설정값이 설치 계획과 다릅니다: {}.", mismatches.join("; ")),
        )
    });

    let loaded_extensions = fact(&facts, "extensions")
        .split(',')
        .map(|extension| extension.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    for extension in required_php_extensions(plan) {
        let present = loaded_extensions.iter().any(|loaded| loaded == extension);
        checks.push(if present {
            InstallCheck::pass(
                format!("php-extension:{extension}"),
                format!("PHP 확장 {extension} 로드 확인."),
            )
        } else {
            InstallCheck::fail(
                format!("php-extension:{extension}"),
                format!("PHP 확장 {extension} 이 로드되지 않았습니다. 앱 설치 전에 패키지/ini 설정을 확인하세요."),
            )
        });
    }

    if plan.web_server == "frankenphp" {
        checks.push(InstallCheck::pass(
            "frankenphp-runtime-boundary",
            format!(
                "FrankenPHP app server listens on {}; Nginx edge owns public 80/443.",
                FRANKENPHP_LISTEN
            ),
        ));
    } else {
        checks.push(php_fpm_pool_value_check(paths, plan, sizing));
    }
    checks
}

pub(super) fn php_runtime_probe_command(plan: &plan::InstallPlan) -> CommandSpec {
    let sapi = if plan.web_server == "frankenphp" {
        "cli"
    } else {
        "fpm"
    };
    CommandSpec::new("env")
        .arg(format!(
            "PHP_INI_SCAN_DIR=/etc/php/{}/{sapi}/conf.d",
            plan.php_version,
        ))
        .arg(format!("php{}", plan.php_version))
        .arg("-c")
        .arg(format!("/etc/php/{}/{sapi}/php.ini", plan.php_version))
        .arg("-r")
        .arg(php_runtime_probe_script())
}

pub(super) fn php_fpm_info_command(plan: &plan::InstallPlan) -> CommandSpec {
    CommandSpec::new(format!("php-fpm{}", plan.php_version)).arg("-i")
}

pub(super) fn php_runtime_probe_script() -> &'static str {
    r#"
echo "php_version=".PHP_VERSION."\n";
echo "sapi=".PHP_SAPI."\n";
echo "loaded_ini=".(php_ini_loaded_file() ?: "-")."\n";
echo "scan_dir=".(getenv("PHP_INI_SCAN_DIR") ?: "-")."\n";
foreach (["memory_limit","upload_max_filesize","post_max_size","max_execution_time","max_input_vars","date.timezone","realpath_cache_size","realpath_cache_ttl","opcache.enable","opcache.memory_consumption","opcache.validate_timestamps","opcache.enable_file_override"] as $key) {
    $value = ini_get($key);
    echo $key."=".($value === false ? "-" : $value)."\n";
}
echo "extensions=".implode(",", array_map("strtolower", get_loaded_extensions()))."\n";
"#
}

pub(super) fn php_fpm_pool_value_check(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    sizing: &plan::ResolvedMemorySizing,
) -> InstallCheck {
    let path = php_pool_path(plan);
    let content = match fs::read_to_string(paths.resolve(&path)) {
        Ok(content) => content,
        Err(error) => {
            return InstallCheck::fail(
                "php-fpm-pool-values",
                format!("{path} 파일을 읽지 못했습니다: {error}"),
            );
        }
    };

    let expected = [
        ("user", plan.site_user.clone()),
        ("group", "www-data".to_string()),
        ("pm", sizing.php_process_manager.clone()),
        ("pm.max_children", sizing.php_max_children.to_string()),
        ("pm.max_requests", "500".to_string()),
    ];
    let mismatches = expected
        .iter()
        .filter_map(|(key, expected)| {
            let actual = pool_value(&content, key).unwrap_or_else(|| "-".to_string());
            if actual == *expected {
                None
            } else {
                Some(format!("{key}: expected {expected}, actual {actual}"))
            }
        })
        .collect::<Vec<_>>();

    if mismatches.is_empty() {
        InstallCheck::pass(
            "php-fpm-pool-values",
            format!(
                "PHP-FPM pool 확인: user={}, group=www-data, pm={}, max_children={}, max_requests=500.",
                plan.site_user, sizing.php_process_manager, sizing.php_max_children
            ),
        )
    } else {
        InstallCheck::fail(
            "php-fpm-pool-values",
            format!(
                "PHP-FPM pool 설정값이 설치 계획과 다릅니다: {}.",
                mismatches.join("; ")
            ),
        )
    }
}

pub(super) fn required_php_extensions(plan: &plan::InstallPlan) -> Vec<&'static str> {
    let mut extensions = match crate::app_profile::resolve_app_profile(&plan.app_profile) {
        Ok(profile) => profile.php_extensions.to_vec(),
        Err(_) => vec![
            "curl",
            "fileinfo",
            "mbstring",
            "openssl",
            "pdo_mysql",
            "xml",
            "zip",
        ],
    };
    if plan.redis_mode == "enable" {
        extensions.push("redis");
    }
    extensions.sort_unstable();
    extensions.dedup();
    extensions
}

pub(super) fn parse_key_value_lines(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

pub(super) fn parse_php_fpm_info(output: &str) -> Vec<(String, String)> {
    const FIELDS: [(&str, &str); 16] = [
        ("php version", "php_version"),
        ("server api", "sapi"),
        ("loaded configuration file", "loaded_ini"),
        ("scan this dir for additional .ini files", "scan_dir"),
        ("memory_limit", "memory_limit"),
        ("upload_max_filesize", "upload_max_filesize"),
        ("post_max_size", "post_max_size"),
        ("max_execution_time", "max_execution_time"),
        ("max_input_vars", "max_input_vars"),
        ("date.timezone", "date.timezone"),
        ("realpath_cache_size", "realpath_cache_size"),
        ("realpath_cache_ttl", "realpath_cache_ttl"),
        ("opcache.enable", "opcache.enable"),
        ("opcache.memory_consumption", "opcache.memory_consumption"),
        ("opcache.validate_timestamps", "opcache.validate_timestamps"),
        (
            "opcache.enable_file_override",
            "opcache.enable_file_override",
        ),
    ];

    let mut facts = Vec::new();
    for line in output.lines() {
        let Some((name, _value)) = line.split_once("=>") else {
            continue;
        };
        let normalized_name = name.trim().to_ascii_lowercase();
        let Some((_, key)) = FIELDS
            .iter()
            .find(|(label, _key)| *label == normalized_name)
        else {
            continue;
        };
        if facts.iter().any(|(existing, _)| existing == key) {
            continue;
        }
        let value = line
            .rsplit_once("=>")
            .map(|(_name, value)| value.trim())
            .unwrap_or("-");
        facts.push(((*key).to_string(), value.to_string()));
    }
    facts
}

pub(super) fn fact(facts: &[(String, String)], key: &str) -> String {
    facts
        .iter()
        .find(|(name, _value)| name == key)
        .map(|(_name, value)| value.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".to_string())
}

pub(super) fn normalize_php_value(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => "1".to_string(),
        "off" => "0".to_string(),
        value => value.to_string(),
    }
}

pub(super) fn short_text(value: &str) -> String {
    let text = value.trim().replace('\n', " ");
    if text.chars().count() > 240 {
        format!("{}...", text.chars().take(240).collect::<String>())
    } else {
        text
    }
}

pub(super) fn short_tail_text(value: &str) -> String {
    tail_text(value, 800)
}

pub(super) fn tail_text(value: &str, limit: usize) -> String {
    let text = value.trim().replace('\n', " ");
    let length = text.chars().count();
    if length > limit {
        format!(
            "...{}",
            text.chars().skip(length - limit).collect::<String>()
        )
    } else {
        text
    }
}

pub(super) fn pool_value(content: &str, key: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('[') {
            return None;
        }
        let (name, value) = line.split_once('=')?;
        if name.trim() == key {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

pub(super) fn blocking_runtime_failure(checks: &[InstallCheck]) -> Option<String> {
    let failures = checks
        .iter()
        .filter(|check| {
            check.status == "fail"
                && (check.name == "php-runtime-probe"
                    || check.name == "php-runtime-limits"
                    || check.name == "php-fpm-pool-values"
                    || check.name == "redis-runtime-values"
                    || check.name.starts_with("php-extension:"))
        })
        .map(|check| format!("{} - {}", check.name, check.message))
        .collect::<Vec<_>>();

    if failures.is_empty() {
        None
    } else {
        Some(format!(
            "서버 런타임 검증 실패. 웹앱 설치를 시작하지 않습니다: {}",
            failures.join("; ")
        ))
    }
}

pub(super) fn apply_redis_runtime<R: CommandRunner>(
    probe: &SystemProbe<R>,
    sizing: &plan::ResolvedMemorySizing,
) -> Result<Vec<InstallCheck>> {
    let previous = redis_runtime_snapshot(probe)?;
    match apply_redis_runtime_inner(probe, sizing) {
        Ok(checks) if checks.iter().all(|check| check.status != "fail") => Ok(checks),
        Ok(mut checks) => {
            restore_redis_runtime(probe, &previous)?;
            checks.push(InstallCheck::pass(
                "redis-runtime-restore",
                "Redis 검증 실패로 변경 전 설정을 자동 복원했습니다.",
            ));
            Ok(checks)
        }
        Err(error) => {
            if let Err(restore_error) = restore_redis_runtime(probe, &previous) {
                return Err(Error::InstallVerificationFailed {
                    checks: format!(
                        "Redis 적용 실패 후 기존 설정 복원도 실패했습니다: apply={error}; restore={restore_error}"
                    ),
                });
            }
            Err(error)
        }
    }
}

fn apply_redis_runtime_inner<R: CommandRunner>(
    probe: &SystemProbe<R>,
    sizing: &plan::ResolvedMemorySizing,
) -> Result<Vec<InstallCheck>> {
    let settings = [
        ("bind", "127.0.0.1".to_string()),
        ("protected-mode", "yes".to_string()),
        (
            "maxmemory",
            sizing.redis_maxmemory.trim().to_ascii_lowercase(),
        ),
        ("maxmemory-policy", "volatile-lru".to_string()),
    ];

    for (key, value) in &settings {
        let output = probe.redis_config_set(key, value).map_err(|error| {
            command_error("redis-config-set", format!("CONFIG SET {key}"), error)
        })?;
        require_success("redis-config-set", format!("CONFIG SET {key}"), output)?;
    }
    let output = probe
        .redis_config_rewrite()
        .map_err(|error| command_error("redis-config-rewrite", "CONFIG REWRITE", error))?;
    require_success("redis-config-rewrite", "CONFIG REWRITE", output)?;
    let output = probe
        .restart_service("redis-server")
        .map_err(|error| command_error("redis-restart", "systemctl restart redis-server", error))?;
    require_success("redis-restart", "systemctl restart redis-server", output)?;

    let expected_maxmemory =
        redis_memory_value_bytes(&sizing.redis_maxmemory).ok_or_else(|| {
            Error::InstallVerificationFailed {
                checks: format!(
                    "Redis maxmemory value `{}` could not be converted to bytes",
                    sizing.redis_maxmemory
                ),
            }
        })?;
    let expected = [
        ("bind", "127.0.0.1".to_string()),
        ("protected-mode", "yes".to_string()),
        ("maxmemory", expected_maxmemory.to_string()),
        ("maxmemory-policy", "volatile-lru".to_string()),
    ];
    let mut mismatches = Vec::new();
    for (key, expected_value) in &expected {
        let output = probe.redis_config_get(key).map_err(|error| {
            command_error("redis-config-get", format!("CONFIG GET {key}"), error)
        })?;
        require_success(
            "redis-config-get",
            format!("CONFIG GET {key}"),
            output.clone(),
        )?;
        let actual = redis_config_value(&output.stdout, key).unwrap_or_else(|| "-".to_string());
        if actual != *expected_value {
            mismatches.push(format!("{key}: expected {expected_value}, actual {actual}"));
        }
    }

    if !mismatches.is_empty() {
        return Ok(vec![InstallCheck::fail(
            "redis-runtime-values",
            format!(
                "Redis effective settings differ from the install plan: {}.",
                mismatches.join("; ")
            ),
        )]);
    }

    Ok(vec![
        InstallCheck::pass(
            "redis-runtime-config",
            "Applied local-only Redis settings, persisted them with CONFIG REWRITE, and restarted Redis.",
        ),
        InstallCheck::pass(
            "redis-runtime-values",
            format!(
                "Verified Redis effective settings: bind=127.0.0.1, protected-mode=yes, maxmemory={}, policy=volatile-lru.",
                sizing.redis_maxmemory
            ),
        ),
    ])
}

fn redis_runtime_snapshot<R: CommandRunner>(
    probe: &SystemProbe<R>,
) -> Result<Vec<(String, String)>> {
    ["bind", "protected-mode", "maxmemory", "maxmemory-policy"]
        .iter()
        .map(|key| {
            let output = probe.redis_config_get(key).map_err(|error| {
                command_error("redis-snapshot", format!("CONFIG GET {key}"), error)
            })?;
            require_success(
                "redis-snapshot",
                format!("CONFIG GET {key}"),
                output.clone(),
            )?;
            let value = redis_config_value(&output.stdout, key).ok_or_else(|| {
                Error::InstallVerificationFailed {
                    checks: format!("Redis snapshot did not return `{key}`"),
                }
            })?;
            Ok(((*key).to_string(), value))
        })
        .collect()
}

fn restore_redis_runtime<R: CommandRunner>(
    probe: &SystemProbe<R>,
    previous: &[(String, String)],
) -> Result<()> {
    for (key, value) in previous {
        let output = probe
            .redis_config_set(key, value)
            .map_err(|error| command_error("redis-restore", format!("CONFIG SET {key}"), error))?;
        require_success("redis-restore", format!("CONFIG SET {key}"), output)?;
    }
    let output = probe
        .redis_config_rewrite()
        .map_err(|error| command_error("redis-restore-rewrite", "CONFIG REWRITE", error))?;
    require_success("redis-restore-rewrite", "CONFIG REWRITE", output)?;
    let output = probe.restart_service("redis-server").map_err(|error| {
        command_error(
            "redis-restore-restart",
            "systemctl restart redis-server",
            error,
        )
    })?;
    require_success(
        "redis-restore-restart",
        "systemctl restart redis-server",
        output,
    )
}

pub(super) fn redis_config_value(output: &str, key: &str) -> Option<String> {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    while let Some(name) = lines.next() {
        let value = lines.next()?;
        if name == key {
            return Some(value.to_string());
        }
    }
    None
}

pub(super) fn memory_value_bytes(value: &str) -> Option<u64> {
    let normalized = value.trim().to_ascii_lowercase();
    let digits = normalized
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<u64>()
        .ok()?;
    let multiplier = if normalized.ends_with('g') || normalized.ends_with("gb") {
        1024_u64.pow(3)
    } else if normalized.ends_with('m') || normalized.ends_with("mb") {
        1024_u64.pow(2)
    } else if normalized.ends_with('k') || normalized.ends_with("kb") {
        1024
    } else {
        1
    };
    digits.checked_mul(multiplier)
}

pub(super) fn redis_memory_value_bytes(value: &str) -> Option<u64> {
    let normalized = value.trim().to_ascii_lowercase();
    let digits = normalized
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<u64>()
        .ok()?;
    let multiplier = if normalized.ends_with('g') || normalized.ends_with("gb") {
        1000_u64.pow(3)
    } else if normalized.ends_with('m') || normalized.ends_with("mb") {
        1000_u64.pow(2)
    } else if normalized.ends_with('k') || normalized.ends_with("kb") {
        1000
    } else {
        1
    };
    digits.checked_mul(multiplier)
}

pub(super) fn detected_memory_sizing<R: CommandRunner>(
    probe: &SystemProbe<R>,
) -> plan::ResolvedMemorySizing {
    let total_memory_kib = probe
        .total_memory_kib()
        .ok()
        .flatten()
        .unwrap_or(1024 * 1024);
    let vcpu_count = probe.vcpu_count().ok().flatten().unwrap_or(1);
    plan::resolve_memory_sizing(total_memory_kib, vcpu_count)
}

pub(super) fn apply_nginx_worker_tuning(
    paths: &InstallPaths,
    sizing: &plan::ResolvedMemorySizing,
    owned: &mut Vec<String>,
) -> Result<InstallCheck> {
    let source = paths.resolve(NGINX_MAIN_CONFIG_PATH);
    let content = fs::read_to_string(&source).map_err(|source| Error::FileReadFailed {
        path: NGINX_MAIN_CONFIG_PATH.to_string(),
        source,
    })?;
    let tuned = nginx_main_runtime_content(&content, sizing)?;
    let backup = paths.resolve(NGINX_MAIN_BACKUP_PATH);
    if !backup.exists() {
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
                path: parent.display().to_string(),
                source,
            })?;
        }
        fs::copy(&source, &backup).map_err(|source| Error::FileWriteFailed {
            path: NGINX_MAIN_BACKUP_PATH.to_string(),
            source,
        })?;
        owned.push(NGINX_MAIN_BACKUP_PATH.to_string());
    } else if !owned.iter().any(|path| path == NGINX_MAIN_BACKUP_PATH) {
        return Err(Error::InstallVerificationFailed {
            checks: format!("{NGINX_MAIN_BACKUP_PATH} exists without installer ownership metadata"),
        });
    }
    write_existing_file(paths, NGINX_MAIN_CONFIG_PATH, &tuned)?;
    Ok(InstallCheck::pass(
        "nginx-main-runtime",
        format!(
            "Tuned {NGINX_MAIN_CONFIG_PATH} and preserved its original at {NGINX_MAIN_BACKUP_PATH}."
        ),
    ))
}

pub(super) fn nginx_main_runtime_content(
    content: &str,
    sizing: &plan::ResolvedMemorySizing,
) -> Result<String> {
    let has_rlimit = content
        .lines()
        .any(|line| line.trim_start().starts_with("worker_rlimit_nofile "));
    let mut found_processes = false;
    let mut found_connections = false;
    let mut found_server_tokens = false;
    let mut in_events = false;
    let mut in_http = false;
    let mut output = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("events") && trimmed.contains('{') {
            in_events = true;
        }
        if trimmed.starts_with("http") && trimmed.contains('{') {
            in_http = true;
            output.push(line.to_string());
            output.push("    server_tokens off;".to_string());
            found_server_tokens = true;
            continue;
        }
        if trimmed.starts_with("worker_processes ") {
            let indent = &line[..line.len() - line.trim_start().len()];
            output.push(format!(
                "{indent}worker_processes {};",
                sizing.nginx_worker_processes
            ));
            if !has_rlimit {
                output.push(format!(
                    "{indent}worker_rlimit_nofile {};",
                    sizing.nginx_worker_rlimit_nofile
                ));
            }
            found_processes = true;
            continue;
        }
        if trimmed.starts_with("worker_rlimit_nofile ") {
            let indent = &line[..line.len() - line.trim_start().len()];
            output.push(format!(
                "{indent}worker_rlimit_nofile {};",
                sizing.nginx_worker_rlimit_nofile
            ));
            continue;
        }
        if in_events && trimmed.starts_with("worker_connections ") {
            let indent = &line[..line.len() - line.trim_start().len()];
            output.push(format!(
                "{indent}worker_connections {};",
                sizing.nginx_worker_connections
            ));
            found_connections = true;
            continue;
        }
        if in_http
            && (trimmed.starts_with("server_tokens ") || trimmed.starts_with("# server_tokens "))
        {
            if !found_server_tokens {
                let indent = &line[..line.len() - line.trim_start().len()];
                output.push(format!("{indent}server_tokens off;"));
                found_server_tokens = true;
            }
            continue;
        }
        output.push(line.to_string());
        if in_events && trimmed == "}" {
            in_events = false;
        }
        if in_http && trimmed == "}" {
            in_http = false;
        }
    }

    if !found_processes || !found_connections || !found_server_tokens {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "{NGINX_MAIN_CONFIG_PATH} does not contain expected worker_processes/events/http directives"
            ),
        });
    }
    let mut content = output.join("\n");
    content.push('\n');
    Ok(content)
}

pub(super) fn apache_mpm_runtime_content(sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        "# Managed by g7inst.\n<IfModule mpm_event_module>\n    StartServers {}\n    ServerLimit {}\n    ThreadsPerChild {}\n    MaxRequestWorkers {}\n    MinSpareThreads {}\n    MaxSpareThreads {}\n    MaxConnectionsPerChild {}\n</IfModule>\n",
        sizing.apache_start_servers,
        sizing.apache_server_limit,
        sizing.apache_threads_per_child,
        sizing.apache_max_request_workers,
        sizing.apache_min_spare_threads,
        sizing.apache_max_spare_threads,
        sizing.apache_max_connections_per_child,
    )
}

pub(super) fn apply_swap_configuration<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    sizing: &plan::ResolvedMemorySizing,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    write_managed_marker_file(
        paths,
        SWAP_UNIT_PATH,
        &swap_unit_content(),
        "G7 Installer managed swapfile",
        owned,
    )?;
    write_managed_marker_file(
        paths,
        SWAP_SYSCTL_PATH,
        swap_sysctl_content(),
        "Managed by g7inst.",
        owned,
    )?;

    if paths.resolve("/") != Path::new("/") {
        let swap_path = paths.resolve(SWAP_FILE_PATH);
        fs::write(
            &swap_path,
            format!("g7inst simulated {}\n", sizing.swap_size),
        )
        .map_err(|source| Error::FileWriteFailed {
            path: SWAP_FILE_PATH.to_string(),
            source,
        })?;
        owned.push(SWAP_FILE_PATH.to_string());
        checks.push(InstallCheck::pass(
            "swapfile",
            format!(
                "Prepared managed {} swapfile at {SWAP_FILE_PATH} with systemd unit {SWAP_UNIT_PATH}.",
                sizing.swap_size
            ),
        ));
        checks.push(InstallCheck::pass(
            "swap-sysctl",
            format!("Prepared swap sysctl policy at {SWAP_SYSCTL_PATH}."),
        ));
        return Ok(checks);
    }

    let swap_unit = paths.resolve(SWAP_UNIT_PATH);
    let verify_command = format!("systemd-analyze verify {}", swap_unit.display());
    let output = probe
        .systemd_verify_units(&[swap_unit])
        .map_err(|error| command_error("swap-unit-verify", &verify_command, error))?;
    require_success("swap-unit-verify", &verify_command, output)?;
    checks.push(InstallCheck::pass(
        "swap-unit-verify",
        format!("{SWAP_UNIT_PATH} 문법을 활성화 전에 검증했습니다."),
    ));

    let output = probe
        .runner()
        .run(&swap_apply_command(&sizing.swap_size))
        .map_err(|err| {
            command_error(
                "swapfile",
                format!("create and enable {SWAP_FILE_PATH}"),
                err,
            )
        })?;
    require_success(
        "swapfile",
        format!("create and enable {SWAP_FILE_PATH}"),
        output,
    )?;
    if !owned.iter().any(|path| path == SWAP_FILE_PATH) {
        owned.push(SWAP_FILE_PATH.to_string());
    }

    checks.push(InstallCheck::pass(
        "swapfile",
        format!(
            "Enabled managed {} swapfile through systemd unit {SWAP_UNIT_PATH}.",
            sizing.swap_size
        ),
    ));
    checks.push(InstallCheck::pass(
        "swap-sysctl",
        format!("Applied vm.swappiness=10 and vm.vfs_cache_pressure=50 from {SWAP_SYSCTL_PATH}."),
    ));
    Ok(checks)
}

pub(super) fn swap_apply_command(swap_size: &str) -> CommandSpec {
    let swap_size = shell_single_quote(swap_size);
    let swap_size_mib = swap_size_to_mib(swap_size.trim_matches('\''));
    CommandSpec::new("sh").arg("-c").arg(format!(
        r#"set -eu
swap_size={swap_size}
if [ ! -f {SWAP_FILE_PATH} ]; then
    fallocate -l "$swap_size" {SWAP_FILE_PATH} || dd if=/dev/zero of={SWAP_FILE_PATH} bs=1M count="{swap_size_mib}" status=none
    chmod 600 {SWAP_FILE_PATH}
    mkswap {SWAP_FILE_PATH} >/dev/null
else
    chmod 600 {SWAP_FILE_PATH}
fi
systemctl daemon-reload
systemctl enable --now swapfile.swap >/dev/null
sysctl --system >/dev/null
swapon --show=NAME | grep -qx {SWAP_FILE_PATH}
"#
    ))
}

pub(super) fn swap_size_to_mib(swap_size: &str) -> u64 {
    let normalized = swap_size.trim().to_ascii_lowercase();
    let digits = normalized
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<u64>()
        .unwrap_or(2);
    if normalized.contains('g') {
        digits.saturating_mul(1024).max(1024)
    } else {
        digits.max(1024)
    }
}

pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(super) fn swap_unit_content() -> String {
    format!(
        r#"[Unit]
Description=G7 Installer managed swapfile
After=local-fs.target

[Swap]
What={SWAP_FILE_PATH}

[Install]
WantedBy=swap.target
"#
    )
}

pub(super) fn swap_sysctl_content() -> &'static str {
    "# Managed by g7inst.\nvm.swappiness = 10\nvm.vfs_cache_pressure = 50\nvm.overcommit_memory = 1\n"
}

pub(super) fn php_fpm_site_socket(plan: &plan::InstallPlan) -> String {
    format!(
        "/run/php/php{}-fpm-{}.sock",
        plan.php_version, plan.site_user
    )
}

pub(super) fn php_pool_path(plan: &plan::InstallPlan) -> String {
    format!(
        "/etc/php/{}/fpm/pool.d/g7-{}.conf",
        plan.php_version, plan.site_user
    )
}

pub(super) fn php_session_path(plan: &plan::InstallPlan) -> String {
    format!("/var/lib/php/sessions/g7-{}", plan.site_user)
}

pub(super) fn php_ini_override_path(plan: &plan::InstallPlan) -> String {
    let sapi = if plan.web_server == "frankenphp" {
        "cli"
    } else {
        "fpm"
    };
    format!(
        "/etc/php/{}/{sapi}/conf.d/99-g7-installer.ini",
        plan.php_version,
    )
}

pub(super) fn php_pool_content(
    plan: &plan::InstallPlan,
    sizing: &plan::ResolvedMemorySizing,
) -> String {
    let process_manager = if sizing.php_process_manager == "ondemand" {
        format!(
            "pm = ondemand\npm.max_children = {}\npm.process_idle_timeout = 10s\npm.max_requests = 500",
            sizing.php_max_children
        )
    } else {
        format!(
            "pm = dynamic\npm.max_children = {}\npm.start_servers = {}\npm.min_spare_servers = {}\npm.max_spare_servers = {}\npm.max_requests = 500",
            sizing.php_max_children,
            sizing.php_start_servers,
            sizing.php_min_spare_servers,
            sizing.php_max_spare_servers
        )
    };
    format!(
        r#"[g7-{site_user}]
user = {site_user}
group = www-data
listen = {socket}
listen.owner = www-data
listen.group = www-data
listen.mode = 0660

{process_manager}

php_admin_value[session.save_path] = {session_path}
request_terminate_timeout = 180s
request_slowlog_timeout = 2s
slowlog = /var/log/php{php_version}-fpm-{site_user}-slow.log
catch_workers_output = yes
"#,
        site_user = plan.site_user,
        socket = php_fpm_site_socket(plan),
        session_path = php_session_path(plan),
        php_version = plan.php_version,
    )
}

pub(super) fn php_ini_override_content(sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        r#"; Managed by g7inst.
memory_limit = {memory_limit}
upload_max_filesize = {upload_limit}
post_max_size = {post_limit}
max_execution_time = 120
max_input_vars = 3000
realpath_cache_size = 4096K
realpath_cache_ttl = 600
opcache.enable = 1
opcache.memory_consumption = {opcache_memory}
opcache.interned_strings_buffer = 16
opcache.max_accelerated_files = 20000
opcache.validate_timestamps = 1
opcache.revalidate_freq = 2
opcache.save_comments = 1
opcache.enable_file_override = 0
"#,
        memory_limit = sizing.php_memory_limit,
        upload_limit = sizing.php_upload_limit,
        post_limit = sizing.php_post_limit,
        opcache_memory = sizing.opcache_memory.trim_end_matches('M'),
    )
}

pub(super) fn database_config_path(plan: &plan::InstallPlan) -> &'static str {
    if plan.database_engine == "mariadb" {
        "/etc/mysql/mariadb.conf.d/z-g7-installer.cnf"
    } else {
        "/etc/mysql/conf.d/g7-installer.cnf"
    }
}

pub(super) fn database_service_name(plan: &plan::InstallPlan) -> &'static str {
    if plan.database_engine == "mariadb" {
        "mariadb"
    } else {
        "mysql"
    }
}

pub(super) fn database_runtime_content(sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        r#"# Managed by g7inst.
[mysqld]
bind-address = 127.0.0.1
innodb_buffer_pool_size = {buffer_pool}
max_connections = {max_connections}
tmp_table_size = {tmp_table_size}
max_heap_table_size = {tmp_table_size}
slow_query_log = ON
long_query_time = 1
min_examined_row_limit = 100
"#,
        buffer_pool = sizing.db_buffer_pool,
        max_connections = sizing.db_max_connections,
        tmp_table_size = sizing.db_tmp_table_size,
    )
}
