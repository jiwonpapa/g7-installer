use super::*;

pub(super) fn preflight_gates(local_test: bool) -> Vec<PlanGate> {
    let mut gates = vec![
        PlanGate {
            name: "os",
            description: "Require Ubuntu 24.04 LTS.",
        },
        PlanGate {
            name: "privilege",
            description: "Install requires root or sudo.",
        },
        PlanGate {
            name: "fresh-server",
            description: "Abort if existing web services or unowned G7 paths are detected.",
        },
        PlanGate {
            name: "site-account",
            description: "Verify or create the selected site account before placing app files.",
        },
        PlanGate {
            name: "web-root",
            description: "Use the selected site account public_html/www/custom root; do not assume /var/www/g7.",
        },
        PlanGate {
            name: "network",
            description: if local_test {
                "Require port 80 for local HTTP setup."
            } else {
                "Require ports 80 and 443 before HTTP/HTTPS setup."
            },
        },
    ];

    if local_test {
        gates.push(PlanGate {
            name: "local-hostname",
            description: "Use a local test hostname without public DNS or Let's Encrypt.",
        });
    } else {
        gates.push(PlanGate {
            name: "dns-public-ip",
            description: "Verify domain A/AAAA records match this VPS public IP before Certbot.",
        });
    }

    gates.extend([
        PlanGate {
            name: "www-canonical",
            description: "Apply requested root/www canonical host policy.",
        },
        PlanGate {
            name: "mail-outbound",
            description: "Check selected SMTP outbound port before writing mail settings.",
        },
        PlanGate {
            name: "server-security",
            description: "Audit Redis, database, firewall, SSH, PHP, and file permissions before applying changes.",
        },
        PlanGate {
            name: "rollback",
            description: "Track created installer-owned files for rollback on failure.",
        },
        PlanGate {
            name: "config-preserve",
            description: "Preserve existing configuration instead of overwriting unowned files.",
        },
    ]);

    if !local_test {
        gates.push(PlanGate {
            name: "certbot-renewal",
            description: "Enable Let's Encrypt renewal through certbot.timer.",
        });
    }

    gates
}

pub(super) struct PackageInput<'a> {
    pub(super) web_server: &'a str,
    pub(super) php_version: &'a str,
    pub(super) php_source: &'a str,
    pub(super) database_engine: &'a str,
    pub(super) database_version: &'a str,
    pub(super) redis_mode: &'a str,
    pub(super) mail_mode: &'a str,
    pub(super) local_test: bool,
    pub(super) app_profile: &'a crate::app_profile::AppProfile,
}

