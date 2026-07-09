use super::*;

pub(super) fn security_checks(
    redis_mode: &str,
    database_engine: &str,
    security_profile: &str,
    ssh_policy: &str,
    local_test: bool,
) -> Vec<PlanSecurityCheck> {
    let mut checks = vec![
        PlanSecurityCheck {
            name: "filesystem-permissions",
            level: "apply",
            description: "Site files owned by the site account; web server gets read access; writable directories stay limited.",
        },
        PlanSecurityCheck {
            name: "database-credentials",
            level: "apply",
            description: "Generate a random app DB password; never use a default password or print secrets to stdout/logs.",
        },
        PlanSecurityCheck {
            name: "database-bind",
            level: "apply",
            description: if database_engine == "mysql" {
                "Keep MySQL bound to localhost/unix socket and create a least-privilege G7 app user."
            } else {
                "Keep MariaDB bound to localhost/unix socket and create a least-privilege G7 app user."
            },
        },
        PlanSecurityCheck {
            name: "ssh-config",
            level: if ssh_policy == "harden" {
                "apply"
            } else {
                "audit"
            },
            description: if ssh_policy == "harden" {
                "Harden sshd after preserving the active SSH port; do not lock out the current session."
            } else {
                "Audit SSH port, root login, and password authentication; do not change SSH automatically."
            },
        },
        PlanSecurityCheck {
            name: "firewall",
            level: if security_profile == "hardened" {
                "apply"
            } else {
                "audit"
            },
            description: "Allow the active SSH port plus 80/443; keep database and Redis ports closed externally.",
        },
        PlanSecurityCheck {
            name: "php-runtime",
            level: "apply",
            description: "Apply PHP-FPM pool limits, opcache settings, upload limits, and per-site runtime isolation.",
        },
    ];

    if redis_mode == "enable" {
        checks.push(PlanSecurityCheck {
            name: "redis-local-only",
            level: "apply",
            description: "Bind Redis to 127.0.0.1/::1 or unix socket, keep protected-mode enabled, and never expose 6379 publicly.",
        });
    }

    if !local_test {
        checks.push(PlanSecurityCheck {
            name: "tls-headers",
            level: "apply",
            description: "Issue HTTPS certificates and apply sane TLS/security headers after domain ownership checks pass.",
        });
    }

    checks
}

pub(super) fn app_requirements(
    profile: &crate::app_profile::AppProfile,
    php_version: &str,
    database_engine: &str,
    redis_mode: &str,
    local_test: bool,
) -> Vec<AppRequirement> {
    let mut requirements = vec![
        php_version_requirement(profile.min_php, php_version),
        AppRequirement {
            name: "database-version".to_string(),
            status: "deferred",
            message: format!(
                "{} selected; app requires {}. Exact server version is verified in the database phase.",
                database_engine, profile.database_requirement
            ),
        },
        AppRequirement {
            name: "document-root".to_string(),
            status: "planned",
            message: match profile.document_root {
                crate::app_profile::DocumentRootStrategy::SiteRoot => {
                    "web server document root uses the selected site root".to_string()
                }
                crate::app_profile::DocumentRootStrategy::PublicSubdir => {
                    "web server document root must point to the app public/ directory".to_string()
                }
            },
        },
    ];

    for extension in profile.php_extensions {
        requirements.push(php_extension_requirement(
            extension,
            php_version,
            redis_mode,
        ));
    }

    for package in profile.system_packages {
        requirements.push(AppRequirement {
            name: format!("system-package:{package}"),
            status: "deferred",
            message: "required by the selected app profile; install in the app phase".to_string(),
        });
    }

    for service in profile.services {
        requirements.push(AppRequirement {
            name: format!("service:{service}"),
            status: "deferred",
            message: "required by the selected app profile; create and verify in the app phase"
                .to_string(),
        });
    }

    for path in profile.writable_paths {
        requirements.push(AppRequirement {
            name: format!("writable:{path}"),
            status: "deferred",
            message: "must be owned by the site account with limited write permissions".to_string(),
        });
    }

    for check in profile.health_checks {
        requirements.push(AppRequirement {
            name: format!("health:{check}"),
            status: "deferred",
            message: "run after app files, vhost, and database settings are applied".to_string(),
        });
    }

    requirements.push(AppRequirement {
        name: "https".to_string(),
        status: if local_test { "skipped" } else { "deferred" },
        message: if local_test {
            "local-test mode skips public TLS issuance".to_string()
        } else {
            "issue and renew Let's Encrypt certificate after app/vhost phase".to_string()
        },
    });

    requirements
}

