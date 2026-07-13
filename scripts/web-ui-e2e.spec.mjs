import { createRequire } from "node:module";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const requireFromWeb = createRequire(path.join(root, "web/package.json"));
const { test, expect } = requireFromWeb("@playwright/test");

const csrf = "e2e-csrf-token";

function json(response, payload, status = 200) {
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
  });
  response.end(JSON.stringify(payload));
}

function mockReport() {
  return {
    version: 1,
    domain: "g7devops.com",
    phase: "completed",
    install_started_at_unix_ms: 1_000,
    install_completed_at_unix_ms: 111_900,
    elapsed_ms: 110_900,
    deployment_mode: "public",
    app_package: "gnuboard7",
    app_profile: "gnuboard7",
    app_profile_label: "Gnuboard 7",
    app_document_root: "/home/g7devops/public_html/public",
    app_url: "https://g7devops.com/install",
    web_server: "nginx",
    php_version: "8.5",
    php_source: "ondrej",
    database: "mysql",
    database_version: "8.4",
    database_name: "g7devops",
    database_user: "g7devops",
    database_password_policy: "user-provided-store-root-only",
    site_user: "g7devops",
    web_root: "/home/g7devops/public_html",
    www_mode: "redirect-to-www",
    redis: "enable",
    mail_mode: "none",
    smtp_host: null,
    smtp_port: null,
    smtp_from: null,
    dns_check: true,
    security_profile: "standard",
    ssh_policy: "audit-only",
    state_path: "/var/lib/g7-installer/state.json",
    owned_files_path: "/var/lib/g7-installer/owned-files.json",
    backup_manifest_path: "/var/backups/g7-installer/manifest.json",
    owned_files: ["/etc/g7-installer/config.toml", "/var/log/g7-installer/report.json"],
    completed_steps: ["packages-installed", "runtime-configured", "vhost-enabled", "database-configured", "certbot-issued", "app-source-prepared", "setup-guide-written", "backup-manifest-written"],
    safety_checks: [{ name: "fresh-server", status: "pass", message: "신규 VPS 조건을 통과했습니다." }],
    preinstall_package_checks: [],
    package_checks: [{ name: "nginx", status: "pass", message: "패키지 설치 확인 완료" }],
    service_checks: [{ name: "nginx", status: "pass", message: "서비스가 실행 중입니다." }],
    port_checks: [{ name: "port-80", status: "pass", message: "80 포트 확인" }],
    network_checks: [],
    runtime_checks: [
      { name: "phpinfo-summary", status: "pass", message: "FPM ini 기준 PHP 정보를 파싱했습니다: PHP 8.5.8, SAPI=cli, ini=/etc/php/8.5/fpm/php.ini, scan_dir=/etc/php/8.5/fpm/conf.d, timezone=Asia/Seoul." },
      { name: "php-runtime-limits", status: "pass", message: "PHP 한도 적용 확인: memory_limit=256M, upload_max_filesize=64M, post_max_size=72M, max_execution_time=120, max_input_vars=3000, opcache.memory_consumption=128." },
      { name: "php-fpm-pool-values", status: "pass", message: "PHP-FPM pool 확인: user=g7devops, group=www-data, pm=ondemand, max_children=8, max_requests=500." },
      { name: "php-extension:mbstring", status: "pass", message: "PHP 확장 mbstring 로드 확인." },
      { name: "php-extension:redis", status: "pass", message: "PHP 확장 redis 로드 확인." },
    ],
    database_checks: [
      { name: "database-created", status: "pass", message: "DB 생성 확인" },
      { name: "database-user-created", status: "pass", message: "DB 계정 확인" },
    ],
    firewall_checks: [{ name: "network-boundary", status: "manual", message: "VPS 제공자 방화벽 또는 별도 유지보수 앱에서 관리하세요." }],
    mail_checks: [{ name: "postfix", status: "pass", message: "Postfix 발송 확인" }],
    certbot_checks: [{ name: "tls-certificate", status: "pass", message: "인증서 확인" }],
    vhost_checks: [{ name: "nginx-configtest", status: "pass", message: "nginx -t 통과" }],
    app_checks: [
      { name: "g7-core-template-engine", status: "pass", message: "G7 core 파일 확인" },
      { name: "g7-install-lock", status: "manual", message: "브라우저 /install 완료 전이면 정상입니다." },
    ],
    setup_guide_path: "/var/log/g7-installer/setup-guide.md",
    app_requirements: [{ name: "php-extension:redis", status: "planned", message: "패키지 단계에서 설치됩니다." }],
    app_followup_steps: [{ name: "open browser installer at /install", description: "app install phase" }],
    problem: null,
  };
}

