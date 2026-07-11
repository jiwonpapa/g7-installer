use super::*;

pub(super) fn apply_vhost_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();

    match plan.web_server.as_str() {
        "nginx" => {
            write_owned_file(
                paths,
                g7_system::nginx::G7_SITE_AVAILABLE,
                &nginx_vhost_content(plan),
                owned,
            )?;
            create_owned_symlink(
                paths,
                g7_system::nginx::G7_SITE_AVAILABLE,
                g7_system::nginx::G7_SITE_ENABLED,
                owned,
            )?;
            checks.push(InstallCheck::pass(
                "nginx-vhost",
                format!(
                    "Wrote {} and enabled it at {}.",
                    g7_system::nginx::G7_SITE_AVAILABLE,
                    g7_system::nginx::G7_SITE_ENABLED
                ),
            ));

            let output = probe
                .nginx_config_test()
                .map_err(|err| command_error("nginx-configtest", "nginx -t", err))?;
            require_success("nginx-configtest", "nginx -t", output)?;
            checks.push(InstallCheck::pass(
                "nginx-configtest",
                "nginx -t completed successfully.",
            ));

            let output = probe
                .reload_service(g7_system::nginx::SERVICE_NAME)
                .map_err(|err| command_error("nginx-reload", "systemctl reload nginx", err))?;
            require_success("nginx-reload", "systemctl reload nginx", output)?;
            checks.push(InstallCheck::pass(
                "nginx-reload",
                "Nginx was reloaded after vhost enable.",
            ));
        }
        "apache" => {
            enable_apache_modules(probe, apache_http_modules())?;
            write_owned_file(
                paths,
                g7_system::apache::G7_SITE_AVAILABLE,
                &apache_vhost_content(plan),
                owned,
            )?;
            create_owned_symlink(
                paths,
                g7_system::apache::G7_SITE_AVAILABLE,
                g7_system::apache::G7_SITE_ENABLED,
                owned,
            )?;
            checks.push(InstallCheck::pass(
                "apache-vhost",
                format!(
                    "Wrote {} and enabled it at {}.",
                    g7_system::apache::G7_SITE_AVAILABLE,
                    g7_system::apache::G7_SITE_ENABLED
                ),
            ));

            let output = probe
                .apache_config_test()
                .map_err(|err| command_error("apache-configtest", "apache2ctl configtest", err))?;
            require_success("apache-configtest", "apache2ctl configtest", output)?;
            checks.push(InstallCheck::pass(
                "apache-configtest",
                "apache2ctl configtest completed successfully.",
            ));

            let output = probe
                .reload_service(g7_system::apache::SERVICE_NAME)
                .map_err(|err| command_error("apache-reload", "systemctl reload apache2", err))?;
            require_success("apache-reload", "systemctl reload apache2", output)?;
            checks.push(InstallCheck::pass(
                "apache-reload",
                "Apache was reloaded after vhost enable.",
            ));
        }
        "frankenphp" => {
            checks.extend(install_frankenphp_app_server(probe, paths, plan, owned)?);
            write_owned_file(
                paths,
                g7_system::nginx::G7_SITE_AVAILABLE,
                &nginx_frankenphp_vhost_content(plan),
                owned,
            )?;
            create_owned_symlink(
                paths,
                g7_system::nginx::G7_SITE_AVAILABLE,
                g7_system::nginx::G7_SITE_ENABLED,
                owned,
            )?;
            checks.push(InstallCheck::pass(
                "frankenphp-vhost",
                format!(
                    "Wrote Nginx edge vhost {} and proxy to FrankenPHP at {}.",
                    g7_system::nginx::G7_SITE_AVAILABLE,
                    FRANKENPHP_LISTEN
                ),
            ));

            let output = probe
                .nginx_config_test()
                .map_err(|err| command_error("nginx-configtest", "nginx -t", err))?;
            require_success("nginx-configtest", "nginx -t", output)?;
            checks.push(InstallCheck::pass(
                "nginx-configtest",
                "nginx -t completed successfully for FrankenPHP edge vhost.",
            ));

            let output = probe
                .reload_service(g7_system::nginx::SERVICE_NAME)
                .map_err(|err| command_error("nginx-reload", "systemctl reload nginx", err))?;
            require_success("nginx-reload", "systemctl reload nginx", output)?;
            checks.push(InstallCheck::pass(
                "nginx-reload",
                "Nginx was reloaded after FrankenPHP edge vhost enable.",
            ));
        }
        _ => return Ok(Vec::new()),
    }

    let smoke_host = primary_http_host(plan);
    match probe.http_host_smoke(&smoke_host) {
        Ok(true) => checks.push(InstallCheck::pass(
            "http-smoke",
            format!("HTTP smoke passed for Host: {smoke_host}."),
        )),
        Ok(false) => {
            return Err(Error::InstallVerificationFailed {
                checks: format!("HTTP smoke failed for Host: {smoke_host}"),
            });
        }
        Err(err) => {
            return Err(command_error(
                "http-smoke",
                format!("curl -H 'Host: {smoke_host}' http://127.0.0.1/"),
                err,
            ));
        }
    }

    Ok(checks)
}

