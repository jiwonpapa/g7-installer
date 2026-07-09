use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOptions {
    pub local_test: bool,
    pub app_profile: String,
    pub web_server: String,
    pub php_version: String,
    pub php_source: String,
    pub database_engine: String,
    pub database_name: Option<String>,
    pub database_user: Option<String>,
    pub database_password: Option<String>,
    pub site_user: String,
    pub site_user_password: Option<String>,
    pub web_root_mode: String,
    pub custom_web_root: Option<String>,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_from: Option<String>,
    pub smtp_encryption: String,
    pub security_profile: String,
    pub ssh_policy: String,
    pub rollback: bool,
    pub preserve_config: bool,
    pub dns_check: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            local_test: false,
            app_profile: DEFAULT_APP_PROFILE.to_string(),
            web_server: DEFAULT_WEB_SERVER.to_string(),
            php_version: DEFAULT_PHP_VERSION.to_string(),
            php_source: DEFAULT_PHP_SOURCE.to_string(),
            database_engine: DEFAULT_DATABASE_ENGINE.to_string(),
            database_name: None,
            database_user: None,
            database_password: None,
            site_user: DEFAULT_SITE_USER.to_string(),
            site_user_password: None,
            web_root_mode: DEFAULT_WEB_ROOT_MODE.to_string(),
            custom_web_root: None,
            www_mode: DEFAULT_WWW_MODE.to_string(),
            redis_mode: DEFAULT_REDIS_MODE.to_string(),
            mail_mode: DEFAULT_MAIL_MODE.to_string(),
            smtp_host: None,
            smtp_port: DEFAULT_SMTP_PORT,
            smtp_from: None,
            smtp_encryption: DEFAULT_SMTP_ENCRYPTION.to_string(),
            security_profile: DEFAULT_SECURITY_PROFILE.to_string(),
            ssh_policy: DEFAULT_SSH_POLICY.to_string(),
            rollback: true,
            preserve_config: true,
            dns_check: true,
        }
    }
}

pub fn build(domain: String) -> Result<InstallPlan> {
    build_with_options(domain, PlanOptions::default())
}

