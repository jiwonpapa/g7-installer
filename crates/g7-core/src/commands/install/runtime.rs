use super::*;

pub(super) fn apply_runtime_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    let sizing = detected_memory_sizing(probe);
    let ini_path = php_ini_override_path(plan);

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

    write_new_file(paths, &ini_path, &php_ini_override_content(&sizing), owned)?;
    checks.push(InstallCheck::pass(
        "php-runtime-ini",
        format!("Created PHP runtime override at {ini_path}."),
    ));

    if plan.web_server == "frankenphp" {
        write_existing_file(
            paths,
            g7_system::nginx::G7_SITE_AVAILABLE,
            &nginx_frankenphp_vhost_content(plan),
        )?;
        checks.push(InstallCheck::pass(
            "frankenphp-runtime",
            format!(
                "FrankenPHP runs PHP requests on {} behind the Nginx edge vhost.",
                FRANKENPHP_LISTEN
            ),
        ));
    } else {
        let pool_path = php_pool_path(plan);
        write_new_file(paths, &pool_path, &php_pool_content(plan, &sizing), owned)?;
        checks.push(InstallCheck::pass(
            "php-fpm-pool",
            format!(
                "Created PHP-FPM pool config at {pool_path}; max_children={}, memory_limit={}.",
                sizing.php_max_children, sizing.php_memory_limit
            ),
        ));
    }

    if plan.web_server == "nginx" {
        write_existing_file(
            paths,
            g7_system::nginx::G7_SITE_AVAILABLE,
            &nginx_vhost_content_with_socket_and_sizing(
                plan,
                &php_fpm_site_socket(plan),
                Some(&sizing),
            ),
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
            status: "info".to_string(),
            message: format!(
                "Recommended nginx.conf values: worker_processes={}, worker_connections={}, rlimit_nofile={}. These are reported but not rewritten until nginx.conf backup ownership is implemented.",
                sizing.nginx_worker_processes,
                sizing.nginx_worker_connections,
                sizing.nginx_worker_rlimit_nofile
            ),
        });
    } else if plan.web_server == "apache" {
        write_existing_file(
            paths,
            g7_system::apache::G7_SITE_AVAILABLE,
            &apache_vhost_content_with_socket(plan, &php_fpm_site_socket(plan)),
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
            status: "info".to_string(),
            message: format!(
                "Apache mpm_event target: MaxRequestWorkers={} with PHP-FPM pool max_children={}. Keep MPM tuning in apache2.conf/mpm_event.conf after manual backup ownership is implemented.",
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

    checks.extend(php_runtime_diagnostic_checks(probe, paths, plan, &sizing));

    Ok(checks)
}

pub(super) fn php_runtime_diagnostic_checks<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    sizing: &plan::ResolvedMemorySizing,
) -> Vec<InstallCheck> {
    let mut checks = Vec::new();
    let output = match probe.runner().run(&php_runtime_probe_command(plan)) {
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

    let facts = parse_key_value_lines(&output.stdout);
    checks.push(InstallCheck::pass(
        "phpinfo-summary",
        format!(
            "{} 기준 PHP 정보를 파싱했습니다: PHP {}, SAPI={}, ini={}, scan_dir={}.",
            if plan.web_server == "frankenphp" {
                "CLI ini"
            } else {
                "FPM ini"
            },
            fact(&facts, "php_version"),
            fact(&facts, "sapi"),
            fact(&facts, "loaded_ini"),
            fact(&facts, "scan_dir")
        ),
    ));

    let limits = [
        ("memory_limit", sizing.php_memory_limit.as_str()),
        ("upload_max_filesize", sizing.php_upload_limit.as_str()),
        ("post_max_size", sizing.php_upload_limit.as_str()),
        (
            "opcache.memory_consumption",
            sizing.opcache_memory.trim_end_matches('M'),
        ),
        ("opcache.validate_timestamps", "0"),
        ("opcache.enable_file_override", "1"),
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
        ("pm", "dynamic".to_string()),
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
                "PHP-FPM pool 확인: user={}, group=www-data, pm=dynamic, max_children={}, max_requests=500.",
                plan.site_user, sizing.php_max_children
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

pub(super) fn fact(facts: &[(String, String)], key: &str) -> String {
    facts
        .iter()
        .find(|(name, _value)| name == key)
        .map(|(_name, value)| value.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".to_string())
}

pub(super) fn normalize_php_value(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn short_text(value: &str) -> String {
    let text = value.trim().replace('\n', " ");
    if text.chars().count() > 240 {
        format!("{}...", text.chars().take(240).collect::<String>())
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
                    || check.name.starts_with("php-extension:"))
        })
        .map(|check| format!("{} - {}", check.name, check.message))
        .collect::<Vec<_>>();

    if failures.is_empty() {
        None
    } else {
        Some(format!(
            "PHP 런타임 진단 실패. 웹앱 설치를 시작하지 않습니다: {}",
            failures.join("; ")
        ))
    }
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
    "# Managed by g7inst.\nvm.swappiness = 10\nvm.vfs_cache_pressure = 50\n"
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
    format!(
        r#"[g7-{site_user}]
user = {site_user}
group = www-data
listen = {socket}
listen.owner = www-data
listen.group = www-data
listen.mode = 0660

pm = dynamic
pm.max_children = {php_max_children}
pm.start_servers = {php_start_servers}
pm.min_spare_servers = {php_min_spare_servers}
pm.max_spare_servers = {php_max_spare_servers}
pm.max_requests = 500

php_admin_value[open_basedir] = {web_root}:/tmp
php_admin_value[session.save_path] = /tmp
request_slowlog_timeout = 2s
slowlog = /var/log/php{php_version}-fpm-{site_user}-slow.log
catch_workers_output = yes
"#,
        site_user = plan.site_user,
        socket = php_fpm_site_socket(plan),
        web_root = plan.web_root,
        php_version = plan.php_version,
        php_max_children = sizing.php_max_children,
        php_start_servers = sizing.php_start_servers,
        php_min_spare_servers = sizing.php_min_spare_servers,
        php_max_spare_servers = sizing.php_max_spare_servers,
    )
}

pub(super) fn php_ini_override_content(sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        r#"; Managed by g7inst.
memory_limit = {memory_limit}
upload_max_filesize = {upload_limit}
post_max_size = {upload_limit}
max_execution_time = 120
max_input_vars = 3000
realpath_cache_size = 4096K
realpath_cache_ttl = 600
opcache.enable = 1
opcache.memory_consumption = {opcache_memory}
opcache.interned_strings_buffer = 16
opcache.max_accelerated_files = 20000
opcache.validate_timestamps = 0
opcache.revalidate_freq = 60
opcache.save_comments = 1
opcache.enable_file_override = 1
"#,
        memory_limit = sizing.php_memory_limit,
        upload_limit = sizing.php_upload_limit,
        opcache_memory = sizing.opcache_memory.trim_end_matches('M'),
    )
}

pub(super) fn database_config_path(plan: &plan::InstallPlan) -> &'static str {
    if plan.database_engine == "mariadb" {
        "/etc/mysql/mariadb.conf.d/60-g7-installer.cnf"
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
long_query_time = 0.5
"#,
        buffer_pool = sizing.db_buffer_pool,
        max_connections = sizing.db_max_connections,
        tmp_table_size = sizing.db_tmp_table_size,
    )
}