function mockPlan() {
  return {
    text: "mock plan",
    domain: "g7devops.com",
    deployment_mode: "public",
    app_profile: "gnuboard7",
    app_profile_label: "Gnuboard 7",
    app_document_root: "/home/g7devops/public_html/public",
    web_server: "nginx",
    php_version: "8.5",
    php_source: "ondrej",
    database: "mysql",
    database_version: "8.4",
    database_name: "g7devops",
    database_user: "g7devops",
    database_password_policy: "user-provided-store-root-only",
    app_package: "gnuboard7",
    site_user: "g7devops",
    web_root: "/home/g7devops/public_html",
    packages: [
      { name: "nginx", description: "도메인 요청을 PHP 앱으로 전달하는 웹서버입니다." },
      { name: "php8.5-fpm php8.5-cli", description: "PHP 런타임입니다." },
    ],
    files: [{ path: "/etc/g7-installer/config.toml", action: "create" }],
    services: [{ name: "nginx", action: "reload" }],
    ports: [{ port: 80, protocol: "tcp", purpose: "HTTP" }],
    security_checks: [{ name: "fresh-server", level: "required", description: "신규 VPS 조건" }],
    app_requirements: [{ name: "php-extension:redis", status: "planned", message: "패키지 단계에서 설치됩니다." }],
    app_followup_steps: [{ name: "open browser installer at /install", description: "app install phase" }],
    provisioning: [{ name: "php", title: "PHP-FPM", summary: "PHP 런타임 튜닝", settings: [{ key: "memory_limit", value: "256M" }] }],
    stop_conditions: [],
  };
}

async function asset(pathname) {
  if (pathname === "/app.js") {
    return readFile(path.join(root, "web/app.js"));
  }
  if (pathname === "/modules/event-stream.js") {
    return readFile(path.join(root, "web/modules/event-stream.js"));
  }
  if (pathname === "/app.css") {
    return readFile(path.join(root, "web/dist/app.css"));
  }
  if (pathname === "/assets/setup-orbit-light.webp") {
    return readFile(path.join(root, "web/assets/setup-orbit-light.webp"));
  }
  if (pathname === "/promo.sample.json" || pathname === "/promo.json") {
    return readFile(path.join(root, "web/promo.sample.json"));
  }
  return null;
}