pub(super) fn php_version_requirement(min_php: &str, selected_php: &str) -> AppRequirement {
    if php_version_at_least(selected_php, min_php) {
        AppRequirement {
            name: "php-version".to_string(),
            status: "pass",
            message: format!("PHP {selected_php} satisfies app minimum PHP {min_php}."),
        }
    } else {
        AppRequirement {
            name: "php-version".to_string(),
            status: "fail",
            message: format!("PHP {selected_php} is lower than app minimum PHP {min_php}."),
        }
    }
}

pub(super) fn php_extension_requirement(
    extension: &str,
    php_version: &str,
    redis_mode: &str,
) -> AppRequirement {
    if extension == "redis" && redis_mode == "disable" {
        return AppRequirement {
            name: "php-extension:redis".to_string(),
            status: "fail",
            message: "selected app profile requires Redis, but Redis is disabled".to_string(),
        };
    }

    match package_phase_php_extension_package(extension, php_version) {
        Some(package) => AppRequirement {
            name: format!("php-extension:{extension}"),
            status: "planned",
            message: format!(
                "{package} is included in the package phase; php -m verification belongs to the runtime phase"
            ),
        },
        None => AppRequirement {
            name: format!("php-extension:{extension}"),
            status: "deferred",
            message: "verify with php -m in the runtime/app compatibility phase".to_string(),
        },
    }
}

pub(super) fn package_phase_php_extension_package(
    extension: &str,
    php_version: &str,
) -> Option<String> {
    let package = match extension {
        "bcmath" => "bcmath",
        "curl" => "curl",
        "dom" | "simplexml" | "xml" | "xmlwriter" => "xml",
        "gd" => "gd",
        "imagick" => "imagick",
        "intl" => "intl",
        "ldap" => "ldap",
        "maxminddb" => "maxminddb",
        "mbstring" => "mbstring",
        "memcached" => "memcached",
        "mysqli" | "mysqlnd" | "pdo_mysql" => "mysql",
        "redis" => "redis",
        "zip" => "zip",
        _ => return None,
    };

    Some(format!("php{php_version}-{package}"))
}

pub(super) fn memory_sizing_settings() -> Vec<ProvisioningSetting> {
    let mut settings = vec![
        ProvisioningSetting::new("preset_tiers", "1GB, 2GB, 4GB, 8GB, 16GB, 32GB, >32GB"),
        ProvisioningSetting::new("swap_by_ram", preset_matrix(|preset| preset.swap)),
        ProvisioningSetting::new(
            "os_reserve_by_ram",
            preset_matrix(|preset| preset.os_reserve),
        ),
        ProvisioningSetting::new(
            "php_cpu_guard_by_ram",
            preset_matrix(|preset| preset.php_cpu_guard),
        ),
        ProvisioningSetting::new(
            "nginx_worker_processes_by_cpu_ram",
            preset_matrix(|preset| preset.nginx_worker_processes),
        ),
        ProvisioningSetting::new(
            "apache_max_request_workers_by_ram",
            preset_matrix(|preset| preset.apache_max_request_workers),
        ),
    ];

    settings.extend(MEMORY_SIZING_PRESETS.iter().map(|preset| {
        ProvisioningSetting::new(
            preset.key,
            format!(
                "ram={}, swap={}, os_reserve={}, php_max_children={}, php_pool={}, php_cpu_guard={}, php_memory_limit={}, upload={}, opcache={}, db_buffer_pool={}, db_max_connections={}, db_tmp_table_size={}, redis_maxmemory={}, nginx_worker_processes={}, nginx_worker_connections={}, nginx_rlimit_nofile={}, nginx_keepalive_timeout={}, nginx_fastcgi_buffers={}, apache_mpm={}, apache_start_servers={}, apache_server_limit={}, apache_threads_per_child={}, apache_max_request_workers={}, apache_spare_threads={}, apache_max_connections_per_child={}, note={}",
                preset.ram,
                preset.swap,
                preset.os_reserve,
                preset.php_max_children,
                preset.php_processes,
                preset.php_cpu_guard,
                preset.php_memory_limit,
                preset.php_upload_limit,
                preset.opcache_memory,
                preset.db_buffer_pool,
                preset.db_max_connections,
                preset.db_tmp_table_size,
                preset.redis_maxmemory,
                preset.nginx_worker_processes,
                preset.nginx_worker_connections,
                preset.nginx_worker_rlimit_nofile,
                preset.nginx_keepalive_timeout,
                preset.nginx_fastcgi_buffers,
                preset.apache_mpm,
                preset.apache_start_servers,
                preset.apache_server_limit,
                preset.apache_threads_per_child,
                preset.apache_max_request_workers,
                preset.apache_spare_threads,
                preset.apache_max_connections_per_child,
                preset.note
            ),
        )
    }));

    settings
}