pub(super) fn install_frankenphp_app_server<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    create_owned_dir_if_absent(paths, FRANKENPHP_DIR, owned)?;
    if paths.resolve(FRANKENPHP_BIN_PATH).exists() {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "{FRANKENPHP_BIN_PATH} already exists. Remove it through installer reset or start from a fresh server."
            ),
        });
    }

    let arch_output = probe
        .runner()
        .run(&CommandSpec::new("uname").arg("-m"))
        .map_err(|err| command_error("frankenphp-arch", "uname -m", err))?;
    require_success("frankenphp-arch", "uname -m", arch_output.clone())?;
    let arch = arch_output.stdout.trim();
    let Some(url) = frankenphp_download_url(arch) else {
        return Err(Error::InstallVerificationFailed {
            checks: format!("FrankenPHP binary is not mapped for CPU architecture `{arch}`."),
        });
    };

    let output = probe
        .download_file(url, FRANKENPHP_BIN_PATH)
        .map_err(|err| {
            command_error(
                "frankenphp-download",
                format!("curl -fsSL -o {FRANKENPHP_BIN_PATH} {url}"),
                err,
            )
        })?;
    require_success(
        "frankenphp-download",
        format!("curl -fsSL -o {FRANKENPHP_BIN_PATH} {url}"),
        output,
    )?;
    owned.push(FRANKENPHP_BIN_PATH.to_string());
    let output = probe
        .chmod_path("0755", FRANKENPHP_BIN_PATH)
        .map_err(|err| {
            command_error(
                "frankenphp-chmod",
                format!("chmod 0755 {FRANKENPHP_BIN_PATH}"),
                err,
            )
        })?;
    require_success(
        "frankenphp-chmod",
        format!("chmod 0755 {FRANKENPHP_BIN_PATH}"),
        output,
    )?;
    checks.push(InstallCheck::pass(
        "frankenphp-binary",
        format!("Downloaded FrankenPHP {FRANKENPHP_VERSION} binary for {arch}."),
    ));

    write_new_file(
        paths,
        FRANKENPHP_SERVICE_PATH,
        &frankenphp_service_content(plan),
        owned,
    )?;
    let output = probe
        .systemd_daemon_reload()
        .map_err(|err| command_error("frankenphp-daemon-reload", "systemctl daemon-reload", err))?;
    require_success(
        "frankenphp-daemon-reload",
        "systemctl daemon-reload",
        output,
    )?;
    let output = probe
        .enable_service_now(FRANKENPHP_SERVICE_NAME)
        .map_err(|err| {
            command_error(
                "frankenphp-service-enable",
                format!("systemctl enable --now {FRANKENPHP_SERVICE_NAME}"),
                err,
            )
        })?;
    require_success(
        "frankenphp-service-enable",
        format!("systemctl enable --now {FRANKENPHP_SERVICE_NAME}"),
        output,
    )?;
    match probe.service_activity(FRANKENPHP_SERVICE_NAME) {
        Ok(ServiceActivity::Active) => checks.push(InstallCheck::pass(
            "frankenphp-service",
            format!(
                "{} is active on {}.",
                FRANKENPHP_SERVICE_NAME, FRANKENPHP_LISTEN
            ),
        )),
        Ok(activity) => {
            return Err(Error::InstallVerificationFailed {
                checks: format!(
                    "{} service is not active: {:?}",
                    FRANKENPHP_SERVICE_NAME, activity
                ),
            });
        }
        Err(err) => {
            return Err(command_error(
                "frankenphp-service-verify",
                format!("systemctl is-active {FRANKENPHP_SERVICE_NAME}"),
                err,
            ));
        }
    }

    Ok(checks)
}

