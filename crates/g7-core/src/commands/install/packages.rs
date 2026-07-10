use super::*;

pub(super) fn apply_package_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> std::result::Result<ApplySummary, Box<PackagePhaseFailure>> {
    let packages = package_names(plan);
    let services = managed_services(plan);
    let ports = managed_ports(plan);
    let preinstall_package_checks =
        inspect_preinstall_packages(probe, &packages).map_err(|error| PackagePhaseFailure {
            error,
            summary: ApplySummary::default(),
            completed_steps: Vec::new(),
        })?;
    let mut summary = ApplySummary {
        preinstall_package_checks,
        ..ApplySummary::default()
    };
    let mut completed_steps = Vec::new();

    let output = match probe.apt_update() {
        Ok(output) => output,
        Err(err) => {
            return Err(package_phase_failure(
                command_error("apt-update", "apt-get update", err),
                &summary,
                &completed_steps,
            ));
        }
    };
    if let Err(error) = require_success("apt-update", "apt-get update", output) {
        return Err(package_phase_failure(error, &summary, &completed_steps));
    }
    completed_steps.push("apt-updated".to_string());

    if plan.php_source == g7_system::php::PHP_SOURCE_ONDREJ {
        let source_packages = php_source_prerequisite_packages();
        let install_command = format!("apt-get install -y {}", source_packages.join(" "));
        let output = match probe.apt_install(&source_packages) {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("php-source-prerequisites", &install_command, err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) =
            require_success("php-source-prerequisites", install_command.clone(), output)
        {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }

        let output = match probe.apt_add_repository("ppa:ondrej/php") {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error(
                        "php-source-add",
                        "add-apt-repository -y ppa:ondrej/php",
                        err,
                    ),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) = require_success(
            "php-source-add",
            "add-apt-repository -y ppa:ondrej/php",
            output,
        ) {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
        completed_steps.push("php-apt-source-added".to_string());

        let output = match probe.apt_update() {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("apt-update-after-php-source", "apt-get update", err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) = require_success("apt-update-after-php-source", "apt-get update", output)
        {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
        completed_steps.push("apt-updated-after-php-source".to_string());
    }

    let mut candidate_checks = Vec::new();
    for package in &packages {
        let available = match probe.apt_candidate_available(package) {
            Ok(available) => available,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("apt-candidate", format!("apt-cache policy {package}"), err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if !available {
            candidate_checks.push(InstallCheck::fail(
                package,
                "현재 apt 저장소에서 설치 후보를 찾지 못했습니다.",
            ));
            summary.package_checks = candidate_checks;
            completed_steps.push("package-candidates-checked".to_string());
            return Err(package_phase_failure(
                Error::PackageUnavailable {
                    package: package.clone(),
                },
                &summary,
                &completed_steps,
            ));
        }
        candidate_checks.push(InstallCheck::pass(
            package,
            "apt 저장소에서 설치 후보를 확인했습니다.",
        ));
    }
    summary.package_checks = candidate_checks;
    completed_steps.push("package-candidates-checked".to_string());

    if plan.mail_mode == "local-postfix" && packages.iter().any(|package| package == "postfix") {
        let mailname = postfix_mailname(plan);
        let output = match probe.postfix_preseed(&mailname) {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("postfix-preseed", "debconf-set-selections postfix", err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) =
            require_success("postfix-preseed", "debconf-set-selections postfix", output)
        {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
        completed_steps.push("postfix-preseeded".to_string());
    }

    let install_command = format!("apt-get install -y {}", packages.join(" "));
    let output = match probe.apt_install(&packages) {
        Ok(output) => output,
        Err(err) => {
            return Err(package_phase_failure(
                command_error("apt-install", &install_command, err),
                &summary,
                &completed_steps,
            ));
        }
    };
    if let Err(error) = require_success("apt-install", install_command.clone(), output) {
        return Err(package_phase_failure(error, &summary, &completed_steps));
    }

    for service in &services {
        let command = format!("systemctl enable --now {service}");
        let output = match probe.enable_service_now(service) {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("service-enable", &command, err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) = require_success("service-enable", command.clone(), output) {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
    }

    if plan.mail_mode == "local-postfix" {
        if let Err(error) = apply_local_postfix_runtime(probe, plan) {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
        completed_steps.push("postfix-configured".to_string());
    }

    let package_checks = verify_packages(probe, &packages)
        .map_err(|error| package_phase_failure(error, &summary, &completed_steps))?;
    summary.package_checks = package_checks;
    let service_checks = verify_services(probe, &services)
        .map_err(|error| package_phase_failure(error, &summary, &completed_steps))?;
    summary.service_checks = service_checks;
    let port_checks = verify_ports(probe, &ports)
        .map_err(|error| package_phase_failure(error, &summary, &completed_steps))?;
    summary.port_checks = port_checks;
    if let Err(error) = require_checks_passed(
        &summary.package_checks,
        &summary.service_checks,
        &summary.port_checks,
    ) {
        return Err(package_phase_failure(error, &summary, &completed_steps));
    }
    let network_checks = verify_network_readiness(probe, plan);
    let mail_checks = verify_mail_readiness(probe, plan);
    let certbot_checks = verify_certbot_readiness(probe, plan, &summary.service_checks);

    summary.network_checks = network_checks;
    summary.mail_checks = mail_checks;
    summary.certbot_checks = certbot_checks;
    Ok(summary)
}

pub(super) fn package_phase_failure(
    error: Error,
    summary: &ApplySummary,
    completed_steps: &[String],
) -> Box<PackagePhaseFailure> {
    Box::new(PackagePhaseFailure {
        error,
        summary: summary.clone(),
        completed_steps: completed_steps.to_vec(),
    })
}

pub(super) fn apply_local_postfix_runtime<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Result<()> {
    for (key, value) in local_postfix_runtime_settings(plan) {
        let command = format!("postconf -e {key}");
        let output = probe
            .postconf_set(key, &value)
            .map_err(|err| command_error("postfix-config", &command, err))?;
        require_success("postfix-config", command, output)?;
    }

    let output = probe
        .restart_service("postfix")
        .map_err(|err| command_error("postfix-restart", "systemctl restart postfix", err))?;
    require_success("postfix-restart", "systemctl restart postfix", output)?;

    Ok(())
}

pub(super) fn postfix_mailname(plan: &plan::InstallPlan) -> String {
    plan.domain.trim().trim_end_matches('.').to_string()
}

pub(super) fn local_postfix_runtime_settings(
    plan: &plan::InstallPlan,
) -> Vec<(&'static str, String)> {
    let mailname = postfix_mailname(plan);

    vec![
        ("myhostname", mailname),
        ("myorigin", "$myhostname".to_string()),
        ("inet_interfaces", "loopback-only".to_string()),
        ("inet_protocols", "ipv4".to_string()),
        (
            "mydestination",
            "$myhostname, localhost.$mydomain, localhost".to_string(),
        ),
        ("mynetworks", "127.0.0.0/8".to_string()),
        ("relayhost", String::new()),
    ]
}

pub(super) fn php_source_prerequisite_packages() -> Vec<String> {
    vec![
        "software-properties-common".to_string(),
        "ca-certificates".to_string(),
        "lsb-release".to_string(),
    ]
}

pub(super) fn verify_network_readiness<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Vec<InstallCheck> {
    if !plan.dns_check_required {
        return vec![InstallCheck {
            name: "dns-public-ip".to_string(),
            status: "skipped".to_string(),
            message: "DNS/IP check is skipped for local-test mode or disabled dns_check."
                .to_string(),
        }];
    }

    let public_v4 = match probe.public_ipv4() {
        Ok(Some(address)) => {
            let mut checks = vec![InstallCheck::pass(
                "server-public-ipv4",
                format!("Detected server public IPv4: {address}"),
            )];
            checks.extend(verify_dns_hosts_v4(probe, plan, Some(address)));
            return checks;
        }
        Ok(None) => None,
        Err(err) => {
            return vec![InstallCheck::fail(
                "server-public-ipv4",
                format!("Could not detect server public IPv4: {err}"),
            )];
        }
    };

    verify_dns_hosts_v4(probe, plan, public_v4)
}

pub(super) fn verify_dns_hosts_v4<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
    public_v4: Option<IpAddr>,
) -> Vec<InstallCheck> {
    let mut checks = Vec::new();

    for host in certificate_hosts(plan) {
        let name = if host == plan.domain {
            "dns-a".to_string()
        } else {
            format!("dns-a-{host}")
        };

        let Some(public_v4) = public_v4 else {
            checks.push(InstallCheck::fail(
                name,
                "Server public IPv4 is unavailable, so DNS A record cannot be compared.",
            ));
            continue;
        };

        match probe.dns_ipv4_records(&host) {
            Ok(records) if records.contains(&public_v4) => checks.push(InstallCheck::pass(
                name,
                format!(
                    "{} A record matches server public IPv4 {}.",
                    host, public_v4
                ),
            )),
            Ok(records) if records.is_empty() => checks.push(InstallCheck::fail(
                name,
                format!("{host} has no A record from system resolver."),
            )),
            Ok(records) => checks.push(InstallCheck::fail(
                name,
                format!(
                    "{host} A records {:?} do not include server public IPv4 {}.",
                    records, public_v4
                ),
            )),
            Err(err) => checks.push(InstallCheck::fail(
                name,
                format!("Could not resolve {host} A record: {err}"),
            )),
        }
    }

    checks
}

pub(super) fn verify_mail_readiness<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Vec<InstallCheck> {
    match plan.mail_mode.as_str() {
        "none" => vec![InstallCheck {
            name: "mail-delivery".to_string(),
            status: "skipped".to_string(),
            message: "Mail delivery is disabled for this install.".to_string(),
        }],
        "smtp-relay" => {
            let host = plan.smtp_host.as_deref().unwrap_or("");
            let port = plan.smtp_port.unwrap_or(587);
            let status = probe.tcp_connect(host, port);
            vec![match status {
                Ok(true) => InstallCheck::pass(
                    "smtp-relay",
                    format!("SMTP relay {host}:{port} is reachable from this server."),
                ),
                Ok(false) => InstallCheck::fail(
                    "smtp-relay",
                    format!("SMTP relay {host}:{port} is not reachable from this server."),
                ),
                Err(err) => InstallCheck::fail(
                    "smtp-relay",
                    format!("Could not check SMTP relay {host}:{port}: {err}"),
                ),
            }]
        }
        "local-postfix" => vec![match probe.service_activity("postfix") {
            Ok(ServiceActivity::Active) => InstallCheck::pass(
                "local-postfix",
                "Postfix service is active for outbound-only local mail delivery.",
            ),
            Ok(ServiceActivity::Inactive) => {
                InstallCheck::fail("local-postfix", "Postfix service is inactive.")
            }
            Ok(ServiceActivity::NotFound) => {
                InstallCheck::fail("local-postfix", "Postfix service was not found.")
            }
            Ok(ServiceActivity::Unknown) => {
                InstallCheck::fail("local-postfix", "Postfix service state is unknown.")
            }
            Err(err) => InstallCheck::fail(
                "local-postfix",
                format!("Could not inspect Postfix service: {err}"),
            ),
        }],
        other => vec![InstallCheck::fail(
            "mail-delivery",
            format!("Unsupported mail mode in install report: {other}"),
        )],
    }
}

pub(super) fn verify_certbot_readiness<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
    service_checks: &[InstallCheck],
) -> Vec<InstallCheck> {
    if plan.deployment_mode == "local-test" {
        return vec![InstallCheck {
            name: "certbot".to_string(),
            status: "skipped".to_string(),
            message: "Local test mode skips Let's Encrypt certificates.".to_string(),
        }];
    }

    let mut checks = Vec::new();
    let timer_check = service_checks
        .iter()
        .find(|check| check.name == "certbot.timer")
        .cloned()
        .unwrap_or_else(|| {
            InstallCheck::fail(
                "certbot.timer",
                "certbot.timer was not checked during install.",
            )
        });
    checks.push(timer_check);

    let cert_path = Path::new("/etc/letsencrypt/live").join(&plan.domain);
    if !probe.path_exists(&cert_path) {
        checks.push(InstallCheck {
            name: "certbot-certificate".to_string(),
            status: "deferred".to_string(),
            message: "Certificate issuance waits for the vhost/app phase so HTTP-01 can be served safely.".to_string(),
        });
        checks.push(InstallCheck {
            name: "certbot-renew-dry-run".to_string(),
            status: "deferred".to_string(),
            message: "Renewal dry-run will run after a certificate exists.".to_string(),
        });
        return checks;
    }

    checks.push(InstallCheck::pass(
        "certbot-certificate",
        format!("Existing certificate directory found for {}.", plan.domain),
    ));
    checks.push(InstallCheck {
        name: "certbot-renew-dry-run".to_string(),
        status: "deferred".to_string(),
        message:
            "기존 인증서의 갱신 웹루트를 현재 사이트에 맞춘 뒤 TLS 단계에서 갱신 검증을 실행합니다."
                .to_string(),
    });

    checks
}

pub(super) fn certificate_hosts(plan: &plan::InstallPlan) -> Vec<String> {
    let mut hosts = vec![plan.domain.clone()];
    if plan.www_mode != "none" && !plan.domain.starts_with("www.") {
        hosts.push(format!("www.{}", plan.domain));
    }
    hosts
}

pub(super) fn verify_packages<R: CommandRunner>(
    probe: &SystemProbe<R>,
    packages: &[String],
) -> Result<Vec<InstallCheck>> {
    packages
        .iter()
        .map(|package| match probe.package_status(package) {
            Ok(PackageStatus::Installed) => {
                Ok(InstallCheck::pass(package, "패키지 설치 확인 완료"))
            }
            Ok(PackageStatus::NotInstalled) => {
                Ok(InstallCheck::fail(package, "패키지가 설치되지 않았습니다."))
            }
            Ok(PackageStatus::Unknown) => Ok(InstallCheck::fail(
                package,
                "패키지 상태를 확인하지 못했습니다.",
            )),
            Err(err) => Err(command_error(
                "package-verify",
                format!("dpkg-query {package}"),
                err,
            )),
        })
        .collect()
}

pub(super) fn inspect_preinstall_packages<R: CommandRunner>(
    probe: &SystemProbe<R>,
    packages: &[String],
) -> Result<Vec<InstallCheck>> {
    packages
        .iter()
        .map(|package| match probe.package_status(package) {
            Ok(PackageStatus::Installed) => Ok(InstallCheck {
                name: package.clone(),
                status: "installed".to_string(),
                message: "설치 전부터 있던 패키지입니다. 그대로 사용합니다.".to_string(),
            }),
            Ok(PackageStatus::NotInstalled) => Ok(InstallCheck {
                name: package.clone(),
                status: "not-installed".to_string(),
                message: "설치 전에는 없던 패키지입니다. 이번 설치 대상입니다.".to_string(),
            }),
            Ok(PackageStatus::Unknown) => Ok(InstallCheck {
                name: package.clone(),
                status: "unknown".to_string(),
                message: "설치 전 패키지 상태를 확인하지 못했습니다.".to_string(),
            }),
            Err(err) => Err(command_error(
                "package-baseline",
                format!("dpkg-query {package}"),
                err,
            )),
        })
        .collect()
}

pub(super) fn verify_services<R: CommandRunner>(
    probe: &SystemProbe<R>,
    services: &[String],
) -> Result<Vec<InstallCheck>> {
    services
        .iter()
        .map(|service| match probe.service_activity(service) {
            Ok(ServiceActivity::Active) => Ok(InstallCheck::pass(service, "service is active")),
            Ok(ServiceActivity::Inactive) => Ok(InstallCheck::fail(service, "service is inactive")),
            Ok(ServiceActivity::NotFound) => {
                Ok(InstallCheck::fail(service, "service was not found"))
            }
            Ok(ServiceActivity::Unknown) => {
                Ok(InstallCheck::fail(service, "service state is unknown"))
            }
            Err(err) => Err(command_error(
                "service-verify",
                format!("systemctl is-active {service}"),
                err,
            )),
        })
        .collect()
}

pub(super) fn verify_ports<R: CommandRunner>(
    probe: &SystemProbe<R>,
    ports: &[u16],
) -> Result<Vec<InstallCheck>> {
    ports
        .iter()
        .map(|port| match probe.tcp_port_status(*port) {
            Ok(PortStatus::InUse) => Ok(InstallCheck::pass(
                port.to_string(),
                "TCP port is listening",
            )),
            Ok(PortStatus::Free) => Ok(InstallCheck::fail(
                port.to_string(),
                "TCP port is not listening",
            )),
            Ok(PortStatus::Unknown) => Ok(InstallCheck::fail(
                port.to_string(),
                "TCP port status is unknown",
            )),
            Err(err) => Err(command_error(
                "port-verify",
                format!("ss -H -tulpn for port {port}"),
                err,
            )),
        })
        .collect()
}
