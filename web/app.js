const state = {
  activeStep: "login",
  bootstrap: null,
};

const nodes = {
  status: document.querySelector("#connection-status"),
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
  summaryDomain: document.querySelector("#summary-domain"),
  summaryMode: document.querySelector("#summary-mode"),
  summaryRuntime: document.querySelector("#summary-runtime"),
  summaryData: document.querySelector("#summary-data"),
};

function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  nodes.log.textContent += `\n[${timestamp}] ${message}`;
  nodes.log.scrollTop = nodes.log.scrollHeight;
}

function setConnectionStatus(label, colorClass) {
  nodes.status.textContent = label;
  nodes.status.className = `font-medium ${colorClass}`;
}

function showStep(step) {
  state.activeStep = step;

  document.querySelectorAll("[data-view]").forEach((view) => {
    view.classList.toggle("is-visible", view.dataset.view === step);
  });

  document.querySelectorAll("[data-step]").forEach((button) => {
    button.classList.toggle("is-active", button.dataset.step === step);
  });

  document.querySelectorAll("[data-progress]").forEach((item) => {
    item.classList.toggle("is-active", item.dataset.progress === step);
  });
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

function refreshFormState() {
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
  nodes.summaryMode.textContent = payload.local_test ? "local-test" : "public";
  nodes.summaryRuntime.textContent = `${payload.web_server} / PHP ${payload.php_version}`;
  nodes.summaryData.textContent = `${payload.database} / redis ${payload.redis}`;
}

function renderDraftPlan() {
  const payload = optionPayload();
  nodes.planOutput.textContent = [
    "Plan request",
    `domain: ${payload.domain}`,
    `mode: ${payload.local_test ? "local-test" : "public"}`,
    `web_server: ${payload.web_server}`,
    `php_version: ${payload.php_version}`,
    `database: ${payload.database}`,
    `site_user: ${payload.site_user}`,
    `web_root_mode: ${payload.web_root_mode}`,
    `www_mode: ${payload.www_mode}`,
    `redis: ${payload.redis}`,
    `mail_mode: ${payload.mail_mode}`,
    `security_profile: ${payload.security_profile}`,
    `ssh_policy: ${payload.ssh_policy}`,
    "",
    "API 연결은 다음 배치에서 실제 plan 결과로 교체됩니다.",
  ].join("\n");
}

function renderDoctorPlaceholder() {
  nodes.doctorResults.innerHTML = "";
  ["ubuntu-version", "privilege", "port-80", "port-443", "installer-state"].forEach((name) => {
    const item = document.createElement("div");
    item.className = "result-row";
    item.innerHTML = `<span>${name}</span><strong>대기</strong>`;
    nodes.doctorResults.append(item);
  });
}

function markStage(stage, status) {
  const row = document.querySelector(`[data-stage="${stage}"]`);
  if (!row) {
    return;
  }

  row.dataset.status = status;
  row.querySelector("strong").textContent = status;
}

async function loadBootstrap() {
  const response = await fetch("/api/bootstrap");
  if (!response.ok) {
    throw new Error(`bootstrap failed: ${response.status}`);
  }

  return response.json();
}

function bindEvents() {
  document.querySelectorAll("[data-step]").forEach((button) => {
    button.addEventListener("click", () => showStep(button.dataset.step));
  });

  document.querySelectorAll("[data-next]").forEach((button) => {
    button.addEventListener("click", () => showStep(button.dataset.next));
  });

  document.querySelector("#login-form").addEventListener("submit", (event) => {
    event.preventDefault();
    log("server account login UI submitted; auth API is pending");
    showStep("check");
  });

  document.querySelector("#doctor-button").addEventListener("click", () => {
    renderDoctorPlaceholder();
    log("doctor UI requested; API connection is pending");
  });

  document.querySelector("#plan-button").addEventListener("click", () => {
    renderDraftPlan();
    log("draft plan rendered from current options");
  });

  document.querySelector("#install-button").addEventListener("click", () => {
    ["preflight", "packages", "config", "services", "ports", "http", "report"].forEach((stage) => {
      markStage(stage, "대기");
    });
    markStage("preflight", "준비");
    log("install start UI requested; install API is pending");
  });

  document.querySelector("#report-button").addEventListener("click", () => {
    nodes.reportOutput.textContent = "리포트 API 연결 대기 중입니다.";
    log("report refresh requested");
  });

  document.querySelector("#reset-button").addEventListener("click", () => {
    log("reset requested; reset API is pending");
  });

  nodes.optionsForm.addEventListener("input", refreshFormState);
  nodes.optionsForm.addEventListener("change", refreshFormState);
}

async function boot() {
  bindEvents();
  refreshFormState();

  try {
    state.bootstrap = await loadBootstrap();
    setConnectionStatus("connected", "text-emerald-300");

    if (state.bootstrap.domain) {
      nodes.domain.value = state.bootstrap.domain;
    }
    nodes.mode.value = state.bootstrap.local_test ? "local-test" : "public";
    refreshFormState();
    log("web controller bootstrap loaded");
  } catch (error) {
    setConnectionStatus("error", "text-red-300");
    log(error.message);
  }
}

boot();
