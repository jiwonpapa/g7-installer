use super::*;

pub(super) fn apply_tls_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    network_checks: &[InstallCheck],
) -> Result<Vec<InstallCheck>> {
    if plan.deployment_mode == "local-test" {
        return Ok(vec![
            InstallCheck {
                name: "certbot".to_string(),
                status: "skipped".to_string(),
                message: "Local test mode skips Let's Encrypt certificates.".to_string(),
            },
            InstallCheck {
                name: "tls".to_string(),
                status: "skipped".to_string(),
                message: "Local test mode skips HTTPS vhost mutation.".to_string(),
            },
        ]);
    }

    let failed_dns = network_checks
        .iter()
        .filter(|check| check.status == "fail")
        .map(|check| format!("{}: {}", check.name, check.message))
        .collect::<Vec<_>>();
    if !failed_dns.is_empty() {
        return Ok(vec![
            InstallCheck::fail(
                "tls-dns",
                format!(
                    "Let's Encrypt was not attempted because DNS/IP checks failed: {}",
                    failed_dns.join("; ")
                ),
            ),
            InstallCheck {
                name: "tls-certificate".to_string(),
                status: "deferred".to_string(),
                message: "Fix DNS A records, confirm HTTP access, then resume the TLS phase."
                    .to_string(),
            },
        ]);
    }

    let domains = certificate_hosts(plan);
    let cert_name = plan.domain.clone();
    let email = certificate_email(plan);
    let certbot_challenge_dir = certbot_http01_challenge_dir(plan);
    let certbot_http01_smoke_path = certbot_http01_smoke_path(plan);
    let existing_certificate = certificate_files_exist(paths, &cert_name);
    create_owned_dir_if_absent(
        paths,
        &format!("{}/.well-known", plan.app_document_root),
        owned,
    )?;
    create_owned_dir_if_absent(paths, &certbot_challenge_dir, owned)?;
    if paths.resolve(&certbot_http01_smoke_path).exists() {
        write_existing_file(
            paths,
            &certbot_http01_smoke_path,
            CERTBOT_HTTP01_SMOKE_CONTENT,
        )?;
    } else {
        write_new_file(
            paths,
            &certbot_http01_smoke_path,
            CERTBOT_HTTP01_SMOKE_CONTENT,
            owned,
        )?;
    }
    let owner_group = format!("{}:www-data", plan.site_user);
    let command = format!("chown -R {owner_group} {certbot_challenge_dir}");
    let output = probe
        .chown_recursive(&owner_group, &certbot_challenge_dir)
        .map_err(|err| command_error("certbot-http01-webroot-owner", &command, err))?;
    require_success("certbot-http01-webroot-owner", command, output)?;
    let command = format!("chmod -R 0755 {certbot_challenge_dir}");
    let output = probe
        .chmod_recursive("0755", &certbot_challenge_dir)
        .map_err(|err| command_error("certbot-http01-webroot-permissions", &command, err))?;
    require_success("certbot-http01-webroot-permissions", command, output)?;
    let certbot_http01_uri = certbot_http01_smoke_uri();
    for host in &domains {
        match probe.http_host_path_smoke(host, &certbot_http01_uri) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Error::InstallVerificationFailed {
                    checks: format!(
                        "Certbot HTTP-01 challenge smoke failed for Host: {host} path: {certbot_http01_uri}"
                    ),
                });
            }
            Err(err) => {
                return Err(command_error(
                    "certbot-http01-smoke",
                    format!("curl -H 'Host: {host}' http://127.0.0.1{certbot_http01_uri}"),
                    err,
                ));
            }
        }
    }

    let mut certificate_checks = Vec::new();
    let certificate_check = if existing_certificate {
        if !renewal_webroot_matches(paths, &cert_name, &plan.app_document_root) {
            let output = probe
                .certbot_reconfigure_webroot(&plan.app_document_root, &cert_name)
                .map_err(|err| {
                    command_error(
                        "certbot-reconfigure",
                        format!(
                            "certbot reconfigure --cert-name {cert_name} --webroot-path {}",
                            plan.app_document_root
                        ),
                        err,
                    )
                })?;
            require_success(
                "certbot-reconfigure",
                format!(
                    "certbot reconfigure --cert-name {cert_name} --webroot-path {}",
                    plan.app_document_root
                ),
                output,
            )?;
            certificate_checks.push(InstallCheck::pass(
                "certbot-renewal-webroot",
                format!(
                    "Updated preserved certificate renewal webroot to {}.",
                    plan.app_document_root
                ),
            ));
        } else {
            certificate_checks.push(InstallCheck::pass(
                "certbot-renewal-webroot",
                format!(
                    "Verified preserved certificate renewal webroot: {}.",
                    plan.app_document_root
                ),
            ));
        }
        InstallCheck::pass(
            "tls-certificate",
            format!(
                "기존 Let's Encrypt 인증서 `{cert_name}`를 확인했습니다. 중복 발급 제한을 피하기 위해 새 발급은 실행하지 않았습니다."
            ),
        )
    } else {
        let output = probe
            .certbot_certonly_webroot(&plan.app_document_root, &cert_name, &domains, &email)
            .map_err(|err| {
                command_error(
                    "certbot-certonly",
                    format!(
                        "certbot certonly --webroot -w {} --cert-name {}",
                        plan.app_document_root, cert_name
                    ),
                    err,
                )
            })?;
        require_success(
            "certbot-certonly",
            format!(
                "certbot certonly --webroot -w {} --cert-name {}",
                plan.app_document_root, cert_name
            ),
            output,
        )?;
        InstallCheck::pass(
            "tls-certificate",
            format!(
                "Issued Let's Encrypt certificate `{cert_name}` for {} with Certbot webroot.",
                domains.join(", ")
            ),
        )
    };
    certificate_checks.push(validate_certificate(probe, paths, &cert_name, &domains)?);
    let vhost_check = if matches!(plan.web_server.as_str(), "nginx" | "frankenphp") {
        let sizing = detected_memory_sizing(probe);
        let vhost_content = if plan.web_server == "frankenphp" {
            nginx_frankenphp_tls_vhost_content(plan)
        } else {
            nginx_tls_vhost_content(plan, &php_fpm_site_socket(plan), Some(&sizing))
        };
        write_existing_file(paths, g7_system::nginx::G7_SITE_AVAILABLE, &vhost_content)?;

        let output = probe
            .nginx_config_test()
            .map_err(|err| command_error("nginx-configtest", "nginx -t", err))?;
        require_success("nginx-configtest", "nginx -t", output)?;

        let output = probe
            .reload_service(g7_system::nginx::SERVICE_NAME)
            .map_err(|err| command_error("nginx-reload", "systemctl reload nginx", err))?;
        require_success("nginx-reload", "systemctl reload nginx", output)?;

        InstallCheck::pass(
            if plan.web_server == "frankenphp" {
                "frankenphp-https-vhost"
            } else {
                "nginx-https-vhost"
            },
            format!(
                "Rewrote {} with HTTPS server blocks for {}.",
                g7_system::nginx::G7_SITE_AVAILABLE,
                domains.join(", ")
            ),
        )
    } else {
        enable_apache_modules(probe, apache_tls_modules())?;
        write_existing_file(
            paths,
            g7_system::apache::G7_SITE_AVAILABLE,
            &apache_tls_vhost_content(plan, &php_fpm_site_socket(plan)),
        )?;

        let output = probe
            .apache_config_test()
            .map_err(|err| command_error("apache-configtest", "apache2ctl configtest", err))?;
        require_success("apache-configtest", "apache2ctl configtest", output)?;

        let output = probe
            .reload_service(g7_system::apache::SERVICE_NAME)
            .map_err(|err| command_error("apache-reload", "systemctl reload apache2", err))?;
        require_success("apache-reload", "systemctl reload apache2", output)?;

        InstallCheck::pass(
            "apache-https-vhost",
            format!(
                "Rewrote {} with HTTPS VirtualHost blocks for {}.",
                g7_system::apache::G7_SITE_AVAILABLE,
                domains.join(", ")
            ),
        )
    };

    let output = probe
        .certbot_renew_dry_run(&cert_name)
        .map_err(|err| command_error("certbot-renew-dry-run", "certbot renew --dry-run", err))?;
    require_success("certbot-renew-dry-run", "certbot renew --dry-run", output)?;

    let _ = owned;
    let mut checks = vec![
        InstallCheck::pass(
            "certbot-http01-smoke",
            format!(
                "Verified HTTP-01 challenge path {certbot_http01_uri} for {} before running Certbot.",
                domains.join(", ")
            ),
        ),
        certificate_check,
    ];
    checks.extend(certificate_checks);
    checks.extend([
        vhost_check,
        InstallCheck::pass(
            "certbot-renew-dry-run",
            "certbot renew --dry-run completed successfully.",
        ),
    ]);
    Ok(checks)
}