pub(super) fn preset_matrix<F>(value: F) -> String
where
    F: Fn(&MemorySizingPreset) -> &'static str,
{
    MEMORY_SIZING_PRESETS
        .iter()
        .map(|preset| format!("{}={}", preset.label, value(preset)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) struct ProvisioningInput<'a> {
    pub(super) domain: &'a str,
    pub(super) app_profile: &'a str,
    pub(super) app_document_root: &'a str,
    pub(super) web_server: &'a str,
    pub(super) php_version: &'a str,
    pub(super) php_source: &'a str,
    pub(super) database_engine: &'a str,
    pub(super) database_name: &'a str,
    pub(super) database_user: &'a str,
    pub(super) database_password_policy: &'a str,
    pub(super) site_user: &'a str,
    pub(super) web_root: &'a str,
    pub(super) www_mode: &'a str,
    pub(super) redis_mode: &'a str,
    pub(super) mail_mode: &'a str,
    pub(super) smtp_port: u16,
    pub(super) security_profile: &'a str,
    pub(super) ssh_policy: &'a str,
    pub(super) local_test: bool,
}

pub(super) fn provisioning_sections(input: ProvisioningInput<'_>) -> Vec<ProvisioningSection> {
    let mut server_sizing_settings = vec![
        ProvisioningSetting::new("size_probe", "RAM, vCPU, disk, swap 상태를 먼저 감지"),
        ProvisioningSetting::new(
            "tier_selection",
            "MemTotal GiB를 가장 가까운 보수 등급으로 내림 선택하고, 32GB 초과는 공식 적용",
        ),
        ProvisioningSetting::new(
            "memory_budget",
            "OS reserve, DB, Redis, PHP-FPM, web server 순서로 메모리 예산 분배",
        ),
        ProvisioningSetting::new(
            "profile_floor",
            "1GB RAM / 2 vCPU / 40GB SSD 기준에서도 과부하를 피하는 값 우선",
        ),
    ];
    server_sizing_settings.extend(memory_sizing_settings());

    let mut sections = vec![
        ProvisioningSection {
            name: "server-sizing",
            title: "서버 사양 기반 튜닝",
            summary: "1/2/4/8/16/32GB 프리셋과 32GB 초과 공식으로 메모리 중심 값을 선택합니다."
                .to_string(),
            settings: server_sizing_settings,
        },
        ProvisioningSection {
            name: "web-server",
            title: "웹서버 호스트 설정",
            summary: format!(
                "{} vhost를 {} 문서 루트에 맞춰 생성하고 root/www 정책을 적용합니다.",
                runtime_label(input.web_server),
                input.app_document_root
            ),
            settings: vec![
                ProvisioningSetting::new("server_name", server_names(input.domain, input.www_mode)),
                ProvisioningSetting::new(
                    "redirect_source",
                    redirect_source(input.domain, input.www_mode),
                ),
                ProvisioningSetting::new("document_root", input.app_document_root),
                ProvisioningSetting::new("site_root", input.web_root),
                ProvisioningSetting::new(
                    "php_endpoint",
                    php_endpoint(input.web_server, input.php_version),
                ),
                ProvisioningSetting::new("rewrite_policy", rewrite_policy(input.app_profile)),
                ProvisioningSetting::new("selected_runtime", web_runtime_model(input.web_server)),
                ProvisioningSetting::new(
                    "nginx_worker_processes_by_cpu_ram",
                    preset_matrix(|preset| preset.nginx_worker_processes),
                ),
                ProvisioningSetting::new(
                    "nginx_worker_connections_by_ram",
                    preset_matrix(|preset| preset.nginx_worker_connections),
                ),
                ProvisioningSetting::new(
                    "nginx_worker_rlimit_nofile_by_ram",
                    preset_matrix(|preset| preset.nginx_worker_rlimit_nofile),
                ),
                ProvisioningSetting::new(
                    "nginx_keepalive_timeout_by_ram",
                    preset_matrix(|preset| preset.nginx_keepalive_timeout),
                ),
                ProvisioningSetting::new(
                    "nginx_fastcgi_buffers_by_ram",
                    preset_matrix(|preset| preset.nginx_fastcgi_buffers),
                ),
                ProvisioningSetting::new("apache_mpm", "event + proxy_fcgi + PHP-FPM"),
                ProvisioningSetting::new(
                    "apache_start_servers_by_ram",
                    preset_matrix(|preset| preset.apache_start_servers),
                ),
                ProvisioningSetting::new(
                    "apache_server_limit_by_ram",
                    preset_matrix(|preset| preset.apache_server_limit),
                ),
                ProvisioningSetting::new(
                    "apache_threads_per_child",
                    preset_matrix(|preset| preset.apache_threads_per_child),
                ),
                ProvisioningSetting::new(
                    "apache_max_request_workers_by_ram",
                    preset_matrix(|preset| preset.apache_max_request_workers),
                ),
                ProvisioningSetting::new(
                    "apache_spare_threads_by_ram",
                    preset_matrix(|preset| preset.apache_spare_threads),
                ),
                ProvisioningSetting::new(
                    "apache_max_connections_per_child_by_ram",
                    preset_matrix(|preset| preset.apache_max_connections_per_child),
                ),
                ProvisioningSetting::new(
                    "apache_php_fpm_boundary",
                    "Apache worker 수는 정적/keepalive 처리 여유이고 PHP 동시 실행 상한은 PHP-FPM max_children",
                ),
                ProvisioningSetting::new(
                    "security_headers",
                    "HTTPS 적용 후 HSTS, nosniff, frame deny, referrer policy 후보 적용",
                ),
            ],
        },
        ProvisioningSection {
            name: "php-runtime",
            title: "PHP 런타임 설정",
            summary: format!(
                "PHP {} 런타임, php.ini, opcache를 앱과 서버 사양 기준으로 조정합니다.",
                input.php_version
            ),
            settings: vec![
                ProvisioningSetting::new("package_source", input.php_source),
                ProvisioningSetting::new("pool_user", input.site_user),
                ProvisioningSetting::new(
                    "pm_policy",
                    "dynamic; max_children은 감지 RAM과 vCPU로 계산",
                ),
                ProvisioningSetting::new(
                    "max_children_by_ram",
                    preset_matrix(|preset| preset.php_max_children),
                ),
                ProvisioningSetting::new(
                    "cpu_guard_by_ram",
                    preset_matrix(|preset| preset.php_cpu_guard),
                ),
                ProvisioningSetting::new(
                    "process_pool_by_ram",
                    preset_matrix(|preset| preset.php_processes),
                ),
                ProvisioningSetting::new(
                    "web_server_boundary",
                    "Nginx/Apache worker는 요청 수용 계층이고 PHP 동시 실행은 max_children으로 제한",
                ),
                ProvisioningSetting::new(
                    "memory_limit_by_ram",
                    preset_matrix(|preset| preset.php_memory_limit),
                ),
                ProvisioningSetting::new(
                    "upload_max_filesize_by_ram",
                    preset_matrix(|preset| preset.php_upload_limit),
                ),
                ProvisioningSetting::new(
                    "post_max_size_by_ram",
                    preset_matrix(|preset| preset.php_upload_limit),
                ),
                ProvisioningSetting::new("max_execution_time", "120초 기본 후보"),
                ProvisioningSetting::new(
                    "opcache_memory_by_ram",
                    preset_matrix(|preset| preset.opcache_memory),
                ),
            ],
        },
        ProvisioningSection {
            name: "database",
            title: "DB 생성 및 계정 설정",
            summary: format!(
                "{}에 앱 전용 DB와 최소 권한 계정을 만들고 localhost 전용으로 묶습니다.",
                database_label(input.database_engine)
            ),
            settings: vec![
                ProvisioningSetting::new("database", input.database_name),
                ProvisioningSetting::new("user", input.database_user),
                ProvisioningSetting::new(
                    "password_policy",
                    database_password_policy_label(input.database_password_policy),
                ),
                ProvisioningSetting::new("bind", "127.0.0.1 또는 unix socket 전용"),
                ProvisioningSetting::new(
                    "buffer_pool_by_ram",
                    preset_matrix(|preset| preset.db_buffer_pool),
                ),
                ProvisioningSetting::new(
                    "max_connections_by_ram",
                    preset_matrix(|preset| preset.db_max_connections),
                ),
                ProvisioningSetting::new(
                    "tmp_table_size_by_ram",
                    preset_matrix(|preset| preset.db_tmp_table_size),
                ),
                ProvisioningSetting::new(
                    "backup_note",
                    "앱 설치 후 DB 백업/복구 경로를 리포트에 표시",
                ),
            ],
        },
        ProvisioningSection {
            name: "firewall",
            title: "방화벽 및 포트 정책",
            summary:
                "SSH, HTTP, HTTPS만 외부 공개하고 DB/Redis/설치 UI 포트는 외부 공개를 차단합니다."
                    .to_string(),
            settings: vec![
                ProvisioningSetting::new("allow", "active SSH port, 80/tcp, 443/tcp"),
                ProvisioningSetting::new("deny", "7717/tcp, 3306/tcp, 6379/tcp inbound"),
                ProvisioningSetting::new(
                    "owner",
                    "Lightsail 방화벽을 1차 기준으로 보고 UFW는 서버 내부 보조 정책으로 적용",
                ),
                ProvisioningSetting::new(
                    "verify",
                    "적용 후 ss/ufw/외부 포트 검사 결과를 리포트에 기록",
                ),
            ],
        },
        ProvisioningSection {
            name: "ssl",
            title: "SSL 인증서 및 자동 갱신",
            summary: if input.local_test {
                "공개 도메인 설치가 아니면 인증서 발급은 건너뜁니다.".to_string()
            } else {
                "도메인 IP 일치 확인 후 Let's Encrypt 인증서를 발급하고 certbot.timer를 검증합니다."
                    .to_string()
            },
            settings: vec![
                ProvisioningSetting::new(
                    "domain_check",
                    "A/AAAA와 www 대상이 현재 VPS 공인 IP와 일치해야 진행",
                ),
                ProvisioningSetting::new("issuer", "Let's Encrypt / Certbot"),
                ProvisioningSetting::new("renewal", "certbot.timer enable + renew dry-run 검증"),
                ProvisioningSetting::new(
                    "fallback",
                    "DNS 불일치 시 HTTP vhost까지만 유지하고 인증서 단계 중단",
                ),
            ],
        },
    ];

    if input.redis_mode == "enable" {
        sections.push(ProvisioningSection {
            name: "redis",
            title: "Redis 캐시 설정",
            summary: "Redis를 로컬 전용 캐시/세션 저장소로 구성하고 서버 RAM에 맞춰 maxmemory를 제한합니다."
                .to_string(),
            settings: vec![
                ProvisioningSetting::new("bind", "127.0.0.1/::1 또는 unix socket 전용"),
                ProvisioningSetting::new("protected_mode", "yes"),
                ProvisioningSetting::new(
                    "maxmemory_by_ram",
                    preset_matrix(|preset| preset.redis_maxmemory),
                ),
                ProvisioningSetting::new("policy", "allkeys-lru 기본 후보"),
            ],
        });
    } else {
        sections.push(ProvisioningSection {
            name: "redis",
            title: "Redis 캐시 설정",
            summary: "Redis 비활성 선택에 따라 설치와 앱 연결 설정을 생략합니다.".to_string(),
            settings: vec![ProvisioningSetting::new("status", "disabled")],
        });
    }

    if input.mail_mode != "none" {
        sections.push(ProvisioningSection {
            name: "mail",
            title: "메일 발송 설정",
            summary:
                "회원 인증/알림 메일 발송만 설정하고 수신 메일 서버는 기본 범위에서 제외합니다."
                    .to_string(),
            settings: vec![
                ProvisioningSetting::new("mode", input.mail_mode),
                ProvisioningSetting::new("smtp_port", input.smtp_port.to_string()),
                ProvisioningSetting::new("inbound_mail", "25/465/587 inbound는 열지 않음"),
                ProvisioningSetting::new(
                    "dns_note",
                    "SPF/DKIM/DMARC/PTR은 발송 방식에 따라 리포트에서 안내",
                ),
            ],
        });
    }

    sections.push(ProvisioningSection {
        name: "security-baseline",
        title: "사이트 보안 기본값",
        summary: format!(
            "{} 보안 수준과 {} SSH 정책 기준으로 변경 전 점검, 적용, 검증을 나눕니다.",
            input.security_profile, input.ssh_policy
        ),
        settings: vec![
            ProvisioningSetting::new(
                "file_ownership",
                "웹파일은 사이트 계정 소유, 쓰기 디렉터리만 제한적으로 허용",
            ),
            ProvisioningSetting::new(
                "fail2ban",
                "SSH jail 상태 점검 후 standard/hardened 정책에서 적용 후보",
            ),
            ProvisioningSetting::new(
                "ssh",
                "audit-only는 리포트만, harden은 현재 세션 보존 후 적용",
            ),
            ProvisioningSetting::new(
                "config_preserve",
                "기존 설정은 백업 후 installer-owned 범위만 변경",
            ),
        ],
    });

    sections
}