async function startServer(options = {}) {
  let installPrepared = false;
  let installFailed = false;
  const server = createServer(async (request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    const pathname = url.pathname;

    if (pathname === "/api/bootstrap") {
      json(response, {
        domain: "g7devops.com",
        local_test: false,
        csrf_token: csrf,
        auth: {
          mode: "setup-token",
          status: "authenticated",
          username: null,
          authenticated: true,
          client_ip: "127.0.0.1",
        },
      });
      return;
    }
    if (pathname === "/api/recovery") {
      const freshRecovery = {
        can_resume: false,
        can_retry_step: false,
        can_reset: false,
        can_rollback: false,
        recommended_action: "manual",
        failed_step: null,
        restore_status: null,
        message: "설치기 소유 흔적이 없습니다.",
        metadata_paths: [],
        rollback_reason: null,
        resume_reason: null,
        g7_database_created: false,
        g7_database_confirmed: false,
        g7_database_name: null,
        server_configured: false,
        app_files_prepared: false,
        g7_install_completed: false,
        g7_install_lock_path: null,
        app_install_url: null,
        lifecycle_status: "fresh",
      };
      const pendingRecovery = {
        can_resume: false,
        can_retry_step: false,
        can_reset: true,
        can_rollback: false,
        recommended_action: "reset",
        failed_step: null,
        restore_status: null,
        message: "서버 구성과 앱 파일 배치는 완료됐으며 웹 설치를 마무리해야 합니다.",
        metadata_paths: ["/var/lib/g7-installer/state.json"],
        rollback_reason: "앱/DB/인증서 단계 이후에는 reset을 사용합니다.",
        resume_reason: "현재 단계에서는 이어서 진행할 수 없습니다.",
        g7_database_created: true,
        g7_database_confirmed: true,
        g7_database_name: "g7devops",
        server_configured: true,
        app_files_prepared: true,
        g7_install_completed: false,
        g7_install_lock_path: "/home/g7devops/public_html/storage/app/g7_installed",
        app_install_url: "https://g7devops.com/install/",
        lifecycle_status: "app-install-pending",
      };
      const configuredRecovery = options.recovery && (!options.installFailure || installFailed)
        ? options.recovery
        : null;
      json(response, configuredRecovery || (options.reportExists === false && !installPrepared ? freshRecovery : pendingRecovery));
      return;
    }
    if (pathname === "/api/status") {
      json(response, {
        installed: false,
        install_running: false,
        components: [],
      });
      return;
    }
    if (pathname === "/api/report") {
      if (options.reportExists === false && !installPrepared) {
        json(response, { exists: false, path: "/var/log/g7-installer/report.json", content: "" });
        return;
      }
      json(response, {
        exists: true,
        path: "/var/log/g7-installer/report.json",
        content: JSON.stringify(options.report || mockReport()),
      });
      return;
    }
    if (pathname === "/api/artifacts/setup-guide") {
      response.writeHead(200, { "content-type": "text/markdown; charset=utf-8" });
      response.end("# G7 Installer Setup Guide\n\n- 설정 안내서 테스트\n");
      return;
    }
    if (pathname === "/api/doctor") {
      json(response, options.doctor || {
        install_allowed: true,
        resources: {
          total_memory_mib: 2048,
          available_memory_mib: 1536,
          swap_total_mib: 2048,
          root_available_mib: 40000,
          root_inode_free_percent: 95,
        },
        checks: [
          { name: "ubuntu-version", status: "pass", message: "Ubuntu 24.04 확인" },
          { name: "privilege", status: "pass", message: "root 권한 확인" },
        ],
      });
      return;
    }
    if (pathname === "/api/plan") {
      json(response, mockPlan());
      return;
    }
    if (pathname === "/api/install/prepare") {
      await new Promise((resolve) => setTimeout(resolve, options.installDelayMs || 0));
      if (options.installFailure) {
        installFailed = true;
        json(response, {
          error: "mock vhost failure",
          hint: "복원된 실패 단계를 다시 실행하세요.",
        }, 500);
        return;
      }
      installPrepared = true;
      json(response, mockReport());
      return;
    }
    if (pathname === "/api/reset") {
      await new Promise((resolve) => setTimeout(resolve, options.resetDelayMs || 0));
      json(response, {
        dry_run: false,
        actions: [{ resource: "site:g7", status: "removed", message: "사이트 리소스 정리 완료" }],
        removed: ["/var/lib/g7-installer/state.json"],
        missing: [],
      });
      return;
    }
    if (pathname === "/api/provision/action") {
      json(response, {
        action: "security",
        status: "manual",
        message: "보안 경계는 현재 안내 모드입니다.",
        checks: [{ name: "security-policy", status: "manual", message: "SSH와 VPS 제공자 포트 정책을 확인하세요." }],
      });
      return;
    }

    const file = await asset(pathname);
    if (file) {
      const contentType = pathname.endsWith(".css")
        ? "text/css; charset=utf-8"
        : pathname.endsWith(".webp")
          ? "image/webp"
        : pathname.endsWith(".json")
          ? "application/json; charset=utf-8"
          : "application/javascript; charset=utf-8";
      response.writeHead(200, { "content-type": contentType });
      response.end(file);
      return;
    }

    const html = await readFile(path.join(root, "web/index.html"), "utf8");
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html.replaceAll("__G7INST_ASSET_VERSION__", "e2e").replaceAll("__G7INST_PROMO_MANIFEST_URL__", "/promo.sample.json"));
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return {
    server,
    baseUrl: `http://127.0.0.1:${server.address().port}`,
  };
}

test("wizard routes render concise result and optional setup guide", async ({ page }) => {
  const { server, baseUrl } = await startServer();
  try {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${baseUrl}/setup/connect?token=e2e`);
    await expect(page.getByRole("heading", { name: "새 Ubuntu VPS를 그누보드7 서버로" })).toBeVisible();
    await expect(page.locator(".connect-intro-media")).toHaveAttribute("src", "/assets/setup-orbit-light.webp?v=e2e");
    await expect(page.locator(".hero-logo-node")).toHaveCount(6);
    await expect(page.getByRole("button", { name: "서버 점검 시작" })).toBeVisible();

    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(page.getByRole("heading", { name: "새 Ubuntu VPS를 그누보드7 서버로" })).toBeVisible();
    await expect(page.getByRole("button", { name: "서버 점검 시작" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${baseUrl}/setup/result?token=e2e`);
    const shellBox = await page.locator(".app-shell").boundingBox();
    expect(shellBox?.y).toBe(20);
    await expect(page.getByRole("heading", { name: "설치 결과" })).toBeVisible();
    await expect(page.locator("#live-log")).toHaveCount(1);
    await expect(page.locator("#install-live-log")).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "서버 구성 완료 · 웹 설치 대기" })).toBeVisible();
    await expect(page.getByText("1분 51초", { exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "핵심 구성 상태" })).toBeVisible();
    await page.getByText("PHP 및 런타임 상세", { exact: true }).click();
    await expect(page.getByRole("heading", { name: "PHP 환경 요약" })).toBeVisible();
    await expect(page.getByText("/etc/php/8.5/fpm/php.ini", { exact: true })).toBeVisible();
    await expect(page.getByText("mbstring, redis", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: /리포트 JSON/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /설정 안내서 MD/ })).toBeVisible();

    await page.getByRole("button", { name: "설치 안내서 보기" }).click();
    await expect(page).toHaveURL(/\/setup\/guide/);
    await expect(page.getByRole("heading", { name: "설치 안내서" })).toBeVisible();
    await expect(page.getByText("보안 경계 안내")).toBeVisible();
    await page.getByRole("button", { name: "설정 파일/값 확인" }).first().click();
    await expect(page.getByRole("heading", { name: "웹서버/vhost" })).toBeVisible();
    await expect(page.getByText("/etc/nginx/nginx.conf", { exact: true })).toBeVisible();
    await expect(page.locator("#provision-action-details")).toContainText("php_socket /run/php/php8.5-fpm-g7devops.sock");
    await page.locator("#provision-action-dialog").getByRole("button", { name: "닫기" }).click();

    await page.getByRole("button", { name: "설정 파일/값 확인" }).nth(1).click();
    await expect(page.locator("#provision-action-details code").filter({ hasText: "/etc/php/8.5/fpm/pool.d/g7-g7devops.conf" })).toBeVisible();
    await page.locator("#provision-action-dialog").getByRole("button", { name: "닫기" }).click();

    await page.getByRole("button", { name: "설정 파일/값 확인" }).nth(2).click();
    await expect(page.locator("#provision-action-details code").filter({ hasText: "/etc/mysql/conf.d/g7-installer.cnf" })).toBeVisible();
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("managed server check reports web install pending instead of fresh-server failure", async ({ page }, testInfo) => {
  const { server, baseUrl } = await startServer({
    doctor: {
      install_allowed: false,
      resources: {
        total_memory_mib: 2048,
        available_memory_mib: 1024,
        swap_total_mib: 2048,
        root_available_mib: 40000,
        root_inode_free_percent: 95,
      },
      checks: [
        { name: "ubuntu-version", status: "pass", message: "Ubuntu 24.04 확인" },
        { name: "nginx-service", status: "fail", message: "Nginx가 이미 실행 중입니다." },
        { name: "port-80", status: "fail", message: "TCP 80 포트를 사용 중입니다." },
        { name: "installer-state", status: "fail", message: "설치 상태 파일이 이미 있습니다." },
      ],
    },
  });
  try {
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();

    await expect(page.getByText("서버 구성 완료 · 웹 설치 대기", { exact: true }).first()).toBeVisible();
    await expect(page.getByText("서버 점검 실패", { exact: true })).toHaveCount(0);
    await expect(page.locator("#doctor-status")).toHaveClass(/hidden/);
    await expect(page.locator(".doctor-lifecycle .icon")).toHaveCount(5);
    await expect(page.locator(".doctor-lifecycle")).toContainText("서버 구성완료");
    await expect(page.locator(".doctor-lifecycle")).toContainText("g7devops 현재 확인됨");
    await expect(page.locator(".doctor-lifecycle")).toContainText("앱 파일배치 완료");
    await expect(page.locator(".doctor-lifecycle")).toContainText("그누보드7앱 파일 준비됨");
    await expect(page.getByRole("link", { name: "웹 설치 열기" })).toHaveAttribute("href", "https://g7devops.com/install/");
    await expect(page.getByText("웹 설치 마무리 / 초기화", { exact: true }).first()).toBeVisible();
    await expect(page.locator(".doctor-overview")).toContainText("신규 설치 보호");
    await expect(page.locator(".doctor-overview")).toContainText("1 통과 · 3 보호");
    await expect(page.locator(".doctor-details")).not.toHaveAttribute("open", "");
    await expect(page.locator('.result-row[data-display-status="protected"]')).toHaveCount(3);
    await expect(page.locator(".recovery-technical").first()).not.toHaveAttribute("open", "");
    await expect(page.locator('[data-view="check"] [data-recovery-action="resume"]')).toBeHidden();
    await expect(page.locator('[data-view="check"] [data-recovery-action="rollback"]')).toBeHidden();
    await expect(page.locator("#check-next-button")).toBeHidden();
    await page.screenshot({ path: testInfo.outputPath("managed-server-doctor.png"), fullPage: true });
    await page.locator(".doctor-details > summary").click();
    await expect(page.getByText("Nginx가 이미 실행 중입니다.", { exact: true })).toBeVisible();
    await expect(page.locator('.result-row[data-display-status="protected"] strong').first()).toHaveText("보호");
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("options expose MySQL 8.0 and 8.4 only with account autocomplete disabled", async ({ page }) => {
  const { server, baseUrl } = await startServer();
  try {
    await page.goto(`${baseUrl}/setup/options?token=e2e`);
    await expect(page.locator('input[name="database"]')).toHaveValue("mysql");
    await expect(page.locator('select[name="database_version"] option')).toHaveText([
      "MySQL 8.0 (Ubuntu 기본 APT)",
      "MySQL 8.4 LTS (공식 MySQL APT)",
    ]);
    await expect(page.locator('select[name="database_version"]')).toHaveValue("8.0");
    await expect(page.getByText("MariaDB", { exact: true })).toHaveCount(0);
    await expect(page.locator('input[name="site_user"]')).toHaveAttribute("autocomplete", "off");
    await expect(page.locator('#site-password')).toHaveAttribute("autocomplete", "off");
    await expect(page.locator('#database-password')).toHaveAttribute("autocomplete", "off");
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("stack profiles synchronize runtime versions and render desktop/mobile product views", async ({ page }, testInfo) => {
  const { server, baseUrl } = await startServer({ reportExists: false });
  try {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();
    await expect(page.locator(".doctor-overview")).toContainText("전체 메모리");
    await expect(page.locator(".doctor-overview")).toContainText("2.0 GB");
    await expect(page.locator(".doctor-details")).not.toHaveAttribute("open", "");
    await page.getByRole("button", { name: "다음: 설치 프로필" }).click();

    await expect(page.locator('input[name="stack_profile"][value="stable"]')).toBeChecked();
    await expect(page.locator('select[name="php_version"]')).toHaveValue("8.3");
    await expect(page.locator('select[name="database_version"]')).toHaveValue("8.0");
    await expect(page.locator("#stack-profile-label")).toHaveText("운영 권장");
    await expect(page.locator("#site-credential-lane")).toHaveAttribute("data-state", "active");
    await expect(page.locator("#database-credential-lane")).toHaveAttribute("data-state", "pending");

    await page.fill("#site-password", "Test-only_9x!");
    await page.fill("#site-password-confirm", "Test-only_9x!");
    await expect(page.locator("#site-credential-lane")).toHaveAttribute("data-state", "pass");
    await expect(page.locator("#database-credential-lane")).toHaveAttribute("data-state", "active");

    await page.fill("#database-password", "Test-only_9x!");
    await page.fill("#database-password-confirm", "Test-only_9x!");
    await expect(page.locator("#database-credential-lane")).toHaveAttribute("data-state", "pass");
    await expect(page.locator("#credential-state")).toHaveText("입력 확인 완료");

    await page.fill("#site-password", "");
    await page.fill("#site-password-confirm", "");
    await page.fill("#database-password", "");
    await page.fill("#database-password-confirm", "");

    await page.locator('label.profile-segment:has(input[name="stack_profile"][value="latest"])').click();
    await expect(page.locator('select[name="php_version"]')).toHaveValue("8.5");
    await expect(page.locator('select[name="database_version"]')).toHaveValue("8.4");
    await expect(page.locator("#stack-repository-label")).toContainText("MySQL 공식 APT");

    await page.locator('.runtime-segments label:has(input[name="web_server"][value="apache"])').click();
    await expect(page.locator('input[name="install_template"]')).toHaveValue("apache");
    await expect(page.locator("#stack-web-product")).toHaveAttribute("data-product", "apache");
    await expect(page.locator("#stack-web-label")).toHaveText("Apache");

    await page.locator('label.profile-segment:has(input[name="stack_profile"][value="custom"])').click();
    await page.locator('select[name="php_version"]').selectOption("8.3");
    await expect(page.locator('input[name="stack_profile"][value="custom"]')).toBeChecked();
    await expect(page.locator("#advanced-settings")).toHaveAttribute("open", "");

    await page.locator('label.profile-segment:has(input[name="stack_profile"][value="stable"])').click();
    await page.locator('.runtime-segments label:has(input[name="web_server"][value="nginx"])').click();
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.screenshot({
      path: testInfo.outputPath("profile-desktop.png"),
      animations: "disabled",
      fullPage: false,
    });

    await page.setViewportSize({ width: 390, height: 844 });
    await page.evaluate(() => window.scrollTo(0, 0));
    await expect(page.locator(".product-mark")).toBeVisible();
    await expect(page.locator(".wizard-progress .step")).toHaveCount(6);
    expect(await page.locator(".wizard-progress .step").first().evaluate((element) => element.getBoundingClientRect().height)).toBeLessThanOrEqual(5);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({
      path: testInfo.outputPath("profile-mobile.png"),
      animations: "disabled",
      fullPage: false,
    });
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("manually cleared database identifiers stay empty and block the next step", async ({ page }) => {
  const { server, baseUrl } = await startServer({ reportExists: false });
  try {
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();
    await page.getByRole("button", { name: "다음: 설치 프로필" }).click();
    await expect(page).toHaveURL(/\/setup\/options/);
    const databaseName = page.locator('#database-name-input');
    const databaseUser = page.locator('#database-user-input');

    await expect(databaseName).not.toHaveValue("");
    await expect(databaseUser).not.toHaveValue("");
    await databaseName.fill("");
    await databaseUser.fill("");

    await expect(databaseName).toHaveValue("");
    await expect(databaseUser).toHaveValue("");
    await expect(page.getByRole("button", { name: "다음: 설치 검토" })).toBeDisabled();

    await page.reload();
    await expect(databaseName).toHaveValue("");
    await expect(databaseUser).toHaveValue("");
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("legacy saved database options migrate to MySQL 8.0", async ({ page }) => {
  const { server, baseUrl } = await startServer();
  try {
    await page.goto(`${baseUrl}/setup/options?token=e2e`);
    await page.evaluate(() => {
      sessionStorage.setItem("g7inst-wizard-state-v2", JSON.stringify({
        activeStep: "options",
        form: {
          database: "mariadb",
          database_version: "apt-default",
        },
      }));
    });
    await page.reload();

    await expect(page.locator('input[name="database"]')).toHaveValue("mysql");
    await expect(page.locator('select[name="database_version"]')).toHaveValue("8.0");
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("saved interrupted report exposes the exact failed check", async ({ page }) => {
  const report = mockReport();
  report.phase = "app-configured";
  report.problem = null;
  report.certbot_checks = [{
    name: "tls-config",
    status: "fail",
    message: "TLS configuration failed: renewal webroot points to /home/old/public_html/public",
  }];
  const { server, baseUrl } = await startServer({ report });
  try {
    await page.goto(`${baseUrl}/setup/result?token=e2e`);
    await expect(page.getByText("중단 원인")).toBeVisible();
    await expect(page.getByText("TLS configuration failed: renewal webroot points to /home/old/public_html/public", { exact: true })).toBeVisible();
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("restored failed step is offered as an in-place retry", async ({ page }) => {
  const { server, baseUrl } = await startServer({
    recovery: {
      can_resume: true,
      can_retry_step: true,
      can_reset: true,
      can_rollback: false,
      recommended_action: "resume",
      failed_step: "runtime",
      restore_status: "restored",
      message: "실패한 단계의 변경을 복원한 뒤 해당 단계부터 다시 실행할 수 있습니다.",
      metadata_paths: ["/var/lib/g7-installer/state.json"],
      rollback_reason: "현재 단계에서는 패키지 되돌리기를 사용할 수 없습니다.",
      resume_reason: null,
    },
  });
  try {
    await page.goto(`${baseUrl}/setup/result?token=e2e`);
    const message = page.locator("[data-recovery-summary]:visible [data-recovery-summary-message]");
    await expect(message).toContainText("실패 단계: runtime");
    await expect(message).toContainText("자동 복원되었습니다");
    await expect(page.locator('[data-view="report"] [data-recovery-action="resume"]')).toHaveText(/수정 후 현재 단계 재실행/);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("plan route auto-generates a review after doctor pass", async ({ page }) => {
  const { server, baseUrl } = await startServer({ reportExists: false });
  try {
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();
    await expect(page.getByText("서버 점검 통과")).toBeVisible();

    await page.getByRole("button", { name: "다음: 설치 프로필" }).click();
    await expect(page.locator('input[name="install_template"][value="frankenphp"]')).toHaveCount(0);
    await expect(page.locator('input[name="install_template"][value="frankenphp-octane"]')).toHaveCount(0);
    await expect(page.locator('input[name="app_package"][value="gnuboard7-octane"]')).toHaveCount(0);
    await expect(page.locator('input[name="app_package"][value="laravel"]')).toHaveCount(0);
    await expect(page.locator('input[name="app_package"][value="laravel-octane"]')).toHaveCount(0);
    await expect(page.locator('input[name="app_package"][value="wordpress"]')).toHaveCount(0);
    await expect(page.locator('select[name="web_server"] option[value="frankenphp"]')).toHaveCount(0);
    await page.fill("#site-password", "Test-only_9x!");
    await page.fill("#site-password-confirm", "Test-only_9x!");
    await page.fill("#database-name-input", "g7devops");
    await page.fill("#database-user-input", "g7devops");
    await page.fill("#database-password", "Test-only_9x!");
    await page.fill("#database-password-confirm", "Test-only_9x!");
    await page.getByRole("button", { name: "다음: 설치 검토" }).click();

    await expect(page).toHaveURL(/\/setup\/plan/);
    await expect(page.getByText("선택한 설치 사양")).toBeVisible();
    await expect(page.locator("details.plan-details[open]")).toHaveCount(0);
    await expect(page.locator("details.plan-details .plan-summary-title .icon")).toHaveCount(5);
    await expect(page.getByRole("button", { name: "검토 완료, 설치로 이동" })).toBeEnabled();
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("install uses one modal graph and groups PHP packages", async ({ page }, testInfo) => {
  const { server, baseUrl } = await startServer({ reportExists: false, installDelayMs: 900 });
  try {
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();
    await page.getByRole("button", { name: "다음: 설치 프로필" }).click();
    for (const [selector, value] of [
      ["#site-password", "Test-only_9x!"],
      ["#site-password-confirm", "Test-only_9x!"],
      ["#database-name-input", "g7devops"],
      ["#database-user-input", "g7devops"],
      ["#database-password", "Test-only_9x!"],
      ["#database-password-confirm", "Test-only_9x!"],
    ]) {
      await page.fill(selector, value);
    }
    await page.getByRole("button", { name: "다음: 설치 검토" }).click();
    await page.getByRole("button", { name: "검토 완료, 설치로 이동" }).click();
    await expect(page.getByRole("heading", { name: "그누보드7" })).toBeVisible();

    await page.getByRole("button", { name: "설치 시작" }).click();
    await page.getByRole("button", { name: "시작", exact: true }).click();

    await expect(page.locator("#install-progress-dialog[open]")).toBeVisible();
    await expect(page.locator(".execution-brand")).toContainText("Provisioning run");
    await expect(page.locator("#install-stages .install-graph-node")).toHaveCount(9);
    await expect(page.locator('[data-package-group="php"]')).toContainText("PHP 8.5 런타임 · 2개");
    await expect(page.locator("#install-log-slot #live-log")).toHaveCount(1);
    await expect(page.locator("#install-log-slot #log-dock[open]")).toHaveCount(1);
    await expect(page.locator("#install-log-slot #live-log")).toBeVisible();
    await expect(page.locator("#live-log")).toHaveCount(1);
    await expect(page.locator("dialog[open]")).toHaveCount(1);
    await page.waitForTimeout(220);
    await page.screenshot({ path: testInfo.outputPath("install-progress-modal.png"), fullPage: false });

    await expect(page).toHaveURL(/\/setup\/result/, { timeout: 5000 });
    await expect(page.locator("#install-progress-dialog[open]")).toHaveCount(0);
    await expect(page.locator("#log-dock-home #live-log")).toHaveCount(1);
    await expect(page.locator("#reset-button")).toBeEnabled();
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("failed install unlocks the modal and exposes the restored step retry", async ({ page }) => {
  const { server, baseUrl } = await startServer({
    reportExists: false,
    installFailure: true,
    recovery: {
      can_resume: true,
      can_retry_step: true,
      can_reset: true,
      can_rollback: false,
      recommended_action: "resume",
      failed_step: "vhost",
      restore_status: "restored",
      message: "실패한 단계의 변경을 복원한 뒤 해당 단계부터 다시 실행할 수 있습니다.",
      metadata_paths: ["/var/lib/g7-installer/state.json"],
      rollback_reason: "현재 단계에서는 패키지 되돌리기를 사용할 수 없습니다.",
      resume_reason: null,
    },
  });
  try {
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();
    await page.getByRole("button", { name: "다음: 설치 프로필" }).click();
    for (const [selector, value] of [
      ["#site-password", "Test-only_9x!"],
      ["#site-password-confirm", "Test-only_9x!"],
      ["#database-name-input", "g7devops"],
      ["#database-user-input", "g7devops"],
      ["#database-password", "Test-only_9x!"],
      ["#database-password-confirm", "Test-only_9x!"],
    ]) {
      await page.fill(selector, value);
    }
    await page.getByRole("button", { name: "다음: 설치 검토" }).click();
    await page.getByRole("button", { name: "검토 완료, 설치로 이동" }).click();
    await page.getByRole("button", { name: "설치 시작" }).click();
    await page.getByRole("button", { name: "시작", exact: true }).click();

    const closeButton = page.locator("#install-progress-close");
    await expect(closeButton).toHaveText(/닫고 실패 내용 확인/);
    await expect(closeButton).toBeEnabled();
    await closeButton.click();
    await expect(page.locator("#install-progress-dialog[open]")).toHaveCount(0);
    await expect(page.locator('[data-view="report"] [data-recovery-action="resume"]')).toBeVisible();
    await expect(page.locator('[data-view="report"] [data-recovery-action="resume"]')).toBeEnabled();
    await expect(page.locator('[data-view="report"] [data-recovery-action="resume"]')).toHaveText(/수정 후 현재 단계 재실행/);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("reset overlay shows the shared live log expanded", async ({ page }, testInfo) => {
  const { server, baseUrl } = await startServer({ resetDelayMs: 900 });
  try {
    await page.setViewportSize({ width: 610, height: 476 });
    await page.goto(`${baseUrl}/setup/result?token=e2e`);
    await page.getByText("재설치 및 초기화", { exact: true }).click();
    await page.locator("#reset-button").click();
    await expect(page.locator("#recovery-confirm-yes")).toBeDisabled();
    await page.fill("#recovery-confirm-input", "초기화");
    await expect(page.locator("#recovery-confirm-yes")).toBeEnabled();
    await page.locator("#recovery-confirm-yes").click();

    await expect(page.locator("#operation-overlay:not([hidden])")).toBeVisible();
    await expect(page.locator("#operation-overlay-log-slot #live-log")).toHaveCount(1);
    await expect(page.locator("#operation-overlay-log-slot #log-dock[open]")).toHaveCount(1);
    await expect(page.locator("#operation-overlay-log-slot #live-log")).toBeVisible();
    const resetOverflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(resetOverflow).toBeLessThanOrEqual(1);
    await page.screenshot({ path: testInfo.outputPath("reset-live-log-overlay.png"), fullPage: false });

    await expect(page.getByRole("heading", { name: "초기화 완료되었습니다." })).toBeVisible();
    await page.locator("#operation-overlay-confirm").click();
    await expect(page.locator("#operation-overlay[hidden]")).toHaveCount(1);
    await expect(page.locator("#log-dock-home #live-log")).toHaveCount(1);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("reset confirmation warns when a completed G7 install is detected", async ({ page }) => {
  const { server, baseUrl } = await startServer({
    recovery: {
      can_resume: false,
      can_retry_step: false,
      can_reset: true,
      can_rollback: false,
      recommended_action: "manual",
      failed_step: null,
      restore_status: null,
      message: "그누보드7 DB 생성 기록과 설치 완료 잠금 파일을 확인했습니다.",
      metadata_paths: ["/var/lib/g7-installer/state.json"],
      rollback_reason: "운영 데이터가 있어 패키지 되돌리기를 사용할 수 없습니다.",
      resume_reason: "설치가 이미 완료되었습니다.",
      g7_database_created: true,
      g7_database_confirmed: true,
      g7_database_name: "g7devops",
      server_configured: true,
      app_files_prepared: true,
      g7_install_completed: true,
      g7_install_lock_path: "/home/g7devops/public_html/storage/app/g7_installed",
      app_install_url: "https://g7devops.com/",
      lifecycle_status: "app-installed",
    },
  });
  try {
    await page.goto(`${baseUrl}/setup/result?token=e2e`);
    await page.getByText("재설치 및 초기화", { exact: true }).click();
    await page.locator("#reset-button").click();
    await expect(page.getByText(/이미 설치가 완료된 사이트/)).toBeVisible();
    await expect(page.getByText(/웹파일 전체, DB\/DB 계정/)).toBeVisible();
    await expect(page.getByText(/복구할 수 없습니다/)).toBeVisible();
    await expect(page.locator("#recovery-confirm-yes")).toBeDisabled();
    await page.fill("#recovery-confirm-input", "초기화 아님");
    await expect(page.locator("#recovery-confirm-yes")).toBeDisabled();
    await page.fill("#recovery-confirm-input", "초기화");
    await expect(page.locator("#recovery-confirm-yes")).toBeEnabled();
    await page.getByRole("button", { name: "아니오" }).click();
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("mobile plan keeps one-column flow without horizontal overflow", async ({ page }) => {
  const { server, baseUrl } = await startServer({ reportExists: false });
  try {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();
    await page.getByRole("button", { name: "다음: 설치 프로필" }).click();
    for (const [selector, value] of [
      ["#site-password", "Test-only_9x!"],
      ["#site-password-confirm", "Test-only_9x!"],
      ["#database-name-input", "g7devops"],
      ["#database-user-input", "g7devops"],
      ["#database-password", "Test-only_9x!"],
      ["#database-password-confirm", "Test-only_9x!"],
    ]) {
      await page.fill(selector, value);
    }
    await page.getByRole("button", { name: "다음: 설치 검토" }).click();
    await expect(page.getByText("선택한 설치 사양")).toBeVisible();
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(overflow).toBeLessThanOrEqual(1);
    const columns = await page.locator(".plan-detail-grid").evaluate((element) => getComputedStyle(element).gridTemplateColumns);
    expect(columns.trim().split(/\s+/)).toHaveLength(1);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});