pub(super) fn renewal_webroot_matches(
    paths: &InstallPaths,
    cert_name: &str,
    webroot: &str,
) -> bool {
    let path = format!("/etc/letsencrypt/renewal/{cert_name}.conf");
    fs::read_to_string(paths.resolve(&path))
        .ok()
        .map(|content| {
            let mut in_webroot_map = false;
            let mut found = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') {
                    in_webroot_map = trimmed == "[[webroot_map]]";
                    continue;
                }
                let Some((key, value)) = trimmed.split_once('=') else {
                    continue;
                };
                if key.trim() != "webroot_path" && !in_webroot_map {
                    continue;
                }
                found = true;
                if value.trim().trim_end_matches(',') != webroot {
                    return false;
                }
            }
            found
        })
        .unwrap_or(false)
}

pub(super) fn validate_certificate<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    cert_name: &str,
    domains: &[String],
) -> Result<InstallCheck> {
    let fullchain = paths
        .resolve(&format!("/etc/letsencrypt/live/{cert_name}/fullchain.pem"))
        .display()
        .to_string();
    let private_key = paths
        .resolve(&format!("/etc/letsencrypt/live/{cert_name}/privkey.pem"))
        .display()
        .to_string();

    let output = probe
        .runner()
        .run(
            &CommandSpec::new("openssl")
                .arg("x509")
                .arg("-in")
                .arg(&fullchain)
                .arg("-noout")
                .arg("-checkend")
                .arg("86400"),
        )
        .map_err(|err| command_error("tls-certificate-expiry", "openssl x509 -checkend", err))?;
    require_success(
        "tls-certificate-expiry",
        "openssl x509 -checkend 86400",
        output,
    )?;

    let output = probe
        .runner()
        .run(
            &CommandSpec::new("openssl")
                .arg("x509")
                .arg("-in")
                .arg(&fullchain)
                .arg("-noout")
                .arg("-ext")
                .arg("subjectAltName"),
        )
        .map_err(|err| {
            command_error(
                "tls-certificate-san",
                "openssl x509 -ext subjectAltName",
                err,
            )
        })?;
    let san_output = output.stdout.clone();
    require_success(
        "tls-certificate-san",
        "openssl x509 -ext subjectAltName",
        output,
    )?;
    let missing_domains = domains
        .iter()
        .filter(|domain| !certificate_san_contains(&san_output, domain))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_domains.is_empty() {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "Preserved certificate `{cert_name}` is missing SAN hosts: {}",
                missing_domains.join(", ")
            ),
        });
    }

    let output = probe
        .runner()
        .run(
            &CommandSpec::new("openssl")
                .arg("x509")
                .arg("-in")
                .arg(&fullchain)
                .arg("-pubkey")
                .arg("-noout"),
        )
        .map_err(|err| command_error("tls-certificate-key", "openssl x509 -pubkey", err))?;
    let cert_public_key = output.stdout.trim().to_string();
    require_success("tls-certificate-key", "openssl x509 -pubkey", output)?;

    let output = probe
        .runner()
        .run(
            &CommandSpec::new("openssl")
                .arg("pkey")
                .arg("-in")
                .arg(&private_key)
                .arg("-pubout"),
        )
        .map_err(|err| command_error("tls-private-key", "openssl pkey -pubout", err))?;
    let private_public_key = output.stdout.trim().to_string();
    require_success("tls-private-key", "openssl pkey -pubout", output)?;
    if cert_public_key.is_empty() || cert_public_key != private_public_key {
        return Err(Error::InstallVerificationFailed {
            checks: format!("Certificate `{cert_name}` and private key do not match"),
        });
    }

    Ok(InstallCheck::pass(
        "tls-certificate-validity",
        format!(
            "Verified certificate expiry, SAN hosts ({}) and private-key match.",
            domains.join(", ")
        ),
    ))
}

pub(super) fn certificate_san_contains(output: &str, domain: &str) -> bool {
    output
        .split(|character: char| character == ',' || character.is_whitespace())
        .any(|entry| entry.trim() == format!("DNS:{domain}"))
}
