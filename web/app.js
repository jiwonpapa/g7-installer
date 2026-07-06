const state = {
  activeStep: "login",
  bootstrap: null,
  socket: null,
  csrfToken: null,
  authenticated: false,
  planReady: false,
  theme: localStorage.getItem("g7inst-theme") || "light",
};

const nodes = {
  status: document.querySelector("#connection-status"),
  themeToggle: document.querySelector("#theme-toggle"),
  log: document.querySelector("#live-log"),
  domain: document.querySelector("#domain-input"),
  mode: document.querySelector("#deployment-mode"),
  customWebRoot: document.querySelector("#custom-web-root"),
  webRootMode: document.querySelector("#web-root-mode"),
  mailMode: document.querySelector("#mail-mode"),
  smtpHost: document.querySelector("#smtp-host"),
  smtpPort: document.querySelector("#smtp-port"),
  smtpFrom: document.querySelector("#smtp-from"),
  smtpEncryption: document.querySelector("#smtp-encryption"),
  optionsForm: document.querySelector("#options-form"),
  planOutput: document.querySelector("#plan-output"),
  reportOutput: document.querySelector("#report-output"),
  doctorResults: document.querySelector("#doctor-results"),
  loginStatus: document.querySelector("#login-status"),
  doctorStatus: document.querySelector("#doctor-status"),
  planStatus: document.querySelector("#plan-status"),
  installStatus: document.querySelector("#install-status"),
  reportStatus: document.querySelector("#report-status"),
  installProgress: document.querySelector("#install-progress"),
  confirmSpecButton: document.querySelector("#confirm-spec-button"),
  summaryDomain: document.querySelector("#summary-domain"),
  summaryMode: document.querySelector("#summary-mode"),
  summaryRuntime: document.querySelector("#summary-runtime"),
  summaryData: document.querySelector("#summary-data"),
};

const stepOrder = ["login", "check", "options", "plan", "install", "report"];

const statusLabel = {
  pass: "통과",
  warn: "주의",
  fail: "실패",
  pending: "대기",
};

const checkLabel = {
  "ubuntu-version": "Ubuntu 버전",
  privilege: "실행 권한",
  "nginx-service": "Nginx 서비스",
  "apache-service": "Apache 서비스",
  "port-80": "80 포트",
  "port-443": "443 포트",
  "nginx-config": "Nginx 설정",
  "apache-config": "Apache 설정",
  "g7-web-root": "기존 웹루트",
  "installer-state": "설치 상태 파일",
  "owned-files": "설치기 소유 파일",
  "certbot-live": "인증서 흔적",
};

const errorLabel = {
  "server account login is required": "서버 계정 로그인이 필요합니다.",
  "missing setup session cookie": "설치 세션 쿠키가 없습니다.",
  "setup session expired or invalid": "설치 세션이 만료되었거나 올바르지 않습니다.",
  "missing CSRF token": "보안 확인 토큰이 없습니다.",
  "invalid CSRF token": "보안 확인 토큰이 올바르지 않습니다.",
  "install is already running": "설치 작업이 이미 진행 중입니다.",
  "reset is blocked while install is running": "설치 중에는 리셋할 수 없습니다.",
};