pub(super) fn safety_checks(plan: &plan::InstallPlan, phase: &str) -> Vec<InstallCheck> {
    let mut checks = Vec::new();
    if plan.deployment_mode == "public" {
        checks.push(InstallCheck {
            name: "provider-snapshot".to_string(),
            status: "warn".to_string(),
            message: "Vhost/DB/app/SSL 단계 전에는 Lightsail 스냅샷을 먼저 찍는 것을 권장합니다. 설치기는 provider snapshot 존재 여부를 API로 확인하지 않습니다.".to_string(),
        });
    } else {
        checks.push(InstallCheck {
            name: "provider-snapshot".to_string(),
            status: "skipped".to_string(),
            message: "Local test mode does not require a provider snapshot.".to_string(),
        });
    }

    checks.push(InstallCheck {
        name: "rollback-boundary".to_string(),
        status: "info".to_string(),
        message: if phase == "packages-installed" {
            "Rollback is allowed before app/database/certificate content is created.".to_string()
        } else {
            "Rollback is allowed while web-root contents are installer-owned; it is blocked after app/database/certificate content appears. Restore the VPS snapshot for full-server recovery.".to_string()
        },
    });
    checks.push(InstallCheck {
        name: "resume-policy".to_string(),
        status: "info".to_string(),
        message: "Existing installer state blocks a fresh run. Retry through report/recovery, or reset only installer-owned paths before starting over.".to_string(),
    });
    checks
}

pub(super) fn deferred_vhost_checks(plan: &plan::InstallPlan) -> Vec<InstallCheck> {
    vec![InstallCheck {
        name: "vhost-apply".to_string(),
        status: "deferred".to_string(),
        message: format!(
            "{} vhost apply is not implemented in this batch; package install report remains available.",
            plan.web_server
        ),
    }]
}

pub(super) fn apache_http_modules() -> &'static [&'static str] {
    &[
        "mpm_event",
        "proxy",
        "proxy_fcgi",
        "setenvif",
        "rewrite",
        "headers",
    ]
}

pub(super) fn apache_tls_modules() -> &'static [&'static str] {
    &[
        "mpm_event",
        "ssl",
        "http2",
        "proxy",
        "proxy_fcgi",
        "setenvif",
        "rewrite",
        "headers",
    ]
}

pub(super) fn enable_apache_modules<R: CommandRunner>(
    probe: &SystemProbe<R>,
    modules: &[&str],
) -> Result<()> {
    for module in modules {
        let command = format!("a2enmod {module}");
        let output = probe
            .apache_enable_module(module)
            .map_err(|err| command_error("apache-enable-module", &command, err))?;
        require_success("apache-enable-module", command, output)?;
    }
    Ok(())
}

pub(super) fn apache_vhost_content(plan: &plan::InstallPlan) -> String {
    let php_socket = format!("/run/php/php{}-fpm.sock", plan.php_version);
    apache_vhost_content_with_socket(plan, &php_socket)
}

pub(super) fn apache_vhost_content_with_socket(
    plan: &plan::InstallPlan,
    php_socket: &str,
) -> String {
    let redirect_blocks = apache_http_redirect_blocks(plan);
    let (server_name, aliases) = apache_app_hosts(plan);
    let server_alias = apache_server_alias_line(&aliases);
    format!(
        "{redirect_blocks}<VirtualHost *:80>\n    ServerName {server_name}\n{server_alias}    DocumentRoot {root}\n\n    ErrorLog ${{APACHE_LOG_DIR}}/g7-error.log\n    CustomLog ${{APACHE_LOG_DIR}}/g7-access.log combined\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride All\n        Require all granted\n    </Directory>\n\n    <FilesMatch \\.php$>\n        SetHandler \"proxy:unix:{php_socket}|fcgi://localhost/\"\n    </FilesMatch>\n\n    <FilesMatch \"^\\.\">\n        Require all denied\n    </FilesMatch>\n</VirtualHost>\n",
        root = plan.app_document_root,
    )
}