pub(super) fn packages(input: PackageInput<'_>) -> Vec<PlanPackage> {
    let mut packages = vec![PlanPackage {
        name: web_server_package(input.web_server).to_string(),
        description: web_server_package_description(input.web_server),
    }];

    packages.extend([
        PlanPackage {
            name: php_runtime_packages(input.web_server, input.php_version),
            description: "선택한 앱을 실행하고 PHP 설정을 진단하는 런타임입니다.",
        },
        PlanPackage {
            name: format!(
                "php{}-mysql php{}-mbstring php{}-xml",
                input.php_version, input.php_version, input.php_version
            ),
            description: "DB 접속, 한글 문자열, XML 처리를 위한 PHP 확장입니다.",
        },
        PlanPackage {
            name: format!(
                "php{}-curl php{}-gd php{}-zip",
                input.php_version, input.php_version, input.php_version
            ),
            description: "외부 HTTP 요청, 이미지 처리, 압축 파일 처리를 위한 PHP 확장입니다.",
        },
        PlanPackage {
            name: format!(
                "php{}-intl php{}-bcmath",
                input.php_version, input.php_version
            ),
            description: "다국어/지역화와 정밀 숫자 계산을 위한 PHP 확장입니다.",
        },
        PlanPackage {
            name: format!("php{}-imagick", input.php_version),
            description: "업로드 이미지와 썸네일 처리를 돕는 PHP 이미지 확장입니다.",
        },
        PlanPackage {
            name: database_package(input.database_engine).to_string(),
            description: "게시글, 회원, 설정 데이터를 저장하는 SQL 데이터베이스입니다.",
        },
        PlanPackage {
            name: "curl unzip ca-certificates".to_string(),
            description: "앱 소스 다운로드, 압축 해제, HTTPS 검증에 필요한 도구입니다.",
        },
    ]);

    if input.database_version == "8.4" {
        packages.push(PlanPackage {
            name: "mysql-apt-config".to_string(),
            description: "MySQL 8.4 LTS를 설치하기 위한 Oracle 공식 APT 저장소 설정입니다.",
        });
    }

    if matches!(input.app_profile.id, "laravel" | "laravel-octane") {
        packages.push(PlanPackage {
            name: "git composer nodejs npm".to_string(),
            description: "앱 소스 내려받기와 PHP/프론트엔드 빌드에 필요한 도구입니다.",
        });
    }

    if input.php_source == PHP_SOURCE_ONDREJ {
        packages.push(PlanPackage {
            name: "software-properties-common lsb-release".to_string(),
            description: "Ubuntu 기본값이 아닌 PHP 버전을 설치하기 위한 apt 저장소 도구입니다.",
        });
    }

    if !input.local_test {
        packages.push(PlanPackage {
            name: "certbot".to_string(),
            description: "Let's Encrypt SSL 인증서를 발급하고 갱신합니다.",
        });
        if let Some(package) = certbot_web_plugin_package(input.web_server) {
            packages.push(PlanPackage {
                name: package.to_string(),
                description: "웹서버와 Certbot 인증서 발급을 연결합니다.",
            });
        }
    }

    if input.redis_mode == "enable" {
        packages.push(PlanPackage {
            name: "redis-server".to_string(),
            description: "캐시, 세션, 큐 처리를 위한 로컬 Redis 서버입니다.",
        });
        packages.push(PlanPackage {
            name: format!("php{}-redis", input.php_version),
            description: "PHP 앱이 Redis에 접속하도록 하는 확장입니다.",
        });
    }

    if input.mail_mode == "local-postfix" {
        packages.push(PlanPackage {
            name: "postfix mailutils".to_string(),
            description: "서버에서 알림 메일을 발송하기 위한 Postfix 메일 도구입니다.",
        });
    }

    let mut planned_package_names: HashSet<String> = packages
        .iter()
        .flat_map(|package| package.name.split_whitespace())
        .map(ToOwned::to_owned)
        .collect();
    for extension in input.app_profile.php_extensions {
        if let Some(package) = package_phase_php_extension_package(extension, input.php_version) {
            if planned_package_names.insert(package.clone()) {
                packages.push(PlanPackage {
                    name: package,
                    description: "선택한 앱 프로필의 PHP 확장 요구사항입니다.",
                });
            }
        }
    }
    for package in input.app_profile.system_packages {
        if planned_package_names.insert((*package).to_string()) {
            packages.push(PlanPackage {
                name: (*package).to_string(),
                description: "선택한 앱 프로필의 시스템 도구 요구사항입니다.",
            });
        }
    }

    packages
}