const templates = {
  recommended: {
    domain: null,
    deployment_mode: "public",
    web_server: "nginx",
    php_version: "8.5",
    database: "mysql",
    redis: "enable",
    mail_mode: "none",
    web_root_mode: "public-html",
    www_mode: "redirect-to-root",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
  apache: {
    domain: null,
    deployment_mode: "public",
    web_server: "apache",
    php_version: "8.5",
    database: "mysql",
    redis: "enable",
    mail_mode: "none",
    web_root_mode: "public-html",
    www_mode: "redirect-to-root",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
  local: {
    domain: "g7-test.local",
    deployment_mode: "local-test",
    web_server: "nginx",
    php_version: "8.3",
    database: "mysql",
    redis: "enable",
    mail_mode: "none",
    web_root_mode: "public-html",
    www_mode: "redirect-to-root",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
};

async function withBusy(button, busyText, task) {
  const originalText = button.textContent;
  button.disabled = true;
  if (busyText) {
    button.textContent = busyText;
  }

  try {
    return await task();
  } finally {
    button.disabled = false;
    button.textContent = originalText;
  }
}

function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  nodes.log.textContent += `\n[${timestamp}] ${message}`;
  nodes.log.scrollTop = nodes.log.scrollHeight;
}

function formatError(error) {
  const lines = [localizeMessage(error?.message || String(error))];

  if (error?.hint) {
    lines.push("", `도움말: ${error.hint}`);
  }

  if (Array.isArray(error?.details) && error.details.length > 0) {
    lines.push("", "상세:", ...error.details.map((detail) => `- ${detail}`));
  }

  return lines.join("\n");
}

function localizeMessage(message) {
  return errorLabel[message] || message;
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[char]);
}

function setConnectionStatus(label, badgeClass) {
  nodes.status.textContent = label;
  nodes.status.className = `badge ${badgeClass}`;
}

function setAlert(node, type, title, message) {
  if (!node) {
    return;
  }

  const classByType = {
    success: "alert alert-success",
    error: "alert alert-error",
    warning: "alert alert-warning",
    info: "alert alert-info",
  };

  node.className = `${classByType[type] || classByType.info} mt-5`;
  node.innerHTML = `<div><strong>${escapeHtml(title)}</strong><p class="mt-1 whitespace-pre-line text-sm">${escapeHtml(message)}</p></div>`;
}

function hideAlert(node) {
  if (!node) {
    return;
  }

  node.className = "hidden";
  node.innerHTML = "";
}

function setPlanReady(ready) {
  state.planReady = ready;
  if (nodes.confirmSpecButton) {
    nodes.confirmSpecButton.disabled = !ready;
  }
}

function normalizedStep(step) {
  return stepOrder.includes(step) ? step : "login";
}

function stepUrl(step) {
  return `${window.location.pathname}#${step}`;
}

function writeStepHistory(step, replace) {
  const method = replace ? "replaceState" : "pushState";
  window.history[method]({ step }, document.title, stepUrl(step));
}

function applyTheme(theme) {
  state.theme = theme;
  document.documentElement.dataset.theme = theme;
  localStorage.setItem("g7inst-theme", theme);
  nodes.themeToggle.textContent = theme === "dark" ? "라이트 모드" : "다크 모드";
}

function showStep(nextStep, options = {}) {
  const step = normalizedStep(nextStep);
  const shouldPushHistory = options.pushHistory !== false;
  const wasActiveStep = state.activeStep === step;

  state.activeStep = step;
  const activeIndex = stepOrder.indexOf(step);

  document.querySelectorAll("[data-view]").forEach((view) => {
    view.classList.toggle("is-visible", view.dataset.view === step);
  });

  document.querySelectorAll("[data-step]").forEach((button) => {
    button.classList.toggle("is-active", button.dataset.step === step);
  });

  document.querySelectorAll("[data-progress]").forEach((item) => {
    const index = stepOrder.indexOf(item.dataset.progress);
    item.classList.toggle("step-primary", index >= 0 && index <= activeIndex);
  });

  if (shouldPushHistory && !wasActiveStep) {
    writeStepHistory(step, false);
  }
}

function optionPayload() {
  const form = new FormData(nodes.optionsForm);
  const mode = form.get("deployment_mode");
  const mailMode = form.get("mail_mode");
  const customWebRoot = form.get("web_root")?.trim();

  return {
    domain: form.get("domain")?.trim() || "example.com",
    local_test: mode === "local-test",
    web_server: form.get("web_server"),
    php_version: form.get("php_version"),
    database: form.get("database"),
    site_user: form.get("site_user")?.trim() || "g7",
    web_root_mode: form.get("web_root_mode"),
    web_root: customWebRoot || null,
    www_mode: form.get("www_mode"),
    redis: form.get("redis"),
    mail_mode: mailMode,
    smtp_host: mailMode === "smtp-relay" ? form.get("smtp_host")?.trim() : null,
    smtp_port: mailMode === "smtp-relay" ? Number(form.get("smtp_port") || 587) : 587,
    smtp_from: mailMode === "smtp-relay" ? form.get("smtp_from")?.trim() : null,
    smtp_encryption: mailMode === "smtp-relay" ? form.get("smtp_encryption") : "starttls",
    security_profile: form.get("security_profile"),
    ssh_policy: form.get("ssh_policy"),
    rollback: true,
    preserve_config: true,
    dns_check: mode !== "local-test",
  };
}

function setFormValue(name, value) {
  const field = nodes.optionsForm.elements[name];
  if (!field || value === null || value === undefined) {
    return;
  }

  field.value = value;
}

function applyTemplate(templateName) {
  const template = templates[templateName];
  if (!template) {
    return;
  }

  Object.entries(template).forEach(([name, value]) => {
    if (name === "domain" && value === null) {
      return;
    }
    setFormValue(name, value);
  });

  refreshFormState();
}

function refreshFormState() {
  setPlanReady(false);
  const webRootIsCustom = nodes.webRootMode.value === "custom";
  nodes.customWebRoot.disabled = !webRootIsCustom;
  if (!webRootIsCustom) {
    nodes.customWebRoot.value = "";
  }

  const smtpEnabled = nodes.mailMode.value === "smtp-relay";
  [nodes.smtpHost, nodes.smtpPort, nodes.smtpFrom, nodes.smtpEncryption].forEach((node) => {
    node.disabled = !smtpEnabled;
  });

  refreshSummary();
}

function refreshSummary() {
  const payload = optionPayload();
  nodes.summaryDomain.textContent = payload.domain;
  nodes.summaryMode.textContent = payload.local_test ? "로컬 테스트" : "실제 도메인";
  nodes.summaryRuntime.textContent = `${runtimeLabel(payload.web_server)} / PHP ${payload.php_version}`;
  nodes.summaryData.textContent = `${databaseLabel(payload.database)} / Redis ${payload.redis === "enable" ? "사용" : "미사용"}`;
}

function renderDraftPlan() {
  const payload = optionPayload();
  nodes.planOutput.textContent = [
    "설치 계획 요청값",
    `도메인: ${payload.domain}`,
    `모드: ${payload.local_test ? "로컬 테스트" : "실제 도메인"}`,
    `웹서버: ${runtimeLabel(payload.web_server)}`,
    `PHP: ${payload.php_version}`,
    `데이터베이스: ${databaseLabel(payload.database)}`,
    `사이트 계정: ${payload.site_user}`,
    `웹루트 방식: ${payload.web_root_mode}`,
    `www 처리: ${payload.www_mode}`,
    `Redis: ${payload.redis === "enable" ? "사용" : "미사용"}`,
    `메일: ${payload.mail_mode}`,
    `보안 수준: ${payload.security_profile}`,
    `SSH 정책: ${payload.ssh_policy}`,
    "",
    "계획 생성 버튼을 누르면 실제 plan 결과로 교체됩니다.",
  ].join("\n");
}

function runtimeLabel(value) {
  return value === "apache" ? "Apache" : "Nginx";
}

function databaseLabel(value) {
  return value === "mariadb" ? "MariaDB" : "MySQL";
}

function renderDoctor(report) {
  nodes.doctorResults.innerHTML = "";

  setAlert(
    nodes.doctorStatus,
    report.install_allowed ? "success" : "error",
    report.install_allowed ? "서버 점검 통과" : "서버 점검 실패",
    report.install_allowed
      ? "설치 준비를 계속 진행할 수 있습니다."
      : "실패 항목을 해결한 뒤 다시 점검하세요.",
  );

  report.checks.forEach((check) => {
    const item = document.createElement("div");
    item.className = "result-row";
    item.dataset.status = check.status;
    item.innerHTML = `
      <div class="result-copy">
        <span>${escapeHtml(checkLabel[check.name] || check.name)}</span>
        <p>${escapeHtml(check.message)}</p>
      </div>
      <strong>${escapeHtml(statusLabel[check.status] || check.status)}</strong>
    `;
    nodes.doctorResults.append(item);
  });
}

async function runDoctorCheck() {
  hideAlert(nodes.doctorStatus);
  log("서버 점검 실행");
  const report = await apiFetch("/api/doctor");
  renderDoctor(report);
  log(`서버 점검 완료: install_allowed=${report.install_allowed}`);
  return report;
}

function markStage(stage, status) {
  const row = document.querySelector(`[data-stage="${stage}"]`);
  if (!row) {
    return;
  }

  row.dataset.status = status;
  row.querySelector("strong").textContent = status;
  updateInstallProgress();
}

function updateInstallProgress() {
  if (!nodes.installProgress) {
    return;
  }

  const rows = Array.from(document.querySelectorAll("[data-stage]"));
  const done = rows.filter((row) => row.dataset.status === "성공").length;
  const failed = rows.some((row) => row.dataset.status === "실패");
  nodes.installProgress.value = failed ? 100 : Math.round((done / rows.length) * 100);
}

function connectEvents() {
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  const socket = new WebSocket(`${protocol}://${window.location.host}/api/events`);
  state.socket = socket;

  socket.addEventListener("open", () => {
    setConnectionStatus("연결됨", "badge-success");
  });

  socket.addEventListener("message", (event) => {
    let payload;
    try {
      payload = JSON.parse(event.data);
    } catch (_error) {
      log("invalid event payload received");
      return;
    }

    if (payload.event_type === "log") {
      log(payload.message);
    }
    if (payload.event_type === "stage" && payload.stage && payload.status) {
      markStage(payload.stage, payload.status);
      log(payload.message);
    }
  });

  socket.addEventListener("close", () => {
    setConnectionStatus("연결 끊김", "badge-warning");
  });

  socket.addEventListener("error", () => {
    setConnectionStatus("연결 오류", "badge-error");
  });
}

async function loadBootstrap() {
  return apiFetch("/api/bootstrap");
}

async function apiFetch(path, options = {}) {
  const headers = {
    "content-type": "application/json",
    ...(options.headers || {}),
  };
  if (state.csrfToken) {
    headers["x-g7-csrf"] = state.csrfToken;
  }

  let response;
  try {
    response = await fetch(path, {
      ...options,
      credentials: "same-origin",
      headers,
    });
  } catch (cause) {
    const error = new Error("setup controller request failed");
    error.hint = "서버 프로세스가 실행 중인지 확인하고 브라우저를 새로고침하세요.";
    error.details = [cause?.message || String(cause)];
    error.retryable = true;
    throw error;
  }

  const contentType = response.headers.get("content-type") || "";
  let body;
  try {
    body = contentType.includes("application/json") ? await response.json() : await response.text();
  } catch (cause) {
    const error = new Error("setup controller response could not be parsed");
    error.hint = "웹 컨트롤러를 재시작한 뒤 같은 작업을 다시 실행하세요.";
    error.details = [cause?.message || String(cause)];
    error.retryable = true;
    throw error;
  }

  if (!response.ok) {
    const message = body && typeof body === "object" && body.error ? body.error : `request failed: ${response.status}`;
    const error = new Error(message);
    error.status = response.status;
    if (body && typeof body === "object") {
      error.hint = body.hint || null;
      error.details = Array.isArray(body.details) ? body.details : [];
      error.retryable = Boolean(body.retryable);
    }
    throw error;
  }

  return body;
}

function renderInstallReport(report) {
  nodes.reportOutput.textContent = [
    "설치 준비 완료",
    `도메인: ${report.domain}`,
    `모드: ${report.deployment_mode === "local-test" ? "로컬 테스트" : "실제 도메인"}`,
    `웹서버: ${runtimeLabel(report.web_server)}`,
    `PHP: ${report.php_version}`,
    `데이터베이스: ${databaseLabel(report.database)}`,
    `사이트 계정: ${report.site_user}`,
    `웹루트: ${report.web_root}`,
    `단계: ${report.phase}`,
    `상태 파일: ${report.state_path}`,
    `소유 파일 목록: ${report.owned_files_path}`,
    "",
    "완료된 작업:",
    ...report.completed_steps.map((step) => `- ${step}`),
  ].join("\n");

  ["preflight", "packages", "config", "services", "ports", "http", "report"].forEach((stage) => markStage(stage, "성공"));
  nodes.installProgress.value = 100;
}

function renderResetReport(report) {
  nodes.reportOutput.textContent = [
    "리셋 완료",
    `미리보기: ${report.dry_run}`,
    "",
    "삭제됨:",
    ...(report.removed.length ? report.removed.map((path) => `- ${path}`) : ["- 없음"]),
    "",
    "이미 없던 항목:",
    ...(report.missing.length ? report.missing.map((path) => `- ${path}`) : ["- 없음"]),
  ].join("\n");
}

function renderPlanReport(report) {
  const packages = report.packages.length
    ? report.packages.map((item) => `- ${item.name}: ${item.description}`).join("\n")
    : "- 없음";
  const files = report.files.length
    ? report.files.map((item) => `- ${item.path} (${item.action})`).join("\n")
    : "- 없음";
  const services = report.services.length
    ? report.services.map((item) => `- ${item.name} (${item.action})`).join("\n")
    : "- 없음";
  const ports = report.ports.length
    ? report.ports.map((item) => `- ${item.port}/${item.protocol}: ${item.purpose}`).join("\n")
    : "- 없음";
  const stopConditions = report.stop_conditions.length
    ? report.stop_conditions.map((item) => `- ${item}`).join("\n")
    : "- 없음";

  return [
    "설치 계획 요약",
    `도메인: ${report.domain}`,
    `모드: ${report.deployment_mode === "local-test" ? "로컬 테스트" : "실제 도메인"}`,
    `웹서버: ${runtimeLabel(report.web_server)}`,
    `PHP: ${report.php_version}`,
    `데이터베이스: ${databaseLabel(report.database)}`,
    `사이트 계정: ${report.site_user}`,
    `웹루트: ${report.web_root}`,
    "",
    "설치 예정 패키지:",
    packages,
    "",
    "생성/변경 예정 파일:",
    files,
    "",
    "서비스 계획:",
    services,
    "",
    "포트 계획:",
    ports,
    "",
    "중단 조건:",
    stopConditions,
  ].join("\n");
}

function bindEvents() {
  window.addEventListener("popstate", (event) => {
    showStep(event.state?.step || window.location.hash.replace("#", ""), { pushHistory: false });
  });

  nodes.themeToggle.addEventListener("click", () => {
    applyTheme(state.theme === "dark" ? "light" : "dark");
  });

  document.querySelectorAll("[data-step]").forEach((button) => {
    button.addEventListener("click", () => showStep(button.dataset.step));
  });

  document.querySelectorAll("[data-next]").forEach((button) => {
    button.addEventListener("click", () => showStep(button.dataset.next));
  });

  document.querySelectorAll('input[name="install_template"]').forEach((radio) => {
    radio.addEventListener("change", () => {
      if (radio.checked) {
        applyTemplate(radio.value);
        log("설치 템플릿 적용");
      }
    });
  });

  document.querySelector("#login-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const button = event.submitter;
    const username = document.querySelector("#login-username").value;
    const passwordInput = document.querySelector("#login-password");

    await withBusy(button, "확인 중", async () => {
      try {
        hideAlert(nodes.loginStatus);
        log(`서버 계정 인증 중: ${username}`);
        const response = await apiFetch("/api/auth/login", {
          method: "POST",
          body: JSON.stringify({
            username,
            password: passwordInput.value,
          }),
        });
        passwordInput.value = "";
        state.authenticated = response.authenticated;
        setAlert(nodes.loginStatus, "success", "로그인 성공", `${response.username} 계정으로 인증되었습니다.`);
        log(`서버 계정 인증 성공: ${response.username}`);
        showStep("check");
      } catch (error) {
        passwordInput.value = "";
        setAlert(nodes.loginStatus, "error", "로그인 실패", formatError(error));
        log(formatError(error));
      }
    });
  });

  document.querySelector("#doctor-button").addEventListener("click", async (event) => {
    await withBusy(event.currentTarget, "점검 중", async () => {
      try {
        await runDoctorCheck();
      } catch (error) {
        setAlert(nodes.doctorStatus, "error", "서버 점검 실패", formatError(error));
        log(formatError(error));
      }
    });
  });

  document.querySelector("#plan-button").addEventListener("click", async (event) => {
    await withBusy(event.currentTarget, "생성 중", async () => {
      try {
        hideAlert(nodes.planStatus);
        setPlanReady(false);
        log("설치 계획 생성");
        const report = await apiFetch("/api/plan", {
          method: "POST",
          body: JSON.stringify(optionPayload()),
        });
        nodes.planOutput.textContent = renderPlanReport(report);
        setAlert(nodes.planStatus, "success", "설치 계획 생성 완료", `${report.packages.length}개 패키지 묶음과 ${report.files.length}개 파일 변경 계획을 확인했습니다.`);
        setPlanReady(true);
        log(`설치 계획 준비 완료: packages=${report.packages.length}, files=${report.files.length}`);
      } catch (error) {
        nodes.planOutput.textContent = formatError(error);
        setAlert(nodes.planStatus, "error", "설치 계획 생성 실패", formatError(error));
        setPlanReady(false);
        log(formatError(error));
      }
    });
  });

  document.querySelector("#install-button").addEventListener("click", async (event) => {
    ["preflight", "packages", "config", "services", "ports", "http", "report"].forEach((stage) => {
      markStage(stage, "대기");
    });
    nodes.installProgress.value = 0;
    hideAlert(nodes.installStatus);

    await withBusy(event.currentTarget, "준비 중", async () => {
      try {
        markStage("preflight", "진행");
        setAlert(nodes.installStatus, "info", "설치 준비 진행 중", "서버 사전 점검과 설정 파일 생성을 진행합니다.");
        log("설치 준비 시작");
        const report = await apiFetch("/api/install/prepare", {
          method: "POST",
          body: JSON.stringify(optionPayload()),
        });
        renderInstallReport(report);
        setAlert(nodes.installStatus, "success", "설치 준비 완료", "결과 리포트에서 생성된 파일과 다음 작업을 확인하세요.");
        showStep("report");
        log(`설치 준비 완료: ${report.phase}`);
      } catch (error) {
        markStage("preflight", "실패");
        nodes.reportOutput.textContent = `${formatError(error)}\n\n해결 후 다시 서버 점검을 실행하세요. 테스트 흔적이면 reset을 사용하세요.`;
        setAlert(nodes.installStatus, "error", "설치 준비 실패", formatError(error));
        log(formatError(error));
      }
    });
  });

  document.querySelector("#report-button").addEventListener("click", async (event) => {
    await withBusy(event.currentTarget, "새로고침", async () => {
      try {
        hideAlert(nodes.reportStatus);
        const report = await apiFetch("/api/report");
        nodes.reportOutput.textContent = report.content;
        setAlert(
          nodes.reportStatus,
          report.exists ? "success" : "warning",
          report.exists ? "리포트 확인 완료" : "리포트 없음",
          report.exists ? "서버에 저장된 리포트를 불러왔습니다." : "아직 생성된 리포트가 없습니다.",
        );
        log(`리포트 불러오기: ${report.exists ? "있음" : "없음"}`);
      } catch (error) {
        nodes.reportOutput.textContent = formatError(error);
        setAlert(nodes.reportStatus, "error", "리포트 불러오기 실패", formatError(error));
        log(formatError(error));
      }
    });
  });

  document.querySelector("#reset-button").addEventListener("click", async (event) => {
    if (!window.confirm("설치기가 만든 파일을 리셋할까요?")) {
      return;
    }

    await withBusy(event.currentTarget, "리셋 중", async () => {
      try {
        hideAlert(nodes.reportStatus);
        log("리셋 실행");
        const report = await apiFetch("/api/reset", {
          method: "POST",
          body: JSON.stringify({ dry_run: false }),
        });
        renderResetReport(report);
        setAlert(nodes.reportStatus, "success", "리셋 완료", "설치 준비 흔적을 정리했습니다.");
        log("리셋 완료");
        await runDoctorCheck();
      } catch (error) {
        nodes.reportOutput.textContent = formatError(error);
        setAlert(nodes.reportStatus, "error", "리셋 실패", formatError(error));
        log(formatError(error));
      }
    });
  });

  nodes.optionsForm.addEventListener("input", refreshFormState);
  nodes.optionsForm.addEventListener("change", refreshFormState);
}