pub(super) fn nginx_vhost_content(plan: &plan::InstallPlan) -> String {
    let php_socket = format!("/run/php/php{}-fpm.sock", plan.php_version);
    nginx_vhost_content_with_socket(plan, &php_socket)
}

pub(super) fn nginx_frankenphp_vhost_content(plan: &plan::InstallPlan) -> String {
    let app_hosts = nginx_app_hosts(plan);
    let redirect_blocks = nginx_redirect_blocks(plan);
    let certbot_http01_location = nginx_certbot_http01_challenge_location();
    let proxy = nginx_frankenphp_proxy_location();

    format!(
        "{redirect_blocks}server {{\n    listen 80;\n    listen [::]:80;\n    server_name {app_hosts};\n    root {root};\n    index index.php index.html index.htm;\n\n    access_log /var/log/nginx/g7-access.log;\n    error_log /var/log/nginx/g7-error.log;\n\n{certbot_http01_location}{proxy}\n    location ~ /\\. {{\n        deny all;\n    }}\n}}\n",
        root = plan.app_document_root,
    )
}

pub(super) fn nginx_vhost_content_with_socket(
    plan: &plan::InstallPlan,
    php_socket: &str,
) -> String {
    nginx_vhost_content_with_socket_and_sizing(plan, php_socket, None)
}

pub(super) fn nginx_vhost_content_with_socket_and_sizing(
    plan: &plan::InstallPlan,
    php_socket: &str,
    sizing: Option<&plan::ResolvedMemorySizing>,
) -> String {
    let app_hosts = nginx_app_hosts(plan);
    let redirect_blocks = nginx_redirect_blocks(plan);
    let certbot_http01_location = nginx_certbot_http01_challenge_location();
    let runtime_directives = nginx_server_runtime_directives(sizing);

    format!(
        "{redirect_blocks}server {{\n    listen 80;\n    listen [::]:80;\n    server_name {app_hosts};\n    root {root};\n    index index.php index.html index.htm;\n\n    access_log /var/log/nginx/g7-access.log;\n    error_log /var/log/nginx/g7-error.log;\n\n{runtime_directives}{certbot_http01_location}\n    location / {{\n        try_files $uri $uri/ /index.php?$query_string;\n    }}\n\n    location ~ \\.php$ {{\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass unix:{php_socket};\n    }}\n\n    location ~ /\\. {{\n        deny all;\n    }}\n}}\n",
        root = plan.app_document_root,
    )
}

pub(super) fn nginx_server_runtime_directives(
    sizing: Option<&plan::ResolvedMemorySizing>,
) -> String {
    let Some(sizing) = sizing else {
        return String::new();
    };

    format!(
        "    client_max_body_size {body_limit};\n    keepalive_timeout {keepalive};\n    fastcgi_buffers {fastcgi_buffers};\n    fastcgi_buffer_size 32k;\n\n",
        body_limit = sizing.nginx_client_max_body_size.to_ascii_lowercase(),
        keepalive = sizing.nginx_keepalive_timeout,
        fastcgi_buffers = sizing.nginx_fastcgi_buffers,
    )
}

pub(super) fn nginx_certbot_http01_challenge_location() -> &'static str {
    "    location ^~ /.well-known/acme-challenge/ {\n        default_type \"text/plain\";\n        try_files $uri =404;\n    }\n"
}

pub(super) fn nginx_frankenphp_proxy_location() -> &'static str {
    "    location / {\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_set_header X-Forwarded-Host $host;\n        proxy_set_header X-Forwarded-Port $server_port;\n        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection \"upgrade\";\n        proxy_read_timeout 120s;\n        proxy_send_timeout 120s;\n        proxy_pass http://127.0.0.1:7080;\n    }\n"
}