pub(super) fn files(
    app_profile: &crate::app_profile::AppProfile,
    web_server: &str,
    web_root: &str,
    redis_mode: &str,
    mail_mode: &str,
    local_test: bool,
) -> Vec<PlanFile> {
    let mut files = vec![
        PlanFile::new("/etc/g7-installer/config.toml", "create"),
        PlanFile::new(STATE_PATH, "create/update"),
        PlanFile::new(OWNED_FILES_PATH, "create/update"),
        PlanFile::new("/var/lib/g7-installer/rollback.json", "create/update"),
        PlanFile::new(
            "/var/backups/g7-installer",
            "create for preserved config snapshots",
        ),
        PlanFile::new("/var/log/g7-installer/install.log", "create/append"),
        PlanFile::new(
            "/var/log/g7-installer/commands.jsonl",
            "append redacted external command audit records",
        ),
        PlanFile::new(
            "/var/log/g7-installer/report.json",
            "create/update problem report",
        ),
        PlanFile::new(
            web_root,
            "planned app web root; create or verify in install phase",
        ),
        PlanFile::new(
            app_config_file(app_profile, web_root),
            if matches!(app_profile.id, "gnuboard7" | "gnuboard7-octane") {
                "created by the official G7 browser installer"
            } else {
                "create app config with DB/cache/mail settings using root-only secret handling"
            },
        ),
        web_server_available_file(web_server),
        web_server_enabled_file(web_server),
    ];

    if matches!(web_server, "nginx" | "frankenphp") {
        files.push(PlanFile::new(
            "/var/backups/g7-installer/nginx.conf.before-g7",
            "backup original nginx.conf before worker tuning",
        ));
        files.push(PlanFile::new(
            "/etc/nginx/nginx.conf",
            "update worker limits with reset restoration",
        ));
    } else if web_server == "apache" {
        files.push(PlanFile::new(
            "/etc/apache2/conf-available/g7-runtime.conf",
            "create Apache event MPM tuning",
        ));
        files.push(PlanFile::new(
            "/etc/apache2/conf-enabled/g7-runtime.conf",
            "enable Apache event MPM tuning",
        ));
    }

    if web_server == "frankenphp" {
        files.push(PlanFile::new(
            "/opt/g7-frankenphp/frankenphp",
            "download FrankenPHP app-server binary",
        ));
        files.push(PlanFile::new(
            "/etc/systemd/system/g7-frankenphp.service",
            "create FrankenPHP app-server service",
        ));
    }

    for service in app_profile.services {
        files.push(PlanFile::new(
            format!("/etc/systemd/system/{service}"),
            "create in app phase when enabled",
        ));
    }

    if redis_mode == "enable" {
        files.push(PlanFile::new(
            "/etc/redis/redis.conf",
            "persist local bind, protected mode, maxmemory, and eviction policy with Redis CONFIG REWRITE",
        ));
    }

    if local_test {
        files.push(PlanFile::new(
            "/etc/g7-installer/local-hosts.txt",
            "write local hosts entry suggestion",
        ));
    }

    let _ = mail_mode;

    files
}

pub(super) fn services(
    app_profile: &crate::app_profile::AppProfile,
    web_server: &str,
    php_version: &str,
    database_engine: &str,
    redis_mode: &str,
    mail_mode: &str,
    local_test: bool,
) -> Vec<PlanService> {
    let mut services = vec![
        PlanService {
            name: web_server_service(web_server).to_string(),
            action: "enable and reload",
        },
        PlanService {
            name: database_service(database_engine).to_string(),
            action: "bind locally, create app database/user, enable and start",
        },
    ];

    if web_server == "frankenphp" {
        services.push(PlanService {
            name: "g7-frankenphp".to_string(),
            action: "create as localhost app server and restart",
        });
    } else {
        services.push(PlanService {
            name: format!("php{php_version}-fpm"),
            action: "enable and restart",
        });
    }

    for service in app_profile.services {
        services.push(PlanService {
            name: (*service).to_string(),
            action: "create and verify in app phase",
        });
    }

    if !local_test {
        services.push(PlanService {
            name: "certbot.timer".to_string(),
            action: "enable and verify renewal timer",
        });
    }

    if redis_mode == "enable" {
        services.push(PlanService {
            name: "redis-server".to_string(),
            action: "bind to 127.0.0.1, cap memory, enable and restart",
        });
    }

    if mail_mode == "local-postfix" {
        services.push(PlanService {
            name: "postfix".to_string(),
            action: "configure outbound-only mail transport",
        });
    }

    services
}

pub(super) fn ports(
    redis_mode: &str,
    mail_mode: &str,
    smtp_port: u16,
    local_test: bool,
) -> Vec<PlanPort> {
    let mut ports = vec![PlanPort {
        port: 80,
        protocol: "tcp",
        purpose: if local_test {
            "Inbound local HTTP traffic."
        } else {
            "Inbound HTTP and Let's Encrypt challenge."
        },
    }];

    if !local_test {
        ports.push(PlanPort {
            port: 443,
            protocol: "tcp",
            purpose: "Inbound HTTPS traffic.",
        });
    }

    ports.push(PlanPort {
        port: 3306,
        protocol: "tcp",
        purpose: "Localhost-only SQL database. Must not be open to the public internet.",
    });

    if redis_mode == "enable" {
        ports.push(PlanPort {
            port: 6379,
            protocol: "tcp",
            purpose: "Localhost-only Redis. Must not be open to the public internet.",
        });
    }

    if mail_mode == "smtp-relay" || mail_mode == "local-postfix" {
        ports.push(PlanPort {
            port: smtp_port,
            protocol: "tcp",
            purpose: "Outbound SMTP delivery check.",
        });
    }

    ports
}