async function boot() {
  applyTheme(state.theme);
  bindEvents();
  refreshFormState();
  connectEvents();

  try {
    state.bootstrap = await loadBootstrap();
    state.csrfToken = state.bootstrap.csrf_token;
    state.authenticated = state.bootstrap.auth.authenticated;
    setConnectionStatus("연결됨", "badge-success");

    if (state.bootstrap.domain) {
      nodes.domain.value = state.bootstrap.domain;
    }
    nodes.mode.value = state.bootstrap.local_test ? "local-test" : "public";
    refreshFormState();
    showStep(normalizedStep(window.location.hash.replace("#", "") || state.activeStep), { pushHistory: false });
    writeStepHistory(state.activeStep, true);
    log("웹 컨트롤러 준비 완료");
    log(`인증 상태: ${state.bootstrap.auth.status}`);
    if (state.bootstrap.auth.client_ip) {
      log(`접속 IP 잠금: ${state.bootstrap.auth.client_ip}`);
    }
  } catch (error) {
    setConnectionStatus("오류", "badge-error");
    setAlert(nodes.loginStatus, "error", "접속 확인 실패", `${formatError(error)}\n터미널에 출력된 token URL로 다시 접속하세요.`);
    log(`${formatError(error)}\n터미널에 출력된 token URL로 다시 접속하세요.`);
  }
}

boot();