pub(super) fn nginx_app_hosts(plan: &plan::InstallPlan) -> String {
    match plan.www_mode.as_str() {
        "redirect-to-www" if !plan.domain.starts_with("www.") => format!("www.{}", plan.domain),
        "redirect-to-root" | "none" => plan.domain.clone(),
        _ if !plan.domain.starts_with("www.") => format!("{} www.{}", plan.domain, plan.domain),
        _ => plan.domain.clone(),
    }
}

pub(super) fn secrets_content(
    plan: &plan::InstallPlan,
    db_password: &str,
    smtp_password: Option<&str>,
) -> String {
    let mut content = format!(
        "database_name = {}\ndatabase_user = {}\ndatabase_password = {}\n",
        toml_string(&plan.database_name),
        toml_string(&plan.database_user),
        toml_string(db_password)
    );
    if let (Some(username), Some(password)) = (&plan.smtp_username, smtp_password) {
        content.push_str(&format!(
            "smtp_username = {}\nsmtp_password = {}\n",
            toml_string(username),
            toml_string(password)
        ));
    }
    content
}

pub(super) fn pending_secrets_content(
    database_password: &str,
    site_password: Option<&str>,
    smtp_password: Option<&str>,
) -> String {
    let mut content = format!("database_password = {}\n", toml_string(database_password));
    if let Some(password) = site_password {
        content.push_str(&format!("site_password = {}\n", toml_string(password)));
    }
    if let Some(password) = smtp_password {
        content.push_str(&format!("smtp_password = {}\n", toml_string(password)));
    }
    content
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

pub(super) fn database_sql(plan: &plan::InstallPlan, db_password: &str) -> String {
    format!(
        "CREATE DATABASE IF NOT EXISTS `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;\n\
         CREATE USER IF NOT EXISTS '{user}'@'localhost' IDENTIFIED BY '{password}';\n\
         ALTER USER '{user}'@'localhost' IDENTIFIED BY '{password}';\n\
         GRANT ALL PRIVILEGES ON `{db}`.* TO '{user}'@'localhost';\n\
         FLUSH PRIVILEGES;\n",
        db = sql_identifier(&plan.database_name),
        user = sql_string(&plan.database_user),
        password = sql_string(db_password),
    )
}

pub(super) fn sql_identifier(value: &str) -> String {
    value.replace('`', "``")
}

pub(super) fn sql_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "''")
}

pub(super) fn nginx_redirect_blocks(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "server {{\n    listen 80;\n    listen [::]:80;\n    server_name www.{domain};\n    root {root};\n\n{certbot_http01_location}\n    location / {{\n        return 301 http://{domain}$request_uri;\n    }}\n}}\n\n",
            domain = plan.domain,
            root = plan.app_document_root,
            certbot_http01_location = nginx_certbot_http01_challenge_location()
        ),
        "redirect-to-www" => format!(
            "server {{\n    listen 80;\n    listen [::]:80;\n    server_name {domain};\n    root {root};\n\n{certbot_http01_location}\n    location / {{\n        return 301 http://www.{domain}$request_uri;\n    }}\n}}\n\n",
            domain = plan.domain,
            root = plan.app_document_root,
            certbot_http01_location = nginx_certbot_http01_challenge_location()
        ),
        _ => String::new(),
    }
}

pub(super) fn apache_http_redirect_blocks(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "<VirtualHost *:80>\n    ServerName www.{domain}\n    DocumentRoot {root}\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride None\n        Require all granted\n    </Directory>\n\n    RewriteEngine On\n    RewriteCond %{{REQUEST_URI}} !^/\\.well-known/acme-challenge/\n    RewriteRule ^ http://{domain}%{{REQUEST_URI}} [R=301,L]\n</VirtualHost>\n\n",
            domain = plan.domain,
            root = plan.app_document_root
        ),
        "redirect-to-www" => format!(
            "<VirtualHost *:80>\n    ServerName {domain}\n    DocumentRoot {root}\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride None\n        Require all granted\n    </Directory>\n\n    RewriteEngine On\n    RewriteCond %{{REQUEST_URI}} !^/\\.well-known/acme-challenge/\n    RewriteRule ^ http://www.{domain}%{{REQUEST_URI}} [R=301,L]\n</VirtualHost>\n\n",
            domain = plan.domain,
            root = plan.app_document_root
        ),
        _ => String::new(),
    }
}