pub fn build_with_options(domain: String, options: PlanOptions) -> Result<InstallPlan> {
    let domain = normalize_domain(domain)?;
    let app_profile = resolve_app_profile(&options.app_profile)?;
    let web_server =
        normalize_supported_option("web-server", options.web_server, &SUPPORTED_WEB_SERVERS)?;
    if app_profile.id.ends_with("-octane") && web_server != "frankenphp" {
        return Err(Error::InvalidOption {
            field: "web-server",
            value: web_server,
            supported: "frankenphp for octane app profiles".to_string(),
        });
    }
    let mut php_version = normalize_php_version(options.php_version)?;
    if web_server == "frankenphp" {
        php_version = g7_system::php::NEXT_FPM_VERSION.to_string();
    }
    let database_engine = normalize_supported_option(
        "database",
        options.database_engine,
        &SUPPORTED_DATABASE_ENGINES,
    )?;
    let php_source = normalize_php_source(&php_version, options.php_source)?;
    let site_user = normalize_site_user(options.site_user)?;
    validate_site_user_password(options.site_user_password.as_deref())?;
    validate_database_password(options.database_password.as_deref())?;
    let web_root_mode = normalize_web_root_mode(options.web_root_mode, &options.custom_web_root)?;
    let web_root = web_root_for(
        &domain,
        &site_user,
        &web_root_mode,
        options.custom_web_root.as_deref(),
    )?;
    let app_document_root = app_profile.document_root_for(&web_root);
    let www_mode = normalize_supported_option("www-mode", options.www_mode, &SUPPORTED_WWW_MODES)?;
    let redis_mode =
        normalize_supported_option("redis", options.redis_mode, &SUPPORTED_REDIS_MODES)?;
    let mail_mode =
        normalize_supported_option("mail-mode", options.mail_mode, &SUPPORTED_MAIL_MODES)?;
    let smtp_encryption = normalize_supported_option(
        "smtp-encryption",
        options.smtp_encryption,
        &SUPPORTED_SMTP_ENCRYPTION,
    )?;
    let security_profile = normalize_supported_option(
        "security-profile",
        options.security_profile,
        &SUPPORTED_SECURITY_PROFILES,
    )?;
    let ssh_policy =
        normalize_supported_option("ssh-policy", options.ssh_policy, &SUPPORTED_SSH_POLICIES)?;
    validate_mail_options(
        &mail_mode,
        options.smtp_host.as_deref(),
        options.smtp_from.as_deref(),
    )?;
    let smtp_port = smtp_port_for_mode(&mail_mode, options.smtp_port);
    let database_name = match options.database_name {
        Some(value) if !value.trim().is_empty() => normalize_database_identifier(
            "database-name",
            value,
            64,
            "letters, digits, and underscore only, max 64 characters",
        )?,
        _ => database_name_for_domain(&domain, app_profile.id),
    };
    let database_user = match options.database_user {
        Some(value) if !value.trim().is_empty() => normalize_database_identifier(
            "database-user",
            value,
            32,
            "letters, digits, and underscore only, max 32 characters",
        )?,
        _ => database_user_for_site_user(&site_user, app_profile.id),
    };
    let database_password_policy = if options.database_password.is_some() {
        "user-provided-store-root-only"
    } else {
        "generate-random-store-root-only"
    };

    let dns_check_required = options.dns_check && !options.local_test;
    let deployment_mode = if options.local_test {
        "local-test"
    } else {
        "public"
    }
    .to_string();
    let packages = packages(PackageInput {
        web_server: &web_server,
        php_version: &php_version,
        php_source: &php_source,
        database_engine: &database_engine,
        redis_mode: &redis_mode,
        mail_mode: &mail_mode,
        local_test: options.local_test,
        app_profile,
    });
    let files = files(
        app_profile,
        &web_server,
        &web_root,
        &redis_mode,
        &mail_mode,
        options.local_test,
    );
    let services = services(
        app_profile,
        &web_server,
        &php_version,
        &database_engine,
        &redis_mode,
        &mail_mode,
        options.local_test,
    );
    let ports = ports(&redis_mode, &mail_mode, smtp_port, options.local_test);
    let security_checks = security_checks(
        &redis_mode,
        &database_engine,
        &security_profile,
        &ssh_policy,
        options.local_test,
    );
    let app_requirements = app_requirements(
        app_profile,
        &php_version,
        &database_engine,
        &redis_mode,
        options.local_test,
    );
    let app_followup_steps = app_profile.followup_steps();
    let provisioning = provisioning_sections(ProvisioningInput {
        domain: &domain,
        app_profile: app_profile.id,
        app_document_root: &app_document_root,
        web_server: &web_server,
        php_version: &php_version,
        php_source: &php_source,
        database_engine: &database_engine,
        database_name: &database_name,
        database_user: &database_user,
        database_password_policy,
        site_user: &site_user,
        web_root: &web_root,
        www_mode: &www_mode,
        redis_mode: &redis_mode,
        mail_mode: &mail_mode,
        smtp_port,
        security_profile: &security_profile,
        ssh_policy: &ssh_policy,
        local_test: options.local_test,
    });
    let stop_conditions = stop_conditions(&web_server, &web_root, options.local_test);

    Ok(InstallPlan {
        domain,
        deployment_mode,
        app_profile: app_profile.id.to_string(),
        app_profile_label: app_profile.label,
        app_summary: app_profile.summary,
        app_document_root,
        web_server,
        php_version: php_version.clone(),
        php_source,
        database_engine,
        site_user,
        web_root_mode,
        web_root,
        www_mode,
        redis_mode,
        mail_mode: mail_mode.clone(),
        smtp_host: options.smtp_host,
        smtp_port: smtp_port_for_plan(&mail_mode, smtp_port),
        smtp_from: options.smtp_from,
        smtp_encryption: smtp_encryption_for_plan(&mail_mode, smtp_encryption),
        security_profile,
        ssh_policy,
        database_name,
        database_user,
        database_password_policy,
        rollback_enabled: options.rollback,
        preserve_config: options.preserve_config,
        dns_check_required,
        mode: "dry-run",
        fresh_server_only: true,
        changes_made: false,
        preflight_gates: preflight_gates(options.local_test),
        packages,
        files,
        services,
        ports,
        security_checks,
        app_requirements,
        app_followup_steps,
        provisioning,
        stop_conditions,
    })
}
