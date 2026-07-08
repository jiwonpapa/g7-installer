const state = {
  activeStep: "check",
  bootstrap: null,
  socket: null,
  csrfToken: null,
  authenticated: false,
  doctorPassed: false,
  planReady: false,
  reportReady: false,
  installRunning: false,
  installCompleted: false,
  doctorReport: null,
  planReport: null,
  savedReportPayload: null,
  recoveryStatus: null,
  currentOperation: null,
  operationLocked: false,
  planPackages: [],
  packageTicker: null,
  theme: localStorage.getItem("g7inst-theme") || "light",
};

const nodes = {
  status: document.querySelector("#connection-status"),
  themeToggle: document.querySelector("#theme-toggle"),
  log: document.querySelector("#live-log"),
  domain: document.querySelector("#domain-input"),
  customWebRoot: document.querySelector("#custom-web-root"),
  webRootMode: document.querySelector("#web-root-mode"),
  sitePassword: document.querySelector("#site-password"),
  sitePasswordConfirm: document.querySelector("#site-password-confirm"),
  sitePasswordStatus: document.querySelector("#site-password-status"),
  optionsPlanButtons: document.querySelectorAll('[data-view="options"] [data-next="plan"]'),
  mailMode: document.querySelector("#mail-mode"),
  databaseVersion: document.querySelector("#database-version"),
  smtpHost: document.querySelector("#smtp-host"),
  smtpPort: document.querySelector("#smtp-port"),
  smtpFrom: document.querySelector("#smtp-from"),
  smtpEncryption: document.querySelector("#smtp-encryption"),
  securityGuidance: document.querySelector("#security-guidance"),
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
  reportProgress: document.querySelector("#report-progress"),
  packageProgressList: document.querySelector("#package-progress-list"),
  checkNextButton: document.querySelector("#check-next-button"),
  confirmSpecButton: document.querySelector("#confirm-spec-button"),
  installButton: document.querySelector("#install-button"),
  installResultButton: document.querySelector("#install-result-button"),
  installConfirmDialog: document.querySelector("#install-confirm-dialog"),
  installConfirmSummary: document.querySelector("#install-confirm-summary"),
  installConfirmStart: document.querySelector("#install-confirm-start"),
  recoveryConfirmDialog: document.querySelector("#recovery-confirm-dialog"),
  recoveryConfirmTitle: document.querySelector("#recovery-confirm-title"),
  recoveryConfirmMessage: document.querySelector("#recovery-confirm-message"),
  recoveryConfirmSummary: document.querySelector("#recovery-confirm-summary"),
  recoveryConfirmYes: document.querySelector("#recovery-confirm-yes"),
  summaryPanel: document.querySelector("#summary-panel"),
  floatingHelp: document.querySelector("#floating-help"),
  summaryDomain: document.querySelector("#summary-domain"),
  summaryRuntime: document.querySelector("#summary-runtime"),
  summaryData: document.querySelector("#summary-data"),
  summaryApp: document.querySelector("#summary-app"),
};

const stepOrder = ["login", "check", "options", "plan", "install", "report"];
const wizardStorageKey = "g7inst-wizard-state-v1";

const statusLabel = {
  pass: "통과",
  warn: "주의",
  fail: "실패",
  pending: "대기",
  info: "정보",
  installed: "기존 보존",
  "not-installed": "신규 설치",
  unknown: "확인 필요",
  skipped: "건너뜀",
  deferred: "후속 단계",
  planned: "계획됨",
  manual: "수동 확인",
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
  "setup token session is required": "접속 확인 주소 세션이 필요합니다.",
  "missing setup session cookie": "설치 세션 쿠키가 없습니다.",
  "setup session expired or invalid": "설치 세션이 만료되었거나 올바르지 않습니다.",
  "missing CSRF token": "보안 확인 토큰이 없습니다.",
  "invalid CSRF token": "보안 확인 토큰이 올바르지 않습니다.",
  "install is already running": "설치 작업이 이미 진행 중입니다.",
  "reset is blocked while install is running": "설치 중에는 리셋할 수 없습니다.",
  "rollback is blocked while another install action is running": "다른 설치 작업 중에는 되돌릴 수 없습니다.",
};