pub(super) fn nginx_tls_vhost_content(
    plan: &plan::InstallPlan,
    php_socket: &str,
    sizing: Option<&plan::ResolvedMemorySizing>,
) -> String {
    let http_hosts = certificate_hosts(plan).join(" ");
    let cert_name = &plan.domain;
    let app_hosts = nginx_app_hosts(plan);
    let canonical_redirect = nginx_https_canonical_redirect(plan);
    let certbot_http01_location = nginx_certbot_http01_challenge_location();
    let runtime_directives = nginx_server_runtime_directives(sizing);

    format!(
        "server {{\n    listen 80;\n    listen [::]:80;\n    server_name {http_hosts};\n    root {root};\n\n{certbot_http01_location}\n    location / {{\n        return 301 https://$host$request_uri;\n    }}\n}}\n\n{canonical_redirect}server {{\n    listen 443 ssl http2;\n    listen [::]:443 ssl http2;\n    server_name {app_hosts};\n    root {root};\n    index index.php index.html index.htm;\n\n    ssl_certificate /etc/letsencrypt/live/{cert_name}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{cert_name}/privkey.pem;\n    ssl_protocols TLSv1.2 TLSv1.3;\n    ssl_prefer_server_ciphers off;\n\n    access_log /var/log/nginx/g7-access.log;\n    error_log /var/log/nginx/g7-error.log;\n\n    add_header X-Content-Type-Options nosniff always;\n    add_header X-Frame-Options SAMEORIGIN always;\n    add_header Referrer-Policy strict-origin-when-cross-origin always;\n\n{runtime_directives}{certbot_http01_location}\n    location / {{\n        try_files $uri $uri/ /index.php?$query_string;\n    }}\n\n    location ~ \\.php$ {{\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass unix:{php_socket};\n    }}\n\n    location ~ /\\. {{\n        deny all;\n    }}\n}}\n",
        root = plan.app_document_root,
    )
}

pub(super) fn nginx_frankenphp_tls_vhost_content(plan: &plan::InstallPlan) -> String {
    let http_hosts = certificate_hosts(plan).join(" ");
    let cert_name = &plan.domain;
    let app_hosts = nginx_app_hosts(plan);
    let canonical_redirect = nginx_https_canonical_redirect(plan);
    let certbot_http01_location = nginx_certbot_http01_challenge_location();
    let proxy = nginx_frankenphp_proxy_location();

    format!(
        "server {{\n    listen 80;\n    listen [::]:80;\n    server_name {http_hosts};\n    root {root};\n\n{certbot_http01_location}\n    location / {{\n        return 301 https://$host$request_uri;\n    }}\n}}\n\n{canonical_redirect}server {{\n    listen 443 ssl http2;\n    listen [::]:443 ssl http2;\n    server_name {app_hosts};\n    root {root};\n    index index.php index.html index.htm;\n\n    ssl_certificate /etc/letsencrypt/live/{cert_name}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{cert_name}/privkey.pem;\n    ssl_protocols TLSv1.2 TLSv1.3;\n    ssl_prefer_server_ciphers off;\n\n    access_log /var/log/nginx/g7-access.log;\n    error_log /var/log/nginx/g7-error.log;\n\n    add_header X-Content-Type-Options nosniff always;\n    add_header X-Frame-Options SAMEORIGIN always;\n    add_header Referrer-Policy strict-origin-when-cross-origin always;\n\n{certbot_http01_location}{proxy}\n    location ~ /\\. {{\n        deny all;\n    }}\n}}\n",
        root = plan.app_document_root,
    )
}

