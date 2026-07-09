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
    database_version: "mysql-8.4",
    database_name: "g7devops",
    database_user: "g7devops",
    database_password_policy: "user-provided-store-root-only",
    site_user: "g7devops",
    web_root: "/home/g7devops/public_html",
    www_mode: "redirect-to-www",
    redis: "enable",
    mail_mode: "local-postfix",
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
    completed_steps: ["packages-installed", "vhost-enabled", "runtime-configured", "database-configured", "certbot-issued", "app-source-prepared", "setup-guide-written", "backup-manifest-written"],
    safety_checks: [{ name: "fresh-server", status: "pass", message: "신규 VPS 조건을 통과했습니다." }],
    preinstall_package_checks: [],
    package_checks: [{ name: "nginx", status: "pass", message: "패키지 설치 확인 완료" }],
    service_checks: [{ name: "nginx", status: "pass", message: "서비스가 실행 중입니다." }],
    port_checks: [{ name: "port-80", status: "pass", message: "80 포트 확인" }],
    network_checks: [],
    runtime_checks: [{ name: "php-runtime-limits", status: "pass", message: "PHP 한도 설정 확인" }],
    database_checks: [{ name: "database-user", status: "pass", message: "DB 계정 확인" }],
    firewall_checks: [{ name: "ufw-policy", status: "manual", message: "보안 카드에서 확인하세요." }],
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
    database_version: "mysql-8.4",
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
  if (pathname === "/app.css") {
    return readFile(path.join(root, "web/dist/app.css"));
  }
  if (pathname === "/promo.sample.json" || pathname === "/promo.json") {
    return readFile(path.join(root, "web/promo.sample.json"));
  }
  return null;
}

async function startServer() {
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
      json(response, {
        can_reset: true,
        can_rollback: false,
        recommended_action: "reset",
        message: "설치기 소유 기록이 있어 재설치 초기화가 가능합니다.",
        metadata_paths: ["/var/lib/g7-installer/state.json"],
        rollback_reason: "앱/DB/인증서 단계 이후에는 reset을 사용합니다.",
      });
      return;
    }
    if (pathname === "/api/report") {
      json(response, {
        exists: true,
        path: "/var/log/g7-installer/report.json",
        content: JSON.stringify(mockReport()),
      });
      return;
    }
    if (pathname === "/api/doctor") {
      json(response, {
        install_allowed: true,
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
    if (pathname === "/api/provision/action") {
      json(response, {
        action: "security",
        status: "manual",
        message: "보안 설정은 현재 점검 모드입니다.",
        checks: [{ name: "security-policy", status: "manual", message: "UFW/SSH 정책을 확인하세요." }],
      });
      return;
    }

    const file = await asset(pathname);
    if (file) {
      const contentType = pathname.endsWith(".css")
        ? "text/css; charset=utf-8"
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

test("wizard routes render report, downloads, and provision cards", async ({ page }) => {
  const { server, baseUrl } = await startServer();
  try {
    await page.goto(`${baseUrl}/setup/result?token=e2e`);
    await expect(page.getByRole("heading", { name: "결과 리포트" })).toBeVisible();
    await expect(page.getByText("설치 완료 상태")).toBeVisible();
    await expect(page.getByRole("button", { name: /리포트 JSON/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /설정 안내서 MD/ })).toBeVisible();

    await page.getByRole("button", { name: "세부 설정으로 이동" }).click();
    await expect(page).toHaveURL(/\/setup\/provision/);
    await expect(page.getByRole("heading", { name: "세부 설정 적용/점검" })).toBeVisible();
    await expect(page.getByText("보안/방화벽")).toBeVisible();
    await page.getByRole("button", { name: "설정 파일/값 확인" }).first().click();
    await expect(page.getByRole("heading", { name: "웹서버/vhost" })).toBeVisible();
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("plan route auto-generates a review after doctor pass", async ({ page }) => {
  const { server, baseUrl } = await startServer();
  try {
    await page.goto(`${baseUrl}/setup/doctor?token=e2e`);
    await page.getByRole("button", { name: "점검 실행" }).click();
    await expect(page.getByText("서버 점검 통과")).toBeVisible();

    await page.getByRole("button", { name: "다음: 설치 방식" }).click();
    await page.fill("#site-password", "0808dong!!");
    await page.fill("#site-password-confirm", "0808dong!!");
    await page.fill("#database-name-input", "g7devops");
    await page.fill("#database-user-input", "g7devops");
    await page.fill("#database-password", "0808dong!!");
    await page.fill("#database-password-confirm", "0808dong!!");
    await page.getByRole("button", { name: "다음: 사양 확정" }).last().click();

    await expect(page).toHaveURL(/\/setup\/plan/);
    await expect(page.getByText("선택한 설치 사양")).toBeVisible();
    await expect(page.getByRole("button", { name: "이 사양으로 진행" })).toBeEnabled();
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});