const templates = {
  recommended: {
    domain: null,
    deployment_mode: "public",
    web_server: "nginx",
    php_version: "8.3",
    database: "mysql",
    database_version: "apt-default",
    redis: "enable",
    mail_mode: "none",
    app_package: "gnuboard7",
    web_root_mode: "public-html",
    www_mode: "redirect-to-root",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
  apache: {
    domain: null,
    deployment_mode: "public",
    web_server: "apache",
    php_version: "8.3",
    database: "mysql",
    database_version: "apt-default",
    redis: "enable",
    mail_mode: "none",
    app_package: "gnuboard7",
    web_root_mode: "public-html",
    www_mode: "redirect-to-root",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
};

// Icon paths are sourced from lucide-static and rendered inline to avoid extra requests.
const iconSvg = {
  "check": "<path d=\"M20 6 9 17l-5-5\" />",
  "chevron-left": "<path d=\"m15 18-6-6 6-6\" />",
  "chevron-right": "<path d=\"m9 18 6-6-6-6\" />",
  "clipboard-list": "<rect width=\"8\" height=\"4\" x=\"8\" y=\"2\" rx=\"1\" ry=\"1\" /> <path d=\"M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2\" /> <path d=\"M12 11h4\" /> <path d=\"M12 16h4\" /> <path d=\"M8 11h.01\" /> <path d=\"M8 16h.01\" />",
  "file-check": "<path d=\"M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z\" /> <path d=\"M14 2v5a1 1 0 0 0 1 1h5\" /> <path d=\"m9 15 2 2 4-4\" />",
  "home": "<path d=\"M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8\" /> <path d=\"M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z\" />",
  "log-in": "<path d=\"m10 17 5-5-5-5\" /> <path d=\"M15 12H3\" /> <path d=\"M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4\" />",
  "moon": "<path d=\"M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401\" />",
  "package-plus": "<path d=\"M12 22V12\" /> <path d=\"M16 17h6\" /> <path d=\"M19 14v6\" /> <path d=\"M21 10.535V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.729l7 4a2 2 0 0 0 2 .001l1.675-.955\" /> <path d=\"M3.29 7 12 12l8.71-5\" /> <path d=\"m7.5 4.27 8.997 5.148\" />",
  "play": "<path d=\"M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z\" />",
  "power": "<path d=\"M12 2v10\" /> <path d=\"M18.4 6.6a9 9 0 1 1-12.77.04\" />",
  "refresh-cw": "<path d=\"M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8\" /> <path d=\"M21 3v5h-5\" /> <path d=\"M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16\" /> <path d=\"M8 16H3v5\" />",
  "rotate-ccw": "<path d=\"M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8\" /> <path d=\"M3 3v5h5\" />",
  "scan-line": "<path d=\"M3 7V5a2 2 0 0 1 2-2h2\" /> <path d=\"M17 3h2a2 2 0 0 1 2 2v2\" /> <path d=\"M21 17v2a2 2 0 0 1-2 2h-2\" /> <path d=\"M7 21H5a2 2 0 0 1-2-2v-2\" /> <path d=\"M7 12h10\" />",
  "shield-check": "<path d=\"M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z\" /> <path d=\"m9 12 2 2 4-4\" />",
  "sliders-horizontal": "<path d=\"M10 5H3\" /> <path d=\"M12 19H3\" /> <path d=\"M14 3v4\" /> <path d=\"M16 17v4\" /> <path d=\"M21 12h-9\" /> <path d=\"M21 19h-5\" /> <path d=\"M21 5h-7\" /> <path d=\"M8 10v4\" /> <path d=\"M8 12H3\" />",
  "sun": "<circle cx=\"12\" cy=\"12\" r=\"4\" /> <path d=\"M12 2v2\" /> <path d=\"M12 20v2\" /> <path d=\"m4.93 4.93 1.41 1.41\" /> <path d=\"m17.66 17.66 1.41 1.41\" /> <path d=\"M2 12h2\" /> <path d=\"M20 12h2\" /> <path d=\"m6.34 17.66-1.41 1.41\" /> <path d=\"m19.07 4.93-1.41 1.41\" />",
  "undo-2": "<path d=\"M9 14 4 9l5-5\" /> <path d=\"M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11\" />",
  "x": "<path d=\"M18 6 6 18\" /> <path d=\"m6 6 12 12\" />",
};

function iconMarkup(name) {
  const svg = iconSvg[name];
  if (!svg) {
    return "";
  }

  return `<svg class="icon" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${svg}</svg>`;
}

function buttonLabel(button) {
  return button?.dataset?.label || button?.textContent?.trim() || "";
}

function hydrateIconButton(button) {
  if (!button) {
    return;
  }

  const label = buttonLabel(button);
  button.dataset.label = label;
  const icon = iconMarkup(button.dataset.icon);
  button.innerHTML = `${icon}<span class="btn-label">${escapeHtml(label)}</span>`;
}

function setButtonLabel(button, label) {
  if (!button) {
    return;
  }

  button.dataset.label = label;
  hydrateIconButton(button);
}

function hydrateIconLabel(label) {
  if (!label) {
    return;
  }

  const text = label.dataset.label || label.textContent.trim();
  label.dataset.label = text;
  label.innerHTML = `${iconMarkup(label.dataset.uiIcon)}<span>${escapeHtml(text)}</span>`;
}

function hydrateIcons(root = document) {
  root.querySelectorAll("[data-icon]").forEach((button) => hydrateIconButton(button));
  root.querySelectorAll("[data-ui-icon]").forEach((label) => hydrateIconLabel(label));
}

function setOperationLocked(locked) {
  state.operationLocked = locked;
  document.body.dataset.operationLocked = locked ? "true" : "false";

  document.querySelectorAll("button, input, select, textarea").forEach((control) => {
    if (locked) {
      if (!Object.prototype.hasOwnProperty.call(control.dataset, "lockPrevDisabled")) {
        control.dataset.lockPrevDisabled = control.disabled ? "true" : "false";
      }
      control.disabled = true;
      return;
    }

    if (Object.prototype.hasOwnProperty.call(control.dataset, "lockPrevDisabled")) {
      control.disabled = control.dataset.lockPrevDisabled === "true";
      delete control.dataset.lockPrevDisabled;
    }
  });
}

async function withBusy(button, busyText, task) {
  if (!button) {
    return task();
  }

  const originalText = buttonLabel(button);
  button.disabled = true;
  if (busyText) {
    setButtonLabel(button, busyText);
  }

  try {
    return await task();
  } finally {
    button.disabled = false;
    setButtonLabel(button, originalText);
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
  if (errorLabel[message]) {
    return errorLabel[message];
  }

  if (message.startsWith("package is not available from current apt sources:")) {
    return message.replace(
      "package is not available from current apt sources:",
      "현재 apt 소스에서 찾을 수 없는 패키지:",
    );
  }

  if (message.startsWith("install command failed during")) {
    return message
      .replace("install command failed during", "설치 명령 실패:")
      .replace("exited with status", "종료 코드");
  }

  if (message.startsWith("install verification failed:")) {
    return message.replace("install verification failed:", "설치 검증 실패:");
  }

  if (message.startsWith("rollback blocked:")) {
    return message.replace("rollback blocked:", "되돌리기 중단:");
  }

  if (message.startsWith("rollback command failed during")) {
    return message
      .replace("rollback command failed during", "되돌리기 명령 실패:")
      .replace("exited with status", "종료 코드");
  }

  if (message.startsWith("rollback verification failed:")) {
    return message.replace("rollback verification failed:", "되돌리기 검증 실패:");
  }

  return message;
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

function formValues() {
  const values = {};
  const form = new FormData(nodes.optionsForm);
  form.forEach((value, key) => {
    if (key === "site_password" || key === "site_password_confirm") {
      return;
    }
    values[key] = value;
  });
  return values;
}

function applyFormValues(values) {
  if (!values || typeof values !== "object") {
    return;
  }

  Object.entries(values).forEach(([name, value]) => {
    setFormValue(name, value);
  });
}

function readWizardState() {
  try {
    const raw = sessionStorage.getItem(wizardStorageKey);
    return raw ? JSON.parse(raw) : null;
  } catch (_error) {
    return null;
  }
}

function saveWizardState() {
  try {
    sessionStorage.setItem(wizardStorageKey, JSON.stringify({
      activeStep: state.activeStep,
      form: formValues(),
      doctorReport: state.doctorReport,
      planReport: state.planReport,
      savedReportPayload: state.savedReportPayload,
      recoveryStatus: state.recoveryStatus,
      flags: {
        doctorPassed: state.doctorPassed,
        planReady: state.planReady,
        reportReady: state.reportReady,
        installCompleted: state.installCompleted,
      },
    }));
  } catch (_error) {
    log("브라우저 세션 상태 저장 실패");
  }
}

function clearWizardState() {
  try {
    sessionStorage.removeItem(wizardStorageKey);
  } catch (_error) {
    log("브라우저 세션 상태 삭제 실패");
  }
}

function setPlanReady(ready) {
  state.planReady = ready;
  refreshConfirmSpecButton();
}

function refreshConfirmSpecButton() {
  if (nodes.confirmSpecButton) {
    nodes.confirmSpecButton.disabled = !state.planReady || Boolean(sitePasswordError());
  }
}

function setReportReady(ready) {
  state.reportReady = ready;
  if (nodes.installResultButton) {
    nodes.installResultButton.disabled = !ready;
    setButtonLabel(nodes.installResultButton, ready ? "결과 보기" : "기본 구성 후 결과 보기");
  }
}

function refreshInstallButtonState(label = null) {
  if (!nodes.installButton) {
    return;
  }

  if (state.installRunning) {
    nodes.installButton.disabled = true;
    setButtonLabel(nodes.installButton, "설치 중");
    return;
  }

  if (state.installCompleted) {
    nodes.installButton.disabled = true;
    setButtonLabel(nodes.installButton, "기본 구성 완료");
    return;
  }

  nodes.installButton.disabled = false;
  setButtonLabel(nodes.installButton, label || "기본 구성 시작");
}

function setDoctorPassed(passed) {
  state.doctorPassed = passed;
  if (nodes.checkNextButton) {
    nodes.checkNextButton.disabled = !passed;
    setButtonLabel(nodes.checkNextButton, passed ? "다음: 설치 방식" : "점검 통과 후 다음");
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
  nodes.themeToggle.dataset.icon = theme === "dark" ? "sun" : "moon";
  nodes.themeToggle.title = theme === "dark" ? "라이트 모드" : "다크 모드";
  setButtonLabel(nodes.themeToggle, theme === "dark" ? "라이트 모드" : "다크 모드");
}

function showStep(nextStep, options = {}) {
  if (state.operationLocked && !options.force) {
    log("진행 중인 서버 작업이 끝난 뒤 이동할 수 있습니다.");
    return;
  }

  let step = normalizedStep(nextStep);
  const shouldPushHistory = options.pushHistory !== false;
  const recoveryMode = Boolean(
    state.reportReady
      || state.installCompleted
      || state.recoveryStatus?.can_reset
      || state.recoveryStatus?.can_rollback
      || state.recoveryStatus?.metadata_paths?.length,
  );

  if (!["login", "check"].includes(step) && !state.authenticated) {
    setAlert(
      nodes.loginStatus,
      "warning",
      "접속 확인이 필요합니다",
      "터미널에 출력된 접속 확인 주소로 다시 접속하세요. 서버 비밀번호 입력은 사용하지 않습니다.",
    );
    step = "login";
  }

  if (["options", "plan"].includes(step) && !state.doctorPassed) {
    setAlert(
      nodes.doctorStatus,
      "warning",
      "서버 점검이 먼저 필요합니다",
      "신규 서버 상태를 통과해야 설치 방식 선택으로 넘어갈 수 있습니다.",
    );
    step = "check";
  }

  if (step === "plan") {
    const passwordError = refreshSitePasswordState({ show: true });
    if (passwordError) {
      log(passwordError);
      step = "options";
    }
  }

  if (step === "install" && !state.doctorPassed && !recoveryMode) {
    setAlert(
      nodes.doctorStatus,
      "warning",
      "서버 점검이 먼저 필요합니다",
      "신규 서버 상태를 통과하거나 설치기 복구 상태가 확인되어야 합니다.",
    );
    step = "check";
  }

  if (step === "install" && !recoveryMode) {
    const passwordError = refreshSitePasswordState({ show: true });
    if (passwordError) {
      setAlert(
        nodes.planStatus,
        "warning",
        "사이트 계정 비밀번호 확인 필요",
        passwordError,
      );
      log(passwordError);
      step = "options";
    }
  }

  if (step === "install" && !state.planReady && !recoveryMode) {
    setAlert(
      nodes.planStatus,
      "warning",
      "설치 사양 확정이 필요합니다",
      "계획 생성을 완료한 뒤 이 사양으로 진행 버튼을 누르세요.",
    );
    step = "plan";
  }

  if (step === "report" && !state.reportReady) {
    setAlert(
      nodes.installStatus,
      "warning",
      "설치 결과가 아직 없습니다",
      "기본 서버 구성을 완료해야 결과 리포트를 볼 수 있습니다.",
    );
    step = "install";
  }

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

  refreshSitePasswordState();
  saveWizardState();
}

function optionPayload() {
  const form = new FormData(nodes.optionsForm);
  const mailMode = form.get("mail_mode");
  const customWebRoot = form.get("web_root")?.trim();

  return {
    domain: form.get("domain")?.trim() || "example.com",
    web_server: form.get("web_server"),
    php_version: form.get("php_version"),
    php_source: phpSourceForVersion(form.get("php_version")),
    database: form.get("database"),
    database_version: form.get("database_version"),
    app_package: form.get("app_package"),
    site_user: form.get("site_user")?.trim() || "g7",
    site_password: form.get("site_password") || "",
    site_password_confirm: form.get("site_password_confirm") || "",
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
    dns_check: true,
  };
}

function validateSitePassword(payload) {
  return sitePasswordError(payload);
}

const sitePasswordAlertMessages = [
  "사이트 계정 비밀번호를 입력하세요.",
  "사이트 계정 비밀번호 확인이 일치하지 않습니다.",
  "사이트 계정 비밀번호는 8자 이상이어야 합니다.",
  "사이트 계정 비밀번호에는 콜론, 줄바꿈, 제어문자를 사용할 수 없습니다.",
  "사이트 계정 비밀번호에 사용할 수 없는 문자가 있습니다.",
];

function sitePasswordError(payload = optionPayload()) {
  if (!payload.site_password) {
    return "사이트 계정 비밀번호를 입력하세요.";
  }
  if (payload.site_password !== payload.site_password_confirm) {
    return "사이트 계정 비밀번호 확인이 일치하지 않습니다.";
  }
  if (payload.site_password.length < 8) {
    return "사이트 계정 비밀번호는 8자 이상이어야 합니다.";
  }
  if (/[:\n\r\x00-\x1F\x7F]/.test(payload.site_password)) {
    return "사이트 계정 비밀번호에는 콜론, 줄바꿈, 제어문자를 사용할 수 없습니다.";
  }
  return null;
}

function clearSitePasswordAlerts() {
  [nodes.planStatus, nodes.installStatus].forEach((node) => {
    const text = node?.textContent || "";
    if (sitePasswordAlertMessages.some((message) => text.includes(message))) {
      hideAlert(node);
    }
  });
}

function refreshSitePasswordState(options = {}) {
  const payload = optionPayload();
  const error = sitePasswordError(payload);
  const hasInput = Boolean(payload.site_password || payload.site_password_confirm);
  const shouldShowStatus = Boolean(options.show || hasInput || state.activeStep === "options");

  if (nodes.sitePassword) {
    nodes.sitePassword.setCustomValidity(error || "");
    nodes.sitePassword.classList.toggle("input-error", Boolean(error && shouldShowStatus));
  }
  if (nodes.sitePasswordConfirm) {
    nodes.sitePasswordConfirm.setCustomValidity(error || "");
    nodes.sitePasswordConfirm.classList.toggle("input-error", Boolean(error && shouldShowStatus));
  }
  if (nodes.sitePasswordStatus) {
    nodes.sitePasswordStatus.hidden = !shouldShowStatus;
    nodes.sitePasswordStatus.dataset.status = error ? "fail" : "pass";
    nodes.sitePasswordStatus.textContent = error || "사이트 계정 비밀번호가 일치합니다.";
  }
  nodes.optionsPlanButtons.forEach((button) => {
    button.disabled = Boolean(error);
    button.title = error || "";
    button.setAttribute("aria-disabled", error ? "true" : "false");
  });
  refreshConfirmSpecButton();
  if (!error) {
    clearSitePasswordAlerts();
  }

  return error;
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

function refreshFormState(options = {}) {
  const preservePlan = Boolean(options.preservePlan);
  const shouldPersist = options.persist !== false;

  if (!preservePlan) {
    state.planReport = null;
    state.savedReportPayload = null;
    setPlanReady(false);
  }

  if (!preservePlan && !state.installRunning && !state.installCompleted) {
    setReportReady(false);
    renderPackageProgress([]);
    refreshInstallButtonState();
  }
  const webRootIsCustom = nodes.webRootMode.value === "custom";
  nodes.customWebRoot.disabled = !webRootIsCustom;
  if (!webRootIsCustom) {
    nodes.customWebRoot.value = "";
  }

  const smtpEnabled = nodes.mailMode.value === "smtp-relay";
  [nodes.smtpHost, nodes.smtpPort, nodes.smtpFrom, nodes.smtpEncryption].forEach((node) => {
    node.disabled = !smtpEnabled;
  });

  const database = nodes.optionsForm.elements.database.value;
  if (database === "mariadb") {
    nodes.databaseVersion.value = "apt-default";
    nodes.databaseVersion.disabled = true;
  } else {
    nodes.databaseVersion.disabled = false;
  }

  refreshSecurityGuidance();
  refreshSitePasswordState();
  refreshSummary();
  if (shouldPersist) {
    saveWizardState();
  }
}

function refreshSummary() {
  if (nodes.summaryPanel) {
    nodes.summaryPanel.hidden = !state.authenticated;
  }

  const payload = optionPayload();
  nodes.summaryDomain.textContent = payload.domain;
  nodes.summaryRuntime.textContent = `${runtimeLabel(payload.web_server)} / ${phpRuntimeLabel(payload.php_version, payload.php_source)}`;
  nodes.summaryData.textContent = `${databaseLabel(payload.database)} / Redis ${payload.redis === "enable" ? "사용" : "미사용"}`;
  nodes.summaryApp.textContent = appPackageLabel(payload.app_package);
}

function renderDraftPlan() {
  const payload = optionPayload();
  nodes.planOutput.textContent = [
    "설치 계획 요청값",
    `도메인: ${payload.domain}`,
    `웹서버: ${runtimeLabel(payload.web_server)}`,
    `PHP: ${phpRuntimeLabel(payload.php_version, payload.php_source)}`,
    `데이터베이스: ${databaseLabel(payload.database)} (${databaseVersionLabel(payload.database_version)})`,
    `설치할 앱: ${appPackageLabel(payload.app_package)}`,
    `사이트 계정: ${payload.site_user}`,
    `웹루트 방식: ${payload.web_root_mode}`,
    `www 처리: ${payload.www_mode}`,
    `Redis: ${payload.redis === "enable" ? "사용" : "미사용"}`,
    `메일: ${mailModeLabel(payload.mail_mode)}`,
    `보안 수준: ${payload.security_profile}`,
    `SSH 정책: ${payload.ssh_policy}`,
    "",
    "계획 생성 버튼을 누르면 실제 plan 결과로 교체됩니다.",
  ].join("\n");
}

function runtimeLabel(value) {
  return value === "apache" ? "Apache" : "Nginx";
}

function phpSourceForVersion(version) {
  return version === "8.3" ? "ubuntu" : "ondrej";
}

function phpSourceLabel(value) {
  return value === "ondrej" ? "Ondrej PPA" : "Ubuntu 기본 apt";
}

function phpRuntimeLabel(version, source = phpSourceForVersion(version)) {
  return `PHP ${version || "-"} (${phpSourceLabel(source)})`;
}

function databaseLabel(value) {
  return value === "mariadb" ? "MariaDB" : "MySQL";
}

function databaseVersionLabel(value) {
  const labels = {
    "apt-default": "Ubuntu apt 기본값",
    "mysql-8.0": "MySQL 8.0 계열",
    "mysql-8.4": "MySQL 8.4 LTS 계열",
  };
  return labels[value] || value || "Ubuntu apt 기본값";
}

function appPackageLabel(value) {
  const labels = {
    gnuboard7: "그누보드7",
    wordpress: "WordPress",
    laravel: "Laravel",
  };
  return labels[value] || value || "그누보드7";
}

function mailModeLabel(value) {
  const labels = {
    none: "메일 발송 안 함",
    "smtp-relay": "외부 SMTP로 발송",
    "local-postfix": "서버 Postfix로 발송",
  };
  return labels[value] || value || "메일 발송 안 함";
}

function refreshSecurityGuidance() {
  if (!nodes.securityGuidance) {
    return;
  }

  const payload = optionPayload();
  const notes = [
    "Redis는 로컬 접속만 허용해야 합니다.",
    `${databaseLabel(payload.database)}은 외부 포트를 열지 않고 앱 계정만 별도로 생성해야 합니다.`,
    payload.ssh_policy === "harden"
      ? "SSH 강화는 접속 정책을 바꾸므로 현재 접속 세션을 유지한 상태에서 적용해야 합니다."
      : "SSH는 기본적으로 변경하지 않고 위험 항목만 리포트합니다.",
    payload.mail_mode === "smtp-relay"
      ? "메일은 수신 서버가 아니라 외부 SMTP 발송 전용으로 설정합니다."
      : "메일 수신 서버 구성은 기본 설치 범위에서 제외합니다.",
  ];

  nodes.securityGuidance.innerHTML = `
    <h3>보안 설정 안내</h3>
    <ul>${notes.map((note) => `<li>${escapeHtml(note)}</li>`).join("")}</ul>
  `;
}

function recoveryPanels() {
  return Array.from(document.querySelectorAll("[data-recovery-panel]"));
}

function recoveryActionButtons(action) {
  return Array.from(document.querySelectorAll(`[data-recovery-action="${action}"]`));
}

function renderRecoveryStatus(status) {
  state.recoveryStatus = status || null;
  const hasMetadata = Boolean(status?.metadata_paths?.length);
  const doctorBlocked = Boolean(state.doctorReport && !state.doctorReport.install_allowed);
  const shouldShowPanel = Boolean(state.authenticated && (hasMetadata || doctorBlocked));

  recoveryPanels().forEach((panel) => {
    panel.hidden = !shouldShowPanel;
    const message = panel.querySelector("[data-recovery-message]");
    const paths = panel.querySelector("[data-recovery-paths]");

    if (message) {
      message.textContent = status?.message || "설치기 소유 흔적을 확인하지 못했습니다.";
    }
    if (paths) {
      const rows = status?.metadata_paths?.length
        ? status.metadata_paths.map((path) => `<li>${escapeHtml(path)}</li>`).join("")
        : `<li>설치기 소유 기록 없음</li>`;
      paths.innerHTML = rows;
    }
  });

  recoveryActionButtons("rollback").forEach((button) => {
    button.disabled = !status?.can_rollback;
    button.title = status?.can_rollback
      ? "설치 직후 패키지와 설치 기록을 되돌립니다."
      : (status?.rollback_reason || "안전 조건을 만족하지 않아 되돌릴 수 없습니다.");
  });

  recoveryActionButtons("reset").forEach((button) => {
    button.disabled = !status?.can_reset;
    button.title = status?.can_reset
      ? "설치기 소유 설정과 웹루트 기록을 정리합니다. apt 패키지는 제거하지 않습니다."
      : (status?.can_rollback ? "패키지 설치 직후에는 패키지 되돌리기를 먼저 사용하세요." : "설치기 소유 기록이 없어 정리할 수 없습니다.");
  });

  saveWizardState();
}

async function refreshRecoveryStatus() {
  if (!state.authenticated) {
    renderRecoveryStatus(null);
    return null;
  }

  try {
    const status = await apiFetch("/api/recovery");
    renderRecoveryStatus(status);
    return status;
  } catch (error) {
    renderRecoveryStatus(null);
    log(formatError(error));
    return null;
  }
}

function renderDoctor(report) {
  state.doctorReport = report;
  nodes.doctorResults.innerHTML = "";
  setDoctorPassed(Boolean(report.install_allowed));

  setAlert(
    nodes.doctorStatus,
    report.install_allowed ? "success" : "error",
    report.install_allowed ? "서버 점검 통과" : "서버 점검 실패",
    report.install_allowed
      ? "서버 설치를 계속 진행할 수 있습니다."
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
  renderRecoveryStatus(state.recoveryStatus);
  saveWizardState();
}

async function runDoctorCheck() {
  hideAlert(nodes.doctorStatus);
  log("서버 점검 실행");
  const report = await apiFetch("/api/doctor");
  renderDoctor(report);
  await refreshRecoveryStatus();
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

function flattenPlanPackages(packages) {
  if (!Array.isArray(packages)) {
    return [];
  }

  return packages.flatMap((packageGroup) => String(packageGroup.name || "")
    .split(/\s+/)
    .filter(Boolean)
    .map((name) => ({
      name,
      description: packageGroup.description || "설치 예정 패키지",
    })));
}

function renderPackageProgress(packages) {
  state.planPackages = packages;
  stopPackageTicker();

  if (!nodes.packageProgressList) {
    return;
  }

  if (!packages.length) {
    nodes.packageProgressList.innerHTML = `<div class="empty-state">설치 사양 확정 후 패키지 목록이 표시됩니다.</div>`;
    return;
  }

  nodes.packageProgressList.innerHTML = packages.map((packageItem) => `
    <div class="package-progress-row" data-package="${escapeHtml(packageItem.name)}">
      <div>
        <span>${escapeHtml(packageItem.name)}</span>
        <p>${escapeHtml(packageItem.description)}</p>
      </div>
      <div class="package-progress-meter">
        <progress class="progress progress-primary w-full" value="0" max="100"></progress>
        <strong>대기 0%</strong>
      </div>
    </div>
  `).join("");
}

function updatePackageProgress(name, status, percent, message = null) {
  const row = document.querySelector(`[data-package="${CSS.escape(name)}"]`);
  if (!row) {
    return;
  }

  const progress = row.querySelector("progress");
  const label = row.querySelector("strong");
  const description = row.querySelector("p");
  const value = Math.max(0, Math.min(100, Number(percent) || 0));
  progress.value = value;
  label.textContent = `${status} ${value}%`;
  if (message) {
    description.textContent = message;
  }
}

function resetPackageProgressRows() {
  state.planPackages.forEach((packageItem) => {
    updatePackageProgress(packageItem.name, "대기", 0, packageItem.description);
  });
}

function startPackageTicker() {
  stopPackageTicker();
  if (!state.planPackages.length) {
    return;
  }

  let index = 0;
  let percent = 0;
  updatePackageProgress(state.planPackages[index].name, "설치 중", 5, "apt 작업을 준비하고 있습니다.");

  state.packageTicker = window.setInterval(() => {
    if (!state.installRunning) {
      stopPackageTicker();
      return;
    }

    const packageItem = state.planPackages[index];
    percent = Math.min(95, percent + 10);
    updatePackageProgress(packageItem.name, "설치 중", percent, "apt 설치 또는 검증을 진행 중입니다.");

    if (percent >= 95 && index < state.planPackages.length - 1) {
      updatePackageProgress(
        packageItem.name,
        "검증 대기",
        100,
        "apt 설치 요청을 넘겼고 최종 검증 결과를 기다리고 있습니다.",
      );
      index += 1;
      percent = 5;
      updatePackageProgress(state.planPackages[index].name, "설치 중", percent, "apt 작업을 준비하고 있습니다.");
    }
  }, 700);
}

function stopPackageTicker() {
  if (state.packageTicker) {
    window.clearInterval(state.packageTicker);
    state.packageTicker = null;
  }
}

function applyPackageChecks(checks) {
  stopPackageTicker();
  if (!Array.isArray(checks)) {
    return;
  }

  checks.forEach((check) => {
    updatePackageProgress(
      check.name,
      check.status === "pass" ? "설치됨" : "실패",
      100,
      check.message || "",
    );
  });
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

function setProgress(node, percent) {
  if (!node) {
    return;
  }
  node.value = Math.max(0, Math.min(100, Number(percent) || 0));
}

function showReportProgress(percent = 0) {
  if (!nodes.reportProgress) {
    return;
  }
  nodes.reportProgress.classList.remove("hidden");
  setProgress(nodes.reportProgress, percent);
}

function hideReportProgress() {
  if (!nodes.reportProgress) {
    return;
  }
  nodes.reportProgress.classList.add("hidden");
  setProgress(nodes.reportProgress, 0);
}

function handleProgressEvent(payload) {
  const percent = Number(payload.percent ?? 0);
  if (payload.operation === "rollback" || payload.operation === "reset" || state.currentOperation === "rollback" || state.currentOperation === "reset") {
    showReportProgress(percent);
  } else {
    setProgress(nodes.installProgress, percent);
  }

  if (payload.message) {
    log(payload.message);
  }
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
    if (payload.event_type === "progress") {
      handleProgressEvent(payload);
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

function parseSavedReport(payload) {
  if (!payload?.exists) {
    return null;
  }

  try {
    return JSON.parse(payload.content);
  } catch (_error) {
    return null;
  }
}

function packageItemsFromChecks(checks) {
  if (!Array.isArray(checks)) {
    return [];
  }

  return checks.map((check) => ({
    name: check.name,
    description: check.message || "저장된 설치 리포트",
  }));
}

function applyReportOptions(report) {
  if (!report || typeof report !== "object") {
    return;
  }

  applyFormValues({
    domain: report.domain,
    deployment_mode: report.deployment_mode,
    app_package: report.app_package || report.app_profile,
    web_server: report.web_server,
    php_version: report.php_version,
    database: report.database,
    site_user: report.site_user,
    web_root: report.web_root,
    security_profile: report.security_profile,
    ssh_policy: report.ssh_policy,
  });
  refreshFormState({ preservePlan: true, persist: false });
}

function restoreInstallStateFromReport(report) {
  if (!report || typeof report !== "object") {
    return;
  }

  applyReportOptions(report);
  const packageChecks = Array.isArray(report.package_checks) ? report.package_checks : [];
  if (packageChecks.length) {
    renderPackageProgress(packageItemsFromChecks(packageChecks));
    applyPackageChecks(packageChecks);
  }

  if (report.phase === "packages-installed") {
    ["preflight", "packages", "config", "services", "ports", "http", "report"].forEach((stage) => markStage(stage, "성공"));
    setProgress(nodes.installProgress, 100);
    state.installCompleted = true;
    setReportReady(true);
    refreshInstallButtonState();
    return;
  }

  if (report.phase === "package-failed") {
    markStage("preflight", "성공");
    markStage("packages", "실패");
    setProgress(nodes.installProgress, 100);
    state.installCompleted = false;
    setReportReady(true);
    refreshInstallButtonState("복구 후 다시 시도");
    return;
  }

  if (report.phase === "prepared") {
    markStage("preflight", "성공");
    setProgress(nodes.installProgress, 20);
    state.installCompleted = false;
    setReportReady(true);
    refreshInstallButtonState("복구 후 다시 시도");
  }
}

function renderInstallReport(report) {
  const link = accessLink(report.domain, report.phase);
  nodes.reportOutput.innerHTML = [
    reportSummaryCard("기본 서버 구성 완료", [
      ["도메인", report.domain],
      ["접속 주소", link.html],
      ["웹서버 / PHP", `${runtimeLabel(report.web_server)} / ${phpRuntimeLabel(report.php_version, report.php_source)}`],
      ["데이터베이스", `${databaseLabel(report.database)} (${databaseVersionLabel(report.database_version)})`],
      ["앱 패키지", appPackageLabel(report.app_package)],
      ["앱 문서 루트", report.app_document_root || "-"],
      ["사이트 계정", report.site_user],
      ["웹루트", report.web_root],
      ["메일", mailModeLabel(report.mail_mode)],
      ["SMTP 서버", report.smtp_host || "-"],
      ["DNS/IP 확인", report.dns_check ? "수행" : "건너뜀"],
      ["단계", report.phase],
      ["상태 파일", report.state_path],
      ["소유 파일 목록", report.owned_files_path],
      ["설정 안내서", report.setup_guide_path || "-"],
    ], link.hint),
    listCard("완료된 작업", report.completed_steps),
    checksCard("안전장치", report.safety_checks),
    checksCard("설치 전 패키지 기준", report.preinstall_package_checks),
    checksCard("설치 패키지 검증", report.package_checks),
    checksCard("서비스 검증", report.service_checks),
    checksCard("포트 검증", report.port_checks),
    checksCard("DNS / 네트워크 검증", report.network_checks),
    checksCard("런타임 설정", report.runtime_checks),
    checksCard("DB 설정", report.database_checks),
    checksCard("방화벽 설정", report.firewall_checks),
    checksCard("웹서버 / 도메인 연결 검증", report.vhost_checks),
    checksCard("메일 발송 검증", report.mail_checks),
    checksCard("SSL / Certbot 검증", report.certbot_checks),
    checksCard("웹앱 설치", report.app_checks),
    checksCard("앱 요구사항", report.app_requirements),
  ].join("");

  ["preflight", "packages", "config", "services", "ports", "http", "report"].forEach((stage) => markStage(stage, "성공"));
  setProgress(nodes.installProgress, 100);
  applyPackageChecks(report.package_checks);
  state.installCompleted = true;
  setReportReady(true);
  refreshInstallButtonState();
  state.savedReportPayload = {
    exists: true,
    path: "방금 생성됨",
    content: JSON.stringify(report),
  };
  saveWizardState();
}

function renderSavedReport(payload) {
  state.savedReportPayload = payload;
  if (!payload.exists) {
    nodes.reportOutput.innerHTML = `<div class="empty-state">아직 생성된 리포트가 없습니다.</div>`;
    setReportReady(false);
    saveWizardState();
    return;
  }

  let report;
  try {
    report = JSON.parse(payload.content);
  } catch (_error) {
    nodes.reportOutput.innerHTML = `
      <section class="report-card">
        <h3>리포트 원문</h3>
        <pre class="code-box mt-3">${escapeHtml(payload.content)}</pre>
      </section>
    `;
    setReportReady(true);
    saveWizardState();
    return;
  }

  const link = accessLink(report.domain || "example.com", report.phase);
  nodes.reportOutput.innerHTML = [
    reportSummaryCard("저장된 설치 리포트", [
      ["리포트 파일", payload.path],
      ["도메인", report.domain || "-"],
      ["접속 주소", link.html],
      ["단계", report.phase || "-"],
      ["웹서버 / PHP", `${runtimeLabel(report.web_server)} / ${phpRuntimeLabel(report.php_version, report.php_source)}`],
      ["데이터베이스", databaseLabel(report.database)],
      ["앱 패키지", appPackageLabel(report.app_package || report.app_profile)],
      ["앱 문서 루트", report.app_document_root || "-"],
      ["사이트 계정", report.site_user || "-"],
      ["웹루트", report.web_root || "-"],
      ["메일", mailModeLabel(report.mail_mode)],
      ["SMTP 서버", report.smtp_host || "-"],
      ["DNS/IP 확인", report.dns_check ? "수행" : "건너뜀"],
      ["설정 안내서", report.setup_guide_path || "-"],
    ], link.hint),
    checksCard("안전장치", report.safety_checks),
    checksCard("설치 전 패키지 기준", report.preinstall_package_checks),
    checksCard("설치 패키지 검증", report.package_checks),
    checksCard("서비스 검증", report.service_checks),
    checksCard("포트 검증", report.port_checks),
    checksCard("DNS / 네트워크 검증", report.network_checks),
    checksCard("런타임 설정", report.runtime_checks),
    checksCard("DB 설정", report.database_checks),
    checksCard("방화벽 설정", report.firewall_checks),
    checksCard("웹서버 / 도메인 연결 검증", report.vhost_checks),
    checksCard("메일 발송 검증", report.mail_checks),
    checksCard("SSL / Certbot 검증", report.certbot_checks),
    checksCard("웹앱 설치", report.app_checks),
    checksCard("앱 요구사항", report.app_requirements),
    report.problem ? listCard("문제", [report.problem]) : "",
  ].join("");
  setReportReady(true);
  restoreInstallStateFromReport(report);
  saveWizardState();
}

function renderResetReport(report) {
  nodes.reportOutput.innerHTML = [
    reportSummaryCard("설치 기록 정리 완료", [
      ["미리보기", report.dry_run ? "예" : "아니오"],
      ["의미", "설치기 소유 설정과 웹루트 기록을 정리했습니다. apt 패키지는 제거하지 않습니다."],
    ]),
    listCard("삭제됨", report.removed),
    listCard("이미 없던 항목", report.missing),
  ].join("");
}

function renderRollbackReport(report) {
  nodes.reportOutput.innerHTML = [
    reportSummaryCard("패키지 되돌리기 완료", [
      ["미리보기", report.dry_run ? "예" : "아니오"],
      ["단계", report.phase],
      ["의미", "설치 직후 상태 기준으로 서비스 정리, 패키지 제거, 설치 기록 정리를 시도했습니다."],
    ]),
    actionCard("서비스 처리", report.service_actions),
    actionCard("패키지 처리", report.package_actions),
    listCard("설치 기록 삭제", report.metadata_reset.removed),
    listCard("이미 없던 설치 기록", report.metadata_reset.missing),
  ].join("");
}

function reportSummaryCard(title, entries, note = "") {
  const rows = entries.map(([key, value]) => `
    <div>
      <dt>${escapeHtml(key)}</dt>
      <dd>${value && String(value).startsWith("<a ") ? value : escapeHtml(value ?? "-")}</dd>
    </div>
  `).join("");
  return `
    <section class="report-card">
      <h3>${escapeHtml(title)}</h3>
      ${note ? `<p class="mt-2 text-sm text-base-content/60">${escapeHtml(note)}</p>` : ""}
      <dl>${rows}</dl>
    </section>
  `;
}

function listCard(title, items) {
  const rows = Array.isArray(items) && items.length ? items : ["없음"];
  return `
    <section class="report-card">
      <h3>${escapeHtml(title)}</h3>
      <ul class="report-list">${rows.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul>
    </section>
  `;
}

function checksCard(title, checks) {
  const rows = Array.isArray(checks) && checks.length ? checks : [{ name: "없음", status: "pending", message: "표시할 항목이 없습니다." }];
  return `
    <section class="report-card">
      <h3>${escapeHtml(title)}</h3>
      <div class="result-list mt-3">
        ${rows.map((check) => {
          const normalizedStatus = checkStatus(check.status);
          return `
          <div class="result-row" data-status="${normalizedStatus}">
            <div class="result-copy">
              <span>${escapeHtml(check.name)}</span>
              <p>${escapeHtml(checkMessage(check.status, check.message))}</p>
            </div>
            <strong>${escapeHtml(checkStatusLabel(check.status, normalizedStatus))}</strong>
          </div>
        `;
        }).join("")}
      </div>
    </section>
  `;
}

function actionCard(title, actions) {
  const rows = Array.isArray(actions) && actions.length ? actions : [{ name: "없음", status: "pending", message: "처리할 항목이 없습니다." }];
  return `
    <section class="report-card">
      <h3>${escapeHtml(title)}</h3>
      <div class="result-list mt-3">
        ${rows.map((action) => `
          <div class="result-row" data-status="${actionStatus(action.status)}">
            <div class="result-copy">
              <span>${escapeHtml(action.name)}</span>
              <p>${escapeHtml(action.message || "")}</p>
            </div>
            <strong>${escapeHtml(action.status || "대기")}</strong>
          </div>
        `).join("")}
      </div>
    </section>
  `;
}

function checkStatus(status) {
  if (status === "pass") {
    return "pass";
  }
  if (["installed", "not-installed", "skipped", "deferred", "planned", "manual", "pending"].includes(status)) {
    return "info";
  }
  if (status === "warn" || status === "unknown") {
    return "warn";
  }
  return status === "fail" ? "fail" : "pending";
}

function checkStatusLabel(status, normalizedStatus) {
  return statusLabel[status] || statusLabel[normalizedStatus] || status || "대기";
}

function checkMessage(status, message) {
  const labelByMessage = {
    "package was already installed before G7 installer ran": "설치 전부터 있던 패키지입니다. 되돌리기 때 보존합니다.",
    "package was absent before G7 installer ran": "설치 전에는 없던 패키지입니다. 이번 설치기가 설치했습니다.",
    "package preinstall state is unknown": "설치 전 패키지 상태를 확인하지 못했습니다.",
  };
  return labelByMessage[message] || message || "";
}

function actionStatus(status) {
  if (["removed", "disabled", "reset", "pass", "ok"].includes(status)) {
    return "pass";
  }
  if (["skipped", "missing", "pending"].includes(status)) {
    return "warn";
  }
  return "fail";
}

function accessLink(domain, phase = "packages-installed") {
  if (["prepared", "package-failed", "packages-installed"].includes(phase)) {
    return {
      html: `<span class="text-base-content/60">사이트 페이지는 아직 생성 전입니다.</span>`,
      hint: "HTTP 도메인 연결 검증이 끝나면 도메인 접속 링크를 제공합니다. HTTPS는 인증서 단계 후 활성화됩니다.",
    };
  }

  const href = `http://${domain}`;
  return {
    html: `<a class="link link-primary" href="${escapeHtml(href)}" target="_blank" rel="noreferrer">${escapeHtml(href)}</a>`,
    hint: "인증서 발급 전까지는 HTTP 주소로 먼저 접속합니다.",
  };
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
  const provisioning = Array.isArray(report.provisioning) && report.provisioning.length
    ? report.provisioning.map((section) => {
        const settings = Array.isArray(section.settings) && section.settings.length
          ? section.settings.map((item) => `  - ${item.key}: ${item.value}`).join("\n")
          : "  - 설정 없음";
        return `- ${section.title}: ${section.summary}\n${settings}`;
      }).join("\n")
    : "- 없음";
  const stopConditions = report.stop_conditions.length
    ? report.stop_conditions.map((item) => `- ${item}`).join("\n")
    : "- 없음";

  return [
    "설치 계획 요약",
    `도메인: ${report.domain}`,
    `웹서버: ${runtimeLabel(report.web_server)}`,
    `PHP: ${phpRuntimeLabel(report.php_version, report.php_source)}`,
    `데이터베이스: ${databaseLabel(report.database)} (${databaseVersionLabel(report.database_version)})`,
    `설치할 앱: ${appPackageLabel(report.app_package)} - 서버 스택 준비 후 마지막 설치 대상`,
    `앱 문서 루트: ${report.app_document_root || "-"}`,
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
    "프로비저닝 계획:",
    provisioning,
    "",
    "중단 조건:",
    stopConditions,
  ].join("\n");
}

function renderErrorReport(title, message) {
  nodes.reportOutput.innerHTML = `
    <section class="report-card">
      <h3>${escapeHtml(title)}</h3>
      <p class="mt-3 whitespace-pre-line text-sm text-error">${escapeHtml(message)}</p>
    </section>
  `;
}

function resetInstallStages() {
  stopPackageTicker();
  ["preflight", "packages", "config", "services", "ports", "http", "report"].forEach((stage) => {
    markStage(stage, "대기");
  });
  setProgress(nodes.installProgress, 0);
  resetPackageProgressRows();
}

function installConfirmSummaryHtml(payload) {
  const entries = [
    ["도메인", payload.domain],
    ["웹서버 / PHP", `${runtimeLabel(payload.web_server)} / ${phpRuntimeLabel(payload.php_version, payload.php_source)}`],
    ["데이터베이스", `${databaseLabel(payload.database)} (${databaseVersionLabel(payload.database_version)})`],
    ["앱 패키지", appPackageLabel(payload.app_package)],
    ["웹루트", payload.web_root || payload.web_root_mode],
    ["메일", mailModeLabel(payload.mail_mode)],
  ];

  return `<dl>${entries.map(([key, value]) => `
    <div>
      <dt>${escapeHtml(key)}</dt>
      <dd>${escapeHtml(value)}</dd>
    </div>
  `).join("")}</dl>`;
}

function confirmInstallStart(payload = optionPayload()) {
  if (!nodes.installConfirmDialog?.showModal) {
    return Promise.resolve(window.confirm("기본 서버 구성을 시작할까요?"));
  }

  nodes.installConfirmSummary.innerHTML = installConfirmSummaryHtml(payload);
  nodes.installConfirmDialog.returnValue = "cancel";
  nodes.installConfirmDialog.showModal();

  return new Promise((resolve) => {
    nodes.installConfirmDialog.addEventListener("close", () => {
      resolve(nodes.installConfirmDialog.returnValue === "start");
    }, { once: true });
  });
}

function recoveryConfirmContent(action) {
  if (action === "rollback") {
    return {
      title: "패키지 되돌리기를 실행할까요?",
      message: "설치 직후 운영 데이터가 없을 때만 사용하세요. 서비스 중지, 패키지 제거, 설치 기록 정리를 진행합니다.",
      yesClass: "btn btn-error icon-button",
      rows: [
        ["대상", "이번 설치기가 설치한 apt 패키지와 서비스"],
        ["보존", "설치 전부터 있던 패키지와 운영자가 만든 파일"],
        ["실행 후", "서버 점검 단계로 돌아가 다시 테스트할 수 있습니다."],
      ],
    };
  }

  return {
    title: "설치 기록만 정리할까요?",
    message: "설치기 소유 설정, 도메인 연결 설정, 웹루트 기록을 정리합니다. apt 패키지와 기존 운영 웹서비스는 제거하지 않습니다.",
    yesClass: "btn btn-primary icon-button",
    rows: [
      ["대상", "installer 상태 파일, 리포트, installer가 만든 도메인 연결 설정/웹루트"],
      ["보존", "apt 패키지와 서비스, 설치 전부터 있던 운영 파일"],
      ["실행 후", "패키지가 남아 있으면 신규 설치 점검이 막힐 수 있습니다."],
    ],
  };
}

function recoveryConfirmSummaryHtml(action) {
  const content = recoveryConfirmContent(action);
  return `<dl>${content.rows.map(([key, value]) => `
    <div>
      <dt>${escapeHtml(key)}</dt>
      <dd>${escapeHtml(value)}</dd>
    </div>
  `).join("")}</dl>`;
}

function confirmRecoveryAction(action) {
  const content = recoveryConfirmContent(action);
  if (!nodes.recoveryConfirmDialog?.showModal) {
    return Promise.resolve(window.confirm(content.message));
  }

  nodes.recoveryConfirmTitle.textContent = content.title;
  nodes.recoveryConfirmMessage.textContent = content.message;
  nodes.recoveryConfirmSummary.innerHTML = recoveryConfirmSummaryHtml(action);
  nodes.recoveryConfirmYes.className = content.yesClass;
  nodes.recoveryConfirmDialog.returnValue = "cancel";
  nodes.recoveryConfirmDialog.showModal();

  return new Promise((resolve) => {
    nodes.recoveryConfirmDialog.addEventListener("close", () => {
      resolve(nodes.recoveryConfirmDialog.returnValue === "confirm");
    }, { once: true });
  });
}

function resetWizardForRetry() {
  clearWizardState();
  state.doctorReport = null;
  state.planReport = null;
  state.savedReportPayload = null;
  state.recoveryStatus = null;
  setPlanReady(false);
  setDoctorPassed(false);
  setReportReady(false);
  state.installRunning = false;
  state.installCompleted = false;
  state.currentOperation = null;
  stopPackageTicker();
  hideAlert(nodes.planStatus);
  hideAlert(nodes.installStatus);
  hideAlert(nodes.reportStatus);
  hideReportProgress();
  resetInstallStages();
  nodes.doctorResults.innerHTML = `<div class="empty-state">아직 점검 전입니다. 점검 실행을 누르세요.</div>`;
  nodes.planOutput.textContent = "옵션을 확인한 뒤 계획 생성을 누르세요.";
  renderPackageProgress([]);
  nodes.reportOutput.innerHTML = `<div class="empty-state">아직 리포트가 없습니다.</div>`;
  refreshInstallButtonState();
  renderRecoveryStatus(null);
  showStep("check");
}

function restoreWizardState() {
  const saved = readWizardState();
  if (!saved || typeof saved !== "object") {
    return;
  }

  applyFormValues(saved.form);
  refreshFormState({ preservePlan: true });

  if (saved.doctorReport) {
    renderDoctor(saved.doctorReport);
  }
  if (saved.planReport) {
    state.planReport = saved.planReport;
    nodes.planOutput.textContent = renderPlanReport(saved.planReport);
    renderPackageProgress(flattenPlanPackages(saved.planReport.packages));
    setPlanReady(true);
  }
  if (saved.savedReportPayload) {
    renderSavedReport(saved.savedReportPayload);
  }
  if (saved.recoveryStatus) {
    renderRecoveryStatus(saved.recoveryStatus);
  }

  state.activeStep = normalizedStep(saved.activeStep || state.activeStep);
  if (saved.flags) {
    setDoctorPassed(Boolean(saved.flags.doctorPassed || state.doctorPassed));
    setPlanReady(Boolean(saved.flags.planReady || state.planReady));
    setReportReady(Boolean(saved.flags.reportReady || state.reportReady));
    state.installCompleted = Boolean(saved.flags.installCompleted || state.installCompleted);
    refreshInstallButtonState();
  }
}

async function syncServerState() {
  if (!state.authenticated) {
    renderRecoveryStatus(null);
    return;
  }

  await refreshRecoveryStatus();

  try {
    const reportPayload = await apiFetch("/api/report");
    state.savedReportPayload = reportPayload;
    if (reportPayload.exists) {
      renderSavedReport(reportPayload);
      const report = parseSavedReport(reportPayload);
      restoreInstallStateFromReport(report);
    } else {
      state.savedReportPayload = reportPayload;
      if (!state.installRunning) {
        setReportReady(false);
        if (!state.planReady) {
          state.installCompleted = false;
          refreshInstallButtonState();
        }
      }
    }
    saveWizardState();
  } catch (error) {
    log(formatError(error));
  }
}

async function runRecoveryAction(action, button) {
  if (state.operationLocked || state.installRunning) {
    log("진행 중인 서버 작업이 끝난 뒤 다시 시도하세요.");
    return;
  }

  const statusNode = state.activeStep === "install"
    ? nodes.installStatus
    : (state.activeStep === "check" ? nodes.doctorStatus : nodes.reportStatus);

  if (action === "rollback" && !state.recoveryStatus?.can_rollback) {
    setAlert(statusNode, "warning", "되돌리기 불가", state.recoveryStatus?.rollback_reason || "안전 조건을 만족하지 않습니다.");
    return;
  }

  if (action === "reset" && !state.recoveryStatus?.can_reset) {
    setAlert(statusNode, "warning", "리셋 불가", "설치기 소유 기록이 없거나 패키지 되돌리기를 먼저 해야 합니다.");
    return;
  }

  const confirmed = await confirmRecoveryAction(action);
  if (!confirmed) {
    log(action === "rollback" ? "패키지 되돌리기 취소" : "리셋 취소");
    return;
  }

  const endpoint = action === "rollback" ? "/api/rollback" : "/api/reset";
  const busyText = action === "rollback" ? "되돌리는 중" : "리셋 중";
  const successTitle = action === "rollback" ? "패키지 되돌리기 완료" : "리셋 완료";
  const originalText = buttonLabel(button);
  let recoveryCompleted = false;

  try {
    state.currentOperation = action;
    setButtonLabel(button, busyText);
    setOperationLocked(true);
    showReportProgress(5);
    hideAlert(statusNode);
    hideAlert(nodes.reportStatus);
    log(action === "rollback" ? "패키지 되돌리기 실행" : "리셋 실행");
    const report = await apiFetch(endpoint, {
      method: "POST",
      body: JSON.stringify({ dry_run: false }),
    });

    if (action === "rollback") {
      renderRollbackReport(report);
    } else {
      renderResetReport(report);
    }

    showReportProgress(100);
    setAlert(
      statusNode,
      "success",
      successTitle,
      action === "rollback"
        ? "서비스 중지, apt 패키지 제거, 설치 기록 정리를 완료했습니다."
        : "installer 소유 설정과 웹루트 기록을 정리했습니다.",
    );
    log(successTitle);
    clearWizardState();
    state.installCompleted = false;
    state.planReport = null;
    state.savedReportPayload = null;
    setPlanReady(false);
    setReportReady(false);
    refreshInstallButtonState();
    await refreshRecoveryStatus();
    await runDoctorCheck();
    recoveryCompleted = true;
  } catch (error) {
    renderErrorReport(action === "rollback" ? "패키지 되돌리기 실패" : "리셋 실패", formatError(error));
    setAlert(statusNode, "error", action === "rollback" ? "패키지 되돌리기 실패" : "리셋 실패", formatError(error));
    log(formatError(error));
    await refreshRecoveryStatus();
  } finally {
    state.currentOperation = null;
    setOperationLocked(false);
    if (button && originalText) {
      setButtonLabel(button, originalText);
    }
    renderRecoveryStatus(state.recoveryStatus);
    setDoctorPassed(Boolean(state.doctorReport?.install_allowed));
    setPlanReady(Boolean(state.planReport || state.planReady));
    refreshInstallButtonState(state.installCompleted ? null : undefined);
    saveWizardState();
  }

  if (recoveryCompleted) {
    showStep("check");
  }
}

function bindHelpTooltips() {
  const tooltip = nodes.floatingHelp;
  if (!tooltip) {
    return;
  }

  const hideTooltip = () => {
    tooltip.hidden = true;
    tooltip.textContent = "";
  };

  const showTooltip = (button) => {
    const text = button.parentElement?.querySelector(".help-text")?.textContent?.trim();
    if (!text) {
      return;
    }

    tooltip.textContent = text;
    tooltip.hidden = false;
    const rect = button.getBoundingClientRect();
    const width = tooltip.offsetWidth;
    const height = tooltip.offsetHeight;
    const margin = 12;
    const left = Math.min(Math.max(rect.left + rect.width / 2 - width / 2, margin), window.innerWidth - width - margin);
    let top = rect.top - height - margin;
    if (top < margin) {
      top = rect.bottom + margin;
    }

    tooltip.style.left = `${left}px`;
    tooltip.style.top = `${top}px`;
  };

  document.querySelectorAll(".help-circle").forEach((button) => {
    button.addEventListener("pointerenter", () => showTooltip(button));
    button.addEventListener("focus", () => showTooltip(button));
    button.addEventListener("pointerleave", hideTooltip);
    button.addEventListener("blur", hideTooltip);
  });

  window.addEventListener("resize", hideTooltip);
  window.addEventListener("scroll", hideTooltip, true);
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      hideTooltip();
    }
  });
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
        const payload = optionPayload();
        const passwordError = validateSitePassword(payload);
        if (passwordError) {
          setAlert(nodes.planStatus, "error", "설치 계획 생성 실패", passwordError);
          log(passwordError);
          return;
        }
        hideAlert(nodes.planStatus);
        setPlanReady(false);
        log("설치 계획 생성");
        const report = await apiFetch("/api/plan", {
          method: "POST",
          body: JSON.stringify(payload),
        });
        state.planReport = report;
        nodes.planOutput.textContent = renderPlanReport(report);
        renderPackageProgress(flattenPlanPackages(report.packages));
        setAlert(nodes.planStatus, "success", "설치 계획 생성 완료", `${report.packages.length}개 패키지 묶음과 ${report.files.length}개 파일 변경 계획을 확인했습니다.`);
        setPlanReady(true);
        state.installCompleted = false;
        setReportReady(false);
        refreshInstallButtonState();
        saveWizardState();
        log(`설치 계획 준비 완료: packages=${report.packages.length}, files=${report.files.length}`);
      } catch (error) {
        state.planReport = null;
        nodes.planOutput.textContent = formatError(error);
        setAlert(nodes.planStatus, "error", "설치 계획 생성 실패", formatError(error));
        setPlanReady(false);
        renderPackageProgress([]);
        saveWizardState();
        log(formatError(error));
      }
    });
  });

  document.querySelector("#install-button").addEventListener("click", async (event) => {
    const installButton = event.currentTarget;
    if (state.installRunning || state.installCompleted) {
      return;
    }

    const payload = optionPayload();
    const passwordError = validateSitePassword(payload);
    if (passwordError) {
      setAlert(nodes.installStatus, "error", "기본 서버 구성 실패", passwordError);
      log(passwordError);
      return;
    }

    const confirmed = await confirmInstallStart(payload);
    if (!confirmed) {
      log("기본 서버 구성 취소");
      return;
    }

    state.currentOperation = "install";
    state.installRunning = true;
    state.installCompleted = false;
    setReportReady(false);
    refreshInstallButtonState();
    resetInstallStages();
    hideAlert(nodes.installStatus);
    hideReportProgress();
    startPackageTicker();

    try {
      markStage("preflight", "진행");
      setAlert(nodes.installStatus, "info", "기본 서버 구성 진행 중", "apt 패키지 설치, Nginx 도메인 연결 설정 생성, HTTP 검증을 진행합니다.");
      log("기본 서버 구성 시작");
      const report = await apiFetch("/api/install/prepare", {
        method: "POST",
        body: JSON.stringify(payload),
      });
      renderInstallReport(report);
      await refreshRecoveryStatus();
      setAlert(nodes.installStatus, "success", "기본 서버 구성 완료", "결과 리포트에서 패키지, 서비스, 포트, 도메인 연결 검증 결과와 프로비저닝 계획을 확인하세요.");
      showStep("report");
      log(`기본 서버 구성 완료: ${report.phase}`);
    } catch (error) {
      markStage("packages", "실패");
      stopPackageTicker();
      const reportPayload = await apiFetch("/api/report").catch(() => null);
      if (reportPayload?.exists) {
        renderSavedReport(reportPayload);
        restoreInstallStateFromReport(parseSavedReport(reportPayload));
      } else {
        renderErrorReport("기본 서버 구성 실패", `${formatError(error)}\n\n리포트와 로그를 확인하세요. 패키지 후보 검사 결과와 복구 버튼 상태를 먼저 확인하세요.`);
      }
      setAlert(nodes.installStatus, "error", "기본 서버 구성 실패", formatError(error));
      setReportReady(true);
      await refreshRecoveryStatus();
      showStep("report");
      log(formatError(error));
    } finally {
      state.currentOperation = null;
      state.installRunning = false;
      refreshInstallButtonState(state.installCompleted ? null : "다시 시도");
      if (installButton && !state.installCompleted) {
        installButton.disabled = false;
      }
    }
  });

  document.querySelector("#report-button").addEventListener("click", async (event) => {
    await withBusy(event.currentTarget, "새로고침", async () => {
      try {
        hideAlert(nodes.reportStatus);
        const report = await apiFetch("/api/report");
        renderSavedReport(report);
        await refreshRecoveryStatus();
        setAlert(
          nodes.reportStatus,
          report.exists ? "success" : "warning",
          report.exists ? "리포트 확인 완료" : "리포트 없음",
          report.exists ? "서버에 저장된 리포트를 불러왔습니다." : "아직 생성된 리포트가 없습니다.",
        );
        log(`리포트 불러오기: ${report.exists ? "있음" : "없음"}`);
      } catch (error) {
        renderErrorReport("리포트 불러오기 실패", formatError(error));
        setAlert(nodes.reportStatus, "error", "리포트 불러오기 실패", formatError(error));
        log(formatError(error));
      }
    });
  });

  document.querySelectorAll("[data-recovery-refresh]").forEach((button) => {
    button.addEventListener("click", async (event) => {
      await withBusy(event.currentTarget, "확인 중", async () => {
        await refreshRecoveryStatus();
        await runDoctorCheck();
      });
    });
  });

  document.querySelectorAll("[data-recovery-action]").forEach((button) => {
    button.addEventListener("click", async (event) => {
      await runRecoveryAction(event.currentTarget.dataset.recoveryAction, event.currentTarget);
    });
  });

  document.querySelector("#start-over-button").addEventListener("click", resetWizardForRetry);

  nodes.optionsForm.addEventListener("input", refreshFormState);
  nodes.optionsForm.addEventListener("change", refreshFormState);
}

async function boot() {
  hydrateIcons();
  applyTheme(state.theme);
  bindEvents();
  bindHelpTooltips();
  refreshFormState({ preservePlan: true, persist: false });
  connectEvents();

  try {
    state.bootstrap = await loadBootstrap();
    state.csrfToken = state.bootstrap.csrf_token;
    state.authenticated = state.bootstrap.auth.authenticated;
    setConnectionStatus("연결됨", "badge-success");
    setAlert(
      nodes.loginStatus,
      "success",
      "접속 확인 완료",
      "서버 비밀번호 입력 없이 접속 확인 주소로 설치 권한을 확인했습니다.",
    );

    if (state.bootstrap.domain) {
      nodes.domain.value = state.bootstrap.domain;
    }
    refreshFormState({ preservePlan: true, persist: false });
    restoreWizardState();
    await syncServerState();
    showStep(normalizedStep(window.location.hash.replace("#", "") || state.activeStep), { pushHistory: false });
    writeStepHistory(state.activeStep, true);
    log("웹 컨트롤러 준비 완료");
    log(`접속 상태: ${state.bootstrap.auth.status}`);
    if (state.bootstrap.auth.client_ip) {
      log(`접속 IP 잠금: ${state.bootstrap.auth.client_ip}`);
    }
  } catch (error) {
    setConnectionStatus("오류", "badge-error");
    setAlert(nodes.loginStatus, "error", "접속 확인 실패", `${formatError(error)}\n터미널에 출력된 접속 확인 주소로 다시 접속하세요.`);
    log(`${formatError(error)}\n터미널에 출력된 접속 확인 주소로 다시 접속하세요.`);
  }
}

boot();