pub(super) fn apache_tls_vhost_content(plan: &plan::InstallPlan, php_socket: &str) -> String {
    let http_hosts = certificate_hosts(plan).join(" ");
    let cert_name = &plan.domain;
    let canonical_redirect = apache_https_canonical_redirect(plan);
    let (server_name, aliases) = apache_app_hosts(plan);
    let server_alias = apache_server_alias_line(&aliases);
    format!(
        "<VirtualHost *:80>\n    ServerName {primary_host}\n    ServerAlias {http_hosts}\n    DocumentRoot {root}\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride None\n        Require all granted\n    </Directory>\n\n    RewriteEngine On\n    RewriteCond %{{REQUEST_URI}} !^/\\.well-known/acme-challenge/\n    RewriteRule ^ https://%{{HTTP_HOST}}%{{REQUEST_URI}} [R=301,L]\n</VirtualHost>\n\n{canonical_redirect}<VirtualHost *:443>\n    ServerName {server_name}\n{server_alias}    DocumentRoot {root}\n\n    ErrorLog ${{APACHE_LOG_DIR}}/g7-error.log\n    CustomLog ${{APACHE_LOG_DIR}}/g7-access.log combined\n\n    SSLEngine on\n    SSLCertificateFile /etc/letsencrypt/live/{cert_name}/fullchain.pem\n    SSLCertificateKeyFile /etc/letsencrypt/live/{cert_name}/privkey.pem\n    Protocols h2 http/1.1\n\n    Header always set X-Content-Type-Options \"nosniff\"\n    Header always set X-Frame-Options \"SAMEORIGIN\"\n    Header always set Referrer-Policy \"strict-origin-when-cross-origin\"\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride All\n        Require all granted\n    </Directory>\n\n    <FilesMatch \\.php$>\n        SetHandler \"proxy:unix:{php_socket}|fcgi://localhost/\"\n    </FilesMatch>\n\n    <FilesMatch \"^\\.\">\n        Require all denied\n    </FilesMatch>\n</VirtualHost>\n",
        primary_host = primary_http_host(plan),
        root = plan.app_document_root,
    )
}

pub(super) fn nginx_https_canonical_redirect(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "server {{\n    listen 443 ssl http2;\n    listen [::]:443 ssl http2;\n    server_name www.{domain};\n    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;\n    return 301 https://{domain}$request_uri;\n}}\n\n",
            domain = plan.domain
        ),
        "redirect-to-www" => format!(
            "server {{\n    listen 443 ssl http2;\n    listen [::]:443 ssl http2;\n    server_name {domain};\n    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;\n    return 301 https://www.{domain}$request_uri;\n}}\n\n",
            domain = plan.domain
        ),
        _ => String::new(),
    }
}

pub(super) fn apache_https_canonical_redirect(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "<VirtualHost *:443>\n    ServerName www.{domain}\n    SSLEngine on\n    SSLCertificateFile /etc/letsencrypt/live/{domain}/fullchain.pem\n    SSLCertificateKeyFile /etc/letsencrypt/live/{domain}/privkey.pem\n    Redirect permanent / https://{domain}/\n</VirtualHost>\n\n",
            domain = plan.domain
        ),
        "redirect-to-www" => format!(
            "<VirtualHost *:443>\n    ServerName {domain}\n    SSLEngine on\n    SSLCertificateFile /etc/letsencrypt/live/{domain}/fullchain.pem\n    SSLCertificateKeyFile /etc/letsencrypt/live/{domain}/privkey.pem\n    Redirect permanent / https://www.{domain}/\n</VirtualHost>\n\n",
            domain = plan.domain
        ),
        _ => String::new(),
    }
}

pub(super) fn apache_app_hosts(plan: &plan::InstallPlan) -> (String, Vec<String>) {
    match plan.www_mode.as_str() {
        "redirect-to-www" if !plan.domain.starts_with("www.") => {
            (format!("www.{}", plan.domain), Vec::new())
        }
        "redirect-to-root" | "none" => (plan.domain.clone(), Vec::new()),
        _ if !plan.domain.starts_with("www.") => {
            (plan.domain.clone(), vec![format!("www.{}", plan.domain)])
        }
        _ => (plan.domain.clone(), Vec::new()),
    }
}

pub(super) fn apache_server_alias_line(aliases: &[String]) -> String {
    if aliases.is_empty() {
        String::new()
    } else {
        format!("    ServerAlias {}\n", aliases.join(" "))
    }
}

pub(super) fn certificate_email(plan: &plan::InstallPlan) -> String {
    plan.smtp_from
        .clone()
        .unwrap_or_else(|| format!("admin@{}", plan.domain.trim_start_matches("www.")))
}

pub(super) fn primary_http_host(plan: &plan::InstallPlan) -> String {
    if plan.www_mode == "redirect-to-www" && !plan.domain.starts_with("www.") {
        format!("www.{}", plan.domain)
    } else {
        plan.domain.clone()
    }
}

pub(super) fn frankenphp_download_url(arch: &str) -> Option<&'static str> {
    match arch.trim() {
        "x86_64" | "amd64" => Some(
            "https://github.com/php/frankenphp/releases/download/v1.12.4/frankenphp-linux-x86_64",
        ),
        "aarch64" | "arm64" => Some(
            "https://github.com/php/frankenphp/releases/download/v1.12.4/frankenphp-linux-aarch64",
        ),
        _ => None,
    }
}

pub(super) fn frankenphp_service_content(plan: &plan::InstallPlan) -> String {
    format!(
        "[Unit]\nDescription=G7 FrankenPHP app server\nAfter=network-online.target mysql.service mariadb.service redis-server.service\nWants=network-online.target\n\n[Service]\nType=simple\nUser={site_user}\nGroup=www-data\nWorkingDirectory={web_root}\nEnvironment=APP_ENV=production\nEnvironment=DB_HOST=127.0.0.1\nEnvironment=DB_READ_HOST=127.0.0.1\nExecStart={bin} php-server --listen {listen} --root {root} --access-log\nRestart=always\nRestartSec=3\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=full\nReadWritePaths={web_root} /tmp\nLimitNOFILE=65535\n\n[Install]\nWantedBy=multi-user.target\n",
        site_user = plan.site_user,
        web_root = plan.web_root,
        bin = FRANKENPHP_BIN_PATH,
        listen = FRANKENPHP_LISTEN,
        root = plan.app_document_root,
    )
}

pub(super) fn frankenphp_octane_service_content(plan: &plan::InstallPlan) -> String {
    format!(
        "[Unit]\nDescription=G7 {label} on FrankenPHP\nAfter=network-online.target mysql.service mariadb.service redis-server.service\nWants=network-online.target\n\n[Service]\nType=simple\nUser={site_user}\nGroup=www-data\nWorkingDirectory={web_root}\nEnvironment=APP_ENV=production\nEnvironment=DB_HOST=127.0.0.1\nEnvironment=DB_READ_HOST=127.0.0.1\nExecStart=/usr/bin/php artisan octane:frankenphp --host={host} --port={port} --admin-port=2019 --workers=auto --max-requests=500\nRestart=always\nRestartSec=3\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=full\nReadWritePaths={web_root} /tmp\nLimitNOFILE=65535\nTimeoutStopSec=3600\n\n[Install]\nWantedBy=multi-user.target\n",
        label = plan.app_profile_label,
        site_user = plan.site_user,
        web_root = plan.web_root,
        host = FRANKENPHP_HOST,
        port = FRANKENPHP_PORT,
    )
}

pub(super) fn web_service_name(plan: &plan::InstallPlan) -> &'static str {
    match plan.web_server.as_str() {
        "apache" => g7_system::apache::SERVICE_NAME,
        _ => g7_system::nginx::SERVICE_NAME,
    }
}

pub(super) fn managed_services(plan: &plan::InstallPlan) -> Vec<String> {
    plan.services
        .iter()
        .filter(|service| package_phase_manages_service(&service.name, plan))
        .map(|service| service.name.clone())
        .collect()
}

pub(super) fn package_phase_manages_service(service: &str, plan: &plan::InstallPlan) -> bool {
    service == web_service_name(plan)
        || service == format!("php{}-fpm", plan.php_version)
        || service
            == if plan.database_engine == "mysql" {
                "mysql"
            } else {
                "mariadb"
            }
        || service == "certbot.timer"
        || service == "redis-server"
        || service == "postfix"
}

pub(super) fn managed_ports(plan: &plan::InstallPlan) -> Vec<u16> {
    plan.ports
        .iter()
        .filter_map(|port| match port.port {
            80 | 3306 | 6379 => Some(port.port),
            _ => None,
        })
        .collect()
}
