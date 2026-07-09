const state = {
  activeStep: "login",
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
  planSignature: null,
  planGenerating: false,
  savedReportPayload: null,
  recoveryStatus: null,
  currentOperation: null,
  operationLocked: false,
  provisionActionResults: {},
  planPackages: [],
  packageTicker: null,
  liveLogEntries: [],
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
  databaseName: document.querySelector("#database-name-input"),
  databaseUser: document.querySelector("#database-user-input"),
  databasePassword: document.querySelector("#database-password"),
  databasePasswordConfirm: document.querySelector("#database-password-confirm"),
  databasePasswordStatus: document.querySelector("#database-password-status"),
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
  provisionOutput: document.querySelector("#provision-output"),
  doctorResults: document.querySelector("#doctor-results"),
  loginStatus: document.querySelector("#login-status"),
  doctorStatus: document.querySelector("#doctor-status"),
  planStatus: document.querySelector("#plan-status"),
  installStatus: document.querySelector("#install-status"),
  reportStatus: document.querySelector("#report-status"),
  provisionStatus: document.querySelector("#provision-status"),
  installProgress: document.querySelector("#install-progress"),
  activityCurrentStage: document.querySelector("#activity-current-stage"),
  activityCurrentMessage: document.querySelector("#activity-current-message"),
  activityProgressLabel: document.querySelector("#activity-progress-label"),
  activityLogCount: document.querySelector("#activity-log-count"),
  installLiveLog: document.querySelector("#install-live-log"),
  reportProgress: document.querySelector("#report-progress"),
  packageProgressList: document.querySelector("#package-progress-list"),
  packageProgressHelp: document.querySelector("#package-progress-help"),
  checkNextButton: document.querySelector("#check-next-button"),
  planButton: document.querySelector("#plan-button"),
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
  provisionRefreshButton: document.querySelector("#provision-refresh-button"),
  provisionActionDialog: document.querySelector("#provision-action-dialog"),
  provisionActionTitle: document.querySelector("#provision-action-title"),
  provisionActionSummary: document.querySelector("#provision-action-summary"),
  provisionActionStatus: document.querySelector("#provision-action-status"),
  provisionActionDetails: document.querySelector("#provision-action-details"),
  provisionActionResult: document.querySelector("#provision-action-result"),
  provisionActionRun: document.querySelector("#provision-action-run"),
  operationOverlay: document.querySelector("#operation-overlay"),
  operationOverlayTitle: document.querySelector("#operation-overlay-title"),
  operationOverlayMessage: document.querySelector("#operation-overlay-message"),
  operationOverlaySpinner: document.querySelector("#operation-overlay-spinner"),
  operationOverlayConfirm: document.querySelector("#operation-overlay-confirm"),
  promoPanel: document.querySelector("#promo-panel"),
  summaryPanel: document.querySelector("#summary-panel"),
  floatingHelp: document.querySelector("#floating-help"),
  summaryDomain: document.querySelector("#summary-domain"),
  summaryRuntime: document.querySelector("#summary-runtime"),
  summaryData: document.querySelector("#summary-data"),
  summaryApp: document.querySelector("#summary-app"),
};

const stepOrder = ["login", "check", "options", "plan", "install", "report", "provision"];
const stepRoutes = {
  login: "/setup/connect",
  check: "/setup/doctor",
  options: "/setup/options",
  plan: "/setup/plan",
  install: "/setup/install",
  report: "/setup/result",
  provision: "/setup/provision",
};
const routeToStep = Object.fromEntries(Object.entries(stepRoutes).map(([step, route]) => [route, step]));
const wizardStorageKey = "g7inst-wizard-state-v2";
const promoDismissStorageKey = "g7inst-promo-dismissed-v1";
const installStageOrder = [
  "preflight",
  "packages",
  "site",
  "vhost",
  "runtime",
  "database",
  "ssl",
  "app",
  "report",
];
const installStageLabels = {
  preflight: "서버 사전 점검",
  packages: "패키지 설치/검증",
  site: "사이트 계정/웹루트",
  vhost: "웹서버 vhost/HTTP 검증",
  runtime: "PHP/런타임 튜닝",
  database: "DB 튜닝/계정 생성",
  ssl: "SSL 인증서/HTTPS 검증",
  app: "웹앱 파일 배치",
  report: "리포트 생성",
};
const phaseCompletedStages = {
  prepared: ["preflight"],
  "package-failed": ["preflight"],
  "packages-installed": ["preflight", "packages"],
  "vhost-enabled": ["preflight", "packages", "site", "vhost"],
  "runtime-configured": ["preflight", "packages", "site", "vhost", "runtime"],
  "database-configured": ["preflight", "packages", "site", "vhost", "runtime", "database"],
  "tls-enabled": ["preflight", "packages", "site", "vhost", "runtime", "database", "ssl"],
  completed: installStageOrder,
};

const statusLabel = {
  pass: "통과",
  warn: "주의",
  fail: "실패",
  pending: "대기",
  info: "정보",
  installed: "기존 패키지",
  "not-installed": "설치 대상",
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
  "g7-core-template-engine": "G7 core 빌드 파일",
  "g7-install-lock": "G7 설치 잠금",
  "g7-artisan-about": "G7 artisan 상태",
  "g7-ckeditor-upload-limit": "CKEditor 업로드 제한",
  "app-browser-install": "브라우저 설치 확인",
  "phpinfo-summary": "PHP 정보 요약",
  "php-runtime-probe": "PHP 진단 실행",
  "php-runtime-limits": "PHP 한도 설정",
  "php-fpm-pool-values": "PHP-FPM pool 값",
  "frankenphp-binary": "FrankenPHP 바이너리",
  "frankenphp-service": "FrankenPHP 서비스",
  "g7-frankenphp": "FrankenPHP 서비스",
  "frankenphp-vhost": "FrankenPHP vhost",
  "frankenphp-runtime": "FrankenPHP 런타임",
  "frankenphp-restart": "FrankenPHP 재시작",
  "frankenphp-runtime-boundary": "FrankenPHP 공개 경계",
  "frankenphp-edge-runtime-reload": "Nginx edge reload",
  "frankenphp-https-vhost": "FrankenPHP HTTPS",
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
    php_version: "8.5",
    database: "mysql",
    database_version: "mysql-8.4",
    redis: "enable",
    mail_mode: "local-postfix",
    app_package: "gnuboard7",
    web_root_mode: "public-html",
    www_mode: "redirect-to-www",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
  apache: {
    domain: null,
    deployment_mode: "public",
    web_server: "apache",
    php_version: "8.5",
    database: "mysql",
    database_version: "mysql-8.4",
    redis: "enable",
    mail_mode: "local-postfix",
    app_package: "gnuboard7",
    web_root_mode: "public-html",
    www_mode: "redirect-to-www",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
  frankenphp: {
    domain: null,
    deployment_mode: "public",
    web_server: "frankenphp",
    php_version: "8.5",
    database: "mysql",
    database_version: "mysql-8.4",
    redis: "enable",
    mail_mode: "local-postfix",
    app_package: "gnuboard7",
    web_root_mode: "public-html",
    www_mode: "redirect-to-www",
    security_profile: "standard",
    ssh_policy: "audit-only",
  },
};

let operationOverlayResolve = null;

// Icon paths are sourced from lucide-static and rendered inline to avoid extra requests.
const iconSvg = {
  "check": "<path d=\"M20 6 9 17l-5-5\" />",
  "chevron-left": "<path d=\"m15 18-6-6 6-6\" />",
  "chevron-right": "<path d=\"m9 18 6-6-6-6\" />",
  "clipboard-list": "<rect width=\"8\" height=\"4\" x=\"8\" y=\"2\" rx=\"1\" ry=\"1\" /> <path d=\"M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2\" /> <path d=\"M12 11h4\" /> <path d=\"M12 16h4\" /> <path d=\"M8 11h.01\" /> <path d=\"M8 16h.01\" />",
  "download": "<path d=\"M12 15V3\" /> <path d=\"m7 10 5 5 5-5\" /> <path d=\"M5 21h14\" />",
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

function showOperationOverlay(title, message, options = {}) {
  if (!nodes.operationOverlay) {
    return;
  }

  const loading = options.loading !== false;
  nodes.operationOverlayTitle.textContent = title;
  nodes.operationOverlayMessage.textContent = message;
  nodes.operationOverlay.hidden = false;
  nodes.operationOverlaySpinner.hidden = !loading;
  nodes.operationOverlayConfirm.classList.toggle("hidden", loading);
  nodes.operationOverlayConfirm.disabled = loading;
  if (!loading) {
    setButtonLabel(nodes.operationOverlayConfirm, options.confirmLabel || "확인");
    nodes.operationOverlayConfirm.focus();
  }
}

function hideOperationOverlay() {
  if (!nodes.operationOverlay) {
    return;
  }

  nodes.operationOverlay.hidden = true;
  nodes.operationOverlaySpinner.hidden = false;
  nodes.operationOverlayConfirm.classList.add("hidden");
  nodes.operationOverlayConfirm.disabled = true;
}

function waitForOperationOverlayConfirm() {
  return new Promise((resolve) => {
    operationOverlayResolve = resolve;
  });
}

function completeOperationOverlay(title, message) {
  showOperationOverlay(title, message, { loading: false, confirmLabel: "확인" });
  return waitForOperationOverlayConfirm();
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

function renderActivityLog() {
  if (!nodes.installLiveLog) {
    return;
  }

  const recent = state.liveLogEntries.slice(-8);
  nodes.installLiveLog.innerHTML = recent.length
    ? recent.map((entry) => `
      <li>
        <time>${escapeHtml(entry.timestamp)}</time>${escapeHtml(entry.message)}
      </li>
    `).join("")
    : "<li>아직 실행 로그가 없습니다.</li>";

  if (nodes.activityLogCount) {
    nodes.activityLogCount.textContent = `${state.liveLogEntries.length}줄`;
  }
}

function setActivityStatus(stageLabel, message = "", percent = null) {
  if (nodes.activityCurrentStage && stageLabel) {
    nodes.activityCurrentStage.textContent = stageLabel;
  }
  if (nodes.activityCurrentMessage && message) {
    nodes.activityCurrentMessage.textContent = localizeMessage(message);
  }
  if (nodes.activityProgressLabel && percent !== null) {
    const value = Math.max(0, Math.min(100, Math.round(Number(percent) || 0)));
    nodes.activityProgressLabel.textContent = `${value}%`;
  }
}

function clearActivityLog() {
  state.liveLogEntries = [];
  renderActivityLog();
  setActivityStatus("대기 중", "기본 구성을 시작하면 서버 작업 로그와 단계별 결과가 이곳에 표시됩니다.", 0);
}

function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  const localizedMessage = localizeMessage(message || "");
  state.liveLogEntries.push({ timestamp, message: localizedMessage });
  if (state.liveLogEntries.length > 200) {
    state.liveLogEntries = state.liveLogEntries.slice(-200);
  }
  if (nodes.log) {
    if (nodes.log.textContent.trim() === "웹 컨트롤러를 불러오는 중...") {
      nodes.log.textContent = "";
    }
    nodes.log.textContent += `\n[${timestamp}] ${localizedMessage}`;
    nodes.log.scrollTop = nodes.log.scrollHeight;
  }
  renderActivityLog();
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
  message = String(message || "");
  if (errorLabel[message]) {
    return errorLabel[message];
  }

  const eventLabel = {
    "event stream connected": "실시간 로그 연결됨",
    "running server check": "서버 점검 실행 중",
    "building install plan": "설치 계획 계산 중",
    "install plan ready": "설치 계획 계산 완료",
    "install progress: starting preflight": "설치 진행: 서버 사전 점검 시작",
    "install progress: running server install": "설치 진행: 서버 패키지와 기본 구성을 적용 중",
    "preflight started": "서버 사전 점검을 시작했습니다.",
    "preflight passed": "서버 사전 점검을 통과했습니다.",
    "install progress: preflight passed": "설치 진행: 사전 점검 완료",
    "packages installed": "패키지 설치와 검증이 끝났습니다.",
    "install progress: packages installed": "설치 진행: 패키지 설치 완료",
    "site account and web root configured": "사이트 계정과 웹루트를 구성했습니다.",
    "install progress: site account and web root configured": "설치 진행: 사이트 계정/웹루트 구성 완료",
    "web server vhost and HTTP smoke verified": "웹서버 vhost와 HTTP 접속을 검증했습니다.",
    "install progress: vhost verified": "설치 진행: 웹서버 vhost 검증 완료",
    "PHP runtime configured": "PHP 런타임 설정을 적용했습니다.",
    "install progress: runtime configured": "설치 진행: PHP 런타임 설정 완료",
    "database configured": "데이터베이스 설정을 적용했습니다.",
    "install progress: database configured": "설치 진행: 데이터베이스 구성 완료",
    "TLS certificate and HTTPS vhost verified": "SSL 인증서와 HTTPS vhost를 검증했습니다.",
    "install progress: TLS configured": "설치 진행: SSL/HTTPS 구성 완료",
    "web app files prepared": "웹앱 파일 배치를 완료했습니다.",
    "setup guide and report prepared": "설정 안내서와 리포트를 생성했습니다.",
    "install progress: report ready": "설치 진행: 리포트 준비 완료",
    "install progress: failed": "설치 진행: 실패 항목 발생",
    "server install completed": "서버 설치 작업이 완료되었습니다.",
    "running reset": "재설치 초기화 실행 중",
    "reset completed": "재설치 초기화 완료",
    "running package rollback": "패키지 되돌리기 실행 중",
    "package rollback completed": "패키지 되돌리기 완료",
  };
  if (eventLabel[message]) {
    return eventLabel[message];
  }

  if (message.startsWith("server check completed: install_allowed=")) {
    const allowed = message.endsWith("true");
    return `서버 점검 완료: ${allowed ? "설치 가능" : "설치 차단"}`;
  }
  if (message.startsWith("setup access locked to client IP:")) {
    return message.replace("setup access locked to client IP:", "접속 IP 잠금 완료:");
  }
  if (message.startsWith("plan failed:")) {
    return message.replace("plan failed:", "설치 계획 생성 실패:");
  }
  if (message.startsWith("reset failed:")) {
    return message.replace("reset failed:", "재설치 초기화 실패:");
  }
  if (message.startsWith("rollback failed:")) {
    return message.replace("rollback failed:", "패키지 되돌리기 실패:");
  }

  if (message.startsWith("install failed:")) {
    return message.replace("install failed:", "설치 실패:");
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

function compactText(value, maxLength) {
  const text = String(value ?? "").replace(/\s+/g, " ").trim();
  if (text.length <= maxLength) {
    return text;
  }

  return `${text.slice(0, Math.max(0, maxLength - 1)).trim()}…`;
}

function promoManifestUrl() {
  const value = document.querySelector('meta[name="g7-promo-manifest"]')?.content?.trim() || "";
  if (!value || ["off", "disabled", "none"].includes(value.toLowerCase())) {
    return null;
  }
  if (value.includes("__G7INST_PROMO_MANIFEST_URL__")) {
    return "/promo.sample.json";
  }

  return value;
}

function safePromoHref(value) {
  try {
    const url = new URL(String(value ?? "").trim(), window.location.href);
    if (!["http:", "https:"].includes(url.protocol)) {
      return null;
    }
    return url.href;
  } catch (_error) {
    return null;
  }
}

function normalizePromoSlot(slot) {
  if (!slot || typeof slot !== "object") {
    return null;
  }

  const href = safePromoHref(slot.href);
  const title = compactText(slot.title, 34);
  const body = compactText(slot.body, 88);
  if (!href || !title || !body) {
    return null;
  }

  const theme = ["default", "github", "pro"].includes(slot.theme) ? slot.theme : "default";
  return {
    id: compactText(slot.id || title, 48).replace(/[^a-zA-Z0-9_-]/g, "-"),
    title,
    body,
    href,
    theme,
    badge: compactText(slot.badge || "추천", 12),
    cta: compactText(slot.cta || "열기", 18),
  };
}

function promoManifestKey(manifest) {
  return `${manifest?.version ?? 1}:${manifest?.updated_at ?? ""}`;
}

function hidePromoPanel() {
  if (!nodes.promoPanel) {
    return;
  }
  nodes.promoPanel.hidden = true;
  nodes.promoPanel.innerHTML = "";
}

function renderPromoManifest(manifest) {
  if (!nodes.promoPanel) {
    return;
  }

  const slots = (Array.isArray(manifest?.slots) ? manifest.slots : [])
    .map(normalizePromoSlot)
    .filter(Boolean)
    .slice(0, 3);
  const manifestKey = promoManifestKey(manifest);
  if (!slots.length || localStorage.getItem(promoDismissStorageKey) === manifestKey) {
    hidePromoPanel();
    return;
  }

  nodes.promoPanel.dataset.promoKey = manifestKey;
  nodes.promoPanel.innerHTML = `
    <div class="promo-panel-heading">
      <span>추천 도구</span>
      <button type="button" data-promo-dismiss aria-label="추천 도구 숨기기">${iconMarkup("x")}</button>
    </div>
    <div class="promo-list">
      ${slots.map((slot) => `
        <a class="promo-card promo-card-${escapeHtml(slot.theme)}" href="${escapeHtml(slot.href)}" target="_blank" rel="noreferrer noopener">
          <span class="promo-badge">${escapeHtml(slot.badge)}</span>
          <strong>${escapeHtml(slot.title)}</strong>
          <p>${escapeHtml(slot.body)}</p>
          <span class="promo-cta">${escapeHtml(slot.cta)}</span>
        </a>
      `).join("")}
    </div>
  `;
  nodes.promoPanel.hidden = false;
}

async function loadPromoManifest() {
  const url = promoManifestUrl();
  if (!url) {
    hidePromoPanel();
    return;
  }

  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), 2500);
  try {
    const response = await fetch(url, {
      cache: "no-store",
      credentials: "same-origin",
      headers: { Accept: "application/json" },
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`promo manifest ${response.status}`);
    }
    renderPromoManifest(await response.json());
  } catch (_error) {
    hidePromoPanel();
  } finally {
    window.clearTimeout(timeoutId);
  }
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
    if (key === "site_password" || key === "site_password_confirm" || key === "database_password" || key === "database_password_confirm") {
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
      provisionActionResults: state.provisionActionResults,
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
    nodes.confirmSpecButton.disabled = !state.planReady || Boolean(sitePasswordError() || databaseError());
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
  if (stepOrder.includes(step)) {
    return step;
  }
  const path = String(step || "").split("?")[0];
  return routeToStep[path] || "login";
}

function stepUrl(step) {
  const route = stepRoutes[normalizedStep(step)] || stepRoutes.login;
  const query = step === "login" && window.location.search.includes("token=") ? window.location.search : "";
  return `${route}${query}`;
}

function stepFromLocation() {
  if (routeToStep[window.location.pathname]) {
    return routeToStep[window.location.pathname];
  }
  if (window.location.hash) {
    return normalizedStep(window.location.hash.replace("#", ""));
  }
  return "login";
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
        "계정 정보 확인 필요",
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
      "4단계에서 자동 생성된 설치 계획을 확인한 뒤 이 사양으로 진행 버튼을 누르세요.",
    );
    step = "plan";
  }

  if (["report", "provision"].includes(step) && !state.reportReady) {
    setAlert(
      nodes.installStatus,
      "warning",
      "설치 결과가 아직 없습니다",
      "기본 서버 구성을 완료해야 결과와 세부 설정을 볼 수 있습니다.",
    );
    step = "install";
  }

  const wasActiveStep = state.activeStep === step;

  state.activeStep = step;
  document.body.dataset.activeStep = step;
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
  refreshSummary();
  saveWizardState();
  if (state.activeStep === "plan") {
    void generatePlan({ auto: true });
  }
  if (state.activeStep === "provision") {
    renderProvisionPanel(currentReport());
  }
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
    database_name: form.get("database_name")?.trim() || "",
    database_user: form.get("database_user")?.trim() || "",
    database_password: form.get("database_password") || "",
    database_password_confirm: form.get("database_password_confirm") || "",
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
  return sitePasswordError(payload) || databaseError(payload);
}

const sitePasswordAlertMessages = [
  "사이트 계정 비밀번호를 입력하세요.",
  "사이트 계정 비밀번호 확인이 일치하지 않습니다.",
  "사이트 계정 비밀번호는 8자 이상이어야 합니다.",
  "사이트 계정 비밀번호에는 콜론, 줄바꿈, 제어문자를 사용할 수 없습니다.",
  "사이트 계정 비밀번호에 사용할 수 없는 문자가 있습니다.",
  "DB 이름을 입력하세요.",
  "DB 이름 형식이 올바르지 않습니다.",
  "DB 계정을 입력하세요.",
  "DB 계정 형식이 올바르지 않습니다.",
  "DB 비밀번호를 입력하세요.",
  "DB 비밀번호 확인이 일치하지 않습니다.",
  "DB 비밀번호는 8자 이상이어야 합니다.",
  "DB 비밀번호에 사용할 수 없는 문자가 있습니다.",
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

function isDatabaseIdentifier(value, maxLength) {
  return new RegExp(`^[A-Za-z_][A-Za-z0-9_]{0,${maxLength - 1}}$`).test(value || "");
}

function databaseError(payload = optionPayload()) {
  return databaseIdentifierError(payload) || databasePasswordError(payload);
}

function databaseIdentifierError(payload = optionPayload()) {
  if (!payload.database_name) {
    return "DB 이름을 입력하세요.";
  }
  if (!isDatabaseIdentifier(payload.database_name, 64)) {
    return "DB 이름 형식이 올바르지 않습니다. 영문 또는 밑줄로 시작하고 영문, 숫자, 밑줄만 사용하세요.";
  }
  if (!payload.database_user) {
    return "DB 계정을 입력하세요.";
  }
  if (!isDatabaseIdentifier(payload.database_user, 32)) {
    return "DB 계정 형식이 올바르지 않습니다. 영문 또는 밑줄로 시작하고 영문, 숫자, 밑줄만 사용하세요.";
  }
  return null;
}

function databasePasswordError(payload = optionPayload()) {
  if (!payload.database_password) {
    return "DB 비밀번호를 입력하세요.";
  }
  if (payload.database_password !== payload.database_password_confirm) {
    return "DB 비밀번호 확인이 일치하지 않습니다.";
  }
  if (payload.database_password.length < 8) {
    return "DB 비밀번호는 8자 이상이어야 합니다.";
  }
  if (/['\\\n\r\x00-\x1F\x7F]/.test(payload.database_password)) {
    return "DB 비밀번호에 사용할 수 없는 문자가 있습니다. 작은따옴표, 백슬래시, 줄바꿈, 제어문자는 사용할 수 없습니다.";
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
  const dbIdentifierError = databaseIdentifierError(payload);
  const dbPasswordError = databasePasswordError(payload);
  const dbError = dbIdentifierError || dbPasswordError;
  const combinedError = error || dbError;
  const hasInput = Boolean(payload.site_password || payload.site_password_confirm);
  const hasDbPasswordInput = Boolean(payload.database_password || payload.database_password_confirm);
  const shouldShowStatus = Boolean(options.show || hasInput || state.activeStep === "options");
  const shouldShowDbStatus = Boolean(options.show || dbIdentifierError || hasDbPasswordInput || state.activeStep === "options");
  const shouldShowDbPasswordStatus = Boolean(options.show || hasDbPasswordInput || state.activeStep === "options");

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
  [nodes.databaseName, nodes.databaseUser].forEach((node) => {
    if (!node) {
      return;
    }
    node.setCustomValidity(dbIdentifierError || "");
    node.classList.toggle("input-error", Boolean(dbIdentifierError && shouldShowDbStatus));
  });
  [nodes.databasePassword, nodes.databasePasswordConfirm].forEach((node) => {
    if (!node) {
      return;
    }
    node.setCustomValidity(dbPasswordError || "");
    node.classList.toggle("input-error", Boolean(dbPasswordError && shouldShowDbPasswordStatus));
  });
  if (nodes.databasePasswordStatus) {
    nodes.databasePasswordStatus.hidden = !shouldShowDbStatus;
    nodes.databasePasswordStatus.dataset.status = dbError ? "fail" : "pass";
    nodes.databasePasswordStatus.textContent = dbError || "DB 이름, 계정, 비밀번호 확인이 통과했습니다.";
  }
  nodes.optionsPlanButtons.forEach((button) => {
    button.disabled = Boolean(combinedError);
    button.title = combinedError || "";
    button.setAttribute("aria-disabled", combinedError ? "true" : "false");
  });
  refreshConfirmSpecButton();
  if (!combinedError) {
    clearSitePasswordAlerts();
  }

  return combinedError;
}

function setFormValue(name, value) {
  const field = nodes.optionsForm.elements[name];
  if (!field || value === null || value === undefined) {
    return;
  }

  field.value = value;
}

function databasePrefix(appPackage) {
  if (appPackage === "wordpress") {
    return "wp";
  }
  if (appPackage === "laravel") {
    return "laravel";
  }
  return "g7";
}

function derivedDatabaseName(domain, appPackage) {
  const prefix = databasePrefix(appPackage);
  const normalized = String(domain || "example.com")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 48 - prefix.length - 1);
  return `${prefix}_${normalized || "site"}`;
}

function derivedDatabaseUser(siteUser, appPackage) {
  const prefix = databasePrefix(appPackage);
  const normalizedSiteUser = String(siteUser || "g7").replace(/[^A-Za-z0-9_]+/g, "_");
  const value = normalizedSiteUser === "g7" ? `${prefix}_app` : `${prefix}_${normalizedSiteUser}`;
  return value.slice(0, 32);
}

function syncDatabaseDefaults() {
  const form = nodes.optionsForm;
  const domain = form.elements.domain?.value;
  const appPackage = form.elements.app_package?.value;
  const siteUser = form.elements.site_user?.value;
  const nextName = derivedDatabaseName(domain, appPackage);
  const nextUser = derivedDatabaseUser(siteUser, appPackage);

  if (nodes.databaseName && (!nodes.databaseName.value || nodes.databaseName.dataset.autoValue === nodes.databaseName.value)) {
    nodes.databaseName.value = nextName;
    nodes.databaseName.dataset.autoValue = nextName;
  }
  if (nodes.databaseUser && (!nodes.databaseUser.value || nodes.databaseUser.dataset.autoValue === nodes.databaseUser.value)) {
    nodes.databaseUser.value = nextUser;
    nodes.databaseUser.dataset.autoValue = nextUser;
  }
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

function syncFrankenPhpRuntime() {
  const webServer = nodes.optionsForm.elements.web_server?.value;
  const phpVersion = nodes.optionsForm.elements.php_version;
  if (webServer === "frankenphp" && phpVersion && phpVersion.value !== "8.5") {
    phpVersion.value = "8.5";
  }
}

function refreshFormState(options = {}) {
  const preservePlan = Boolean(options.preservePlan);
  const shouldPersist = options.persist !== false;

  if (!preservePlan) {
    state.planReport = null;
    state.planSignature = null;
    state.savedReportPayload = null;
    setPlanReady(false);
  }

  if (!preservePlan && !state.installRunning && !state.installCompleted) {
    setReportReady(false);
    renderPackageProgress([]);
    refreshInstallButtonState();
  }
  syncFrankenPhpRuntime();
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
    if (nodes.databaseVersion.value === "apt-default") {
      nodes.databaseVersion.value = "mysql-8.4";
    }
    nodes.databaseVersion.disabled = false;
  }

  syncDatabaseDefaults();
  refreshSecurityGuidance();
  refreshSitePasswordState();
  refreshSummary();
  if (shouldPersist) {
    saveWizardState();
  }
}

function refreshSummary() {
  if (nodes.summaryPanel) {
    const shouldShowSummary = Boolean(
      state.authenticated
        && !["login", "check"].includes(state.activeStep)
        && (state.doctorPassed || state.planReady || state.reportReady || state.installCompleted),
    );
    nodes.summaryPanel.hidden = !shouldShowSummary;
  }

  const payload = optionPayload();
  nodes.summaryDomain.textContent = payload.domain;
  nodes.summaryRuntime.textContent = `${runtimeLabel(payload.web_server)} / ${phpRuntimeLabel(payload.php_version, payload.php_source)}`;
  nodes.summaryData.textContent = `${databaseLabel(payload.database)} / Redis ${payload.redis === "enable" ? "사용" : "미사용"}`;
  nodes.summaryApp.textContent = appPackageLabel(payload.app_package);
}

function planRequestSignature(payload = optionPayload()) {
  const {
    site_password: _sitePassword,
    site_password_confirm: _sitePasswordConfirm,
    database_password: _databasePassword,
    database_password_confirm: _databasePasswordConfirm,
    ...safePayload
  } = payload;
  return JSON.stringify(safePayload);
}

function planPlaceholderHtml(title, message) {
  return `
    <section class="report-card">
      <h3>${escapeHtml(title)}</h3>
      <p class="mt-3 whitespace-pre-line text-sm text-base-content/60">${escapeHtml(message)}</p>
    </section>
  `;
}

function runtimeLabel(value) {
  if (value === "apache") {
    return "Apache";
  }
  if (value === "frankenphp") {
    return "FrankenPHP";
  }
  return "Nginx";
}

function webServiceName(value) {
  return value === "apache" ? "apache2" : "nginx";
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
      ? "설치기가 만든 계정, DB, 서비스, 웹루트/설정 파일, 패키지, 메타데이터를 정리합니다. 인증서는 보존합니다."
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
        <span>${escapeHtml(checkDisplayName(check.name))}</span>
        <p>${escapeHtml(check.message)}</p>
      </div>
      <strong>${escapeHtml(statusLabel[check.status] || check.status)}</strong>
    `;
    nodes.doctorResults.append(item);
  });
  renderRecoveryStatus(state.recoveryStatus);
  saveWizardState();
}

function auditDoctorReport(report) {
  const checks = Array.isArray(report?.checks) ? report.checks : [];
  const passed = checks.filter((check) => check.status === "pass").length;
  const failed = checks.filter((check) => check.status === "fail").length;
  const warned = checks.filter((check) => ["warn", "unknown"].includes(check.status)).length;
  log(`서버 점검 결과: 통과 ${passed}개, 주의 ${warned}개, 실패 ${failed}개`);
  checks.forEach((check) => {
    const label = checkDisplayName(check.name);
    const status = statusLabel[check.status] || check.status;
    log(`${label}: ${status} - ${localizeMessage(check.message)}`);
  });
}

async function runDoctorCheck() {
  hideAlert(nodes.doctorStatus);
  log("서버 점검 실행");
  const report = await apiFetch("/api/doctor");
  renderDoctor(report);
  await refreshRecoveryStatus();
  auditDoctorReport(report);
  log(`서버 점검 완료: ${report.install_allowed ? "설치 가능" : "설치 차단"}`);
  return report;
}

async function generatePlan(options = {}) {
  if (state.planGenerating) {
    return state.planReport;
  }

  const payload = optionPayload();
  const signature = planRequestSignature(payload);
  const auto = Boolean(options.auto);
  const force = Boolean(options.force);

  if (!force && state.planReady && state.planReport && state.planSignature === signature) {
    return state.planReport;
  }

  const passwordError = validateSitePassword(payload);
  if (passwordError) {
    setAlert(nodes.planStatus, "error", "계정 정보 확인 필요", passwordError);
    nodes.planOutput.innerHTML = planPlaceholderHtml(
      "사양 확인 필요",
      `${passwordError}\n이전으로 돌아가 사이트 계정과 DB 계정 정보를 다시 입력하세요.`,
    );
    setPlanReady(false);
    renderPackageProgress([]);
    saveWizardState();
    log(passwordError);
    return null;
  }

  state.planGenerating = true;
  hideAlert(nodes.planStatus);
  setPlanReady(false);
  renderPackageProgress([]);
  nodes.planOutput.innerHTML = planPlaceholderHtml(
    "설치 계획 생성 중",
    "선택한 사양으로 설치할 패키지, 생성 파일, 서비스 설정을 계산하고 있습니다.",
  );
  if (auto) {
    log("설치 계획 자동 생성");
  } else {
    log("설치 계획 새로고침");
  }

  try {
    const report = await apiFetch("/api/plan", {
      method: "POST",
      body: JSON.stringify(payload),
    });
    state.planReport = report;
    state.planSignature = signature;
    nodes.planOutput.innerHTML = renderPlanReport(report);
    const planPackages = flattenPlanPackages(report.packages);
    renderPackageProgress(planPackages);
    setAlert(
      nodes.planStatus,
      "success",
      "사양 확인 준비 완료",
      `${planPackages.length}개 apt 패키지와 ${report.files.length}개 파일 변경 계획을 정리했습니다. 맞으면 진행하고, 다르면 이전으로 돌아가 수정하세요.`,
    );
    setPlanReady(true);
    state.installCompleted = false;
    setReportReady(false);
    refreshInstallButtonState();
    saveWizardState();
    log(`설치 계획 준비 완료: apt 패키지 ${planPackages.length}개, 파일 변경 ${report.files.length}개`);
    return report;
  } catch (error) {
    state.planReport = null;
    state.planSignature = null;
    nodes.planOutput.innerHTML = planPlaceholderHtml("설치 계획 생성 실패", formatError(error));
    setAlert(nodes.planStatus, "error", "설치 계획 생성 실패", formatError(error));
    setPlanReady(false);
    renderPackageProgress([]);
    saveWizardState();
    log(formatError(error));
    return null;
  } finally {
    state.planGenerating = false;
  }
}

function markStage(stage, status) {
  const row = document.querySelector(`[data-stage="${stage}"]`);
  if (!row) {
    return;
  }

  row.dataset.status = status;
  row.querySelector("strong").textContent = status;
  const stageLabel = installStageLabels[stage] || row.querySelector("span")?.textContent || stage;
  if (status === "진행") {
    setActivityStatus(stageLabel, `${stageLabel} 진행 중입니다.`);
  } else if (status === "실패") {
    setActivityStatus(`${stageLabel} 실패`, "중단된 단계의 실패 항목과 리포트를 확인하세요.", 100);
  } else if (status === "성공") {
    setActivityStatus(stageLabel, `${stageLabel} 완료`);
  }
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
      description: packageDescription(name, packageGroup.description),
    })));
}

function packagePurpose(name) {
  const packageName = String(name || "");
  const exact = {
    nginx: "Nginx 웹서버가 도메인 요청을 앱으로 전달합니다.",
    apache2: "Apache 웹서버가 도메인 요청을 앱으로 전달합니다.",
    "g7-frankenphp": "FrankenPHP가 로컬 앱서버로 PHP 요청을 처리합니다.",
    "mysql-server": "MySQL이 사이트 데이터를 저장합니다.",
    "mariadb-server": "MariaDB가 사이트 데이터를 저장합니다.",
    curl: "원격 파일 다운로드에 사용합니다.",
    unzip: "다운로드한 압축 파일을 해제합니다.",
    "ca-certificates": "HTTPS 인증서 검증에 필요합니다.",
    git: "앱 소스를 Git 저장소에서 내려받습니다.",
    composer: "PHP 의존성을 설치합니다.",
    nodejs: "프론트엔드 빌드 런타임입니다.",
    npm: "프론트엔드 패키지를 설치하고 빌드합니다.",
    "software-properties-common": "추가 apt 저장소를 등록합니다.",
    "lsb-release": "Ubuntu 버전 정보를 확인합니다.",
    certbot: "Let's Encrypt SSL 인증서를 발급하고 갱신합니다.",
    "python3-certbot-nginx": "Nginx와 Certbot 인증서 작업을 연결합니다.",
    "python3-certbot-apache": "Apache와 Certbot 인증서 작업을 연결합니다.",
    "redis-server": "캐시, 세션, 큐를 처리하는 Redis 서버입니다.",
    postfix: "서버에서 알림 메일을 발송합니다.",
    mailutils: "메일 발송 테스트 도구입니다.",
  };

  if (exact[packageName]) {
    return exact[packageName];
  }
  if (/^php\d+\.\d+-fpm$/.test(packageName)) {
    return "PHP-FPM이 PHP 앱 실행을 담당합니다.";
  }
  if (/^php\d+\.\d+-cli$/.test(packageName)) {
    return "Composer와 Artisan 실행에 필요한 PHP CLI입니다.";
  }
  if (/^php\d+\.\d+-mysql$/.test(packageName)) {
    return "PHP에서 MySQL/MariaDB에 접속합니다.";
  }
  if (/^php\d+\.\d+-mbstring$/.test(packageName)) {
    return "한글과 다국어 문자열 처리를 지원합니다.";
  }
  if (/^php\d+\.\d+-xml$/.test(packageName)) {
    return "XML과 DOM 처리를 지원합니다.";
  }
  if (/^php\d+\.\d+-curl$/.test(packageName)) {
    return "PHP에서 외부 HTTP 요청을 처리합니다.";
  }
  if (/^php\d+\.\d+-gd$/.test(packageName)) {
    return "이미지 업로드와 썸네일 처리를 지원합니다.";
  }
  if (/^php\d+\.\d+-zip$/.test(packageName)) {
    return "PHP에서 압축 파일을 처리합니다.";
  }
  if (/^php\d+\.\d+-intl$/.test(packageName)) {
    return "다국어와 지역화 처리를 지원합니다.";
  }
  if (/^php\d+\.\d+-bcmath$/.test(packageName)) {
    return "정밀 숫자 계산을 지원합니다.";
  }
  if (/^php\d+\.\d+-imagick$/.test(packageName)) {
    return "고급 이미지 처리와 썸네일 생성을 지원합니다.";
  }
  if (/^php\d+\.\d+-redis$/.test(packageName)) {
    return "PHP 앱이 Redis에 접속합니다.";
  }

  return "";
}

function packageDescription(name, fallback = "") {
  return packagePurpose(name) || localizeMessage(fallback) || "설치 예정 패키지입니다.";
}

function isPackageLikeCheck(name, message = "") {
  const packageName = String(name || "");
  return Boolean(packagePurpose(packageName))
    || /^php\d+\.\d+-/.test(packageName)
    || String(message || "").startsWith("package ");
}

function packageStatusMessage(check) {
  const purpose = packageDescription(check?.name);
  const status = check?.status || "";
  const message = String(check?.message || "");

  if (status === "pass" || message === "package installed" || message === "패키지 설치 확인 완료") {
    return `${purpose} 패키지 설치와 검증이 완료되었습니다.`;
  }
  if (
    status === "installed"
    || message === "package was already installed before G7 installer ran"
    || message === "설치 전부터 있던 패키지입니다. 그대로 사용합니다."
  ) {
    return `${purpose} 이미 서버에 있어 그대로 사용합니다.`;
  }
  if (
    status === "not-installed"
    || message === "package was absent before G7 installer ran"
    || message === "설치 전에는 없던 패키지입니다. 이번 설치 대상입니다."
  ) {
    return `${purpose} 이번 설치에서 새로 준비합니다.`;
  }
  if (
    message === "package candidate is available from configured apt sources"
    || message === "apt 저장소에서 설치 후보를 확인했습니다."
  ) {
    return `${purpose} apt 저장소에서 설치 가능함을 확인했습니다.`;
  }
  if (
    message === "package candidate is not available from configured apt sources"
    || message === "현재 apt 저장소에서 설치 후보를 찾지 못했습니다."
  ) {
    return `${purpose} 현재 apt 저장소에서 찾지 못했습니다.`;
  }
  if (status === "fail" || message === "package is not installed" || message === "패키지가 설치되지 않았습니다.") {
    return `${purpose} 설치 확인에 실패했습니다.`;
  }
  if (
    status === "unknown"
    || message === "package preinstall state is unknown"
    || message === "package status is unknown"
    || message === "설치 전 패키지 상태를 확인하지 못했습니다."
    || message === "패키지 상태를 확인하지 못했습니다."
  ) {
    return `${purpose} 설치 상태를 확인하지 못했습니다.`;
  }

  return purpose;
}

function renderPackageProgress(packages) {
  state.planPackages = packages;
  stopPackageTicker();

  if (!nodes.packageProgressList) {
    return;
  }

  if (!packages.length) {
    if (nodes.packageProgressHelp) {
      nodes.packageProgressHelp.textContent = "설치 사양 확정 후 apt 패키지별 진행 상태를 표시합니다.";
    }
    nodes.packageProgressList.innerHTML = `<div class="empty-state">설치 사양 확정 후 패키지 목록이 표시됩니다.</div>`;
    return;
  }

  if (nodes.packageProgressHelp) {
    nodes.packageProgressHelp.textContent = `총 ${packages.length}개 apt 패키지를 설치 또는 검증합니다. 각 패키지의 진행률과 결과를 따로 표시합니다.`;
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
  updatePackageProgress(
    state.planPackages[index].name,
    "설치 중",
    5,
    `${packageDescription(state.planPackages[index].name)} 설치를 준비하고 있습니다.`,
  );

  state.packageTicker = window.setInterval(() => {
    if (!state.installRunning) {
      stopPackageTicker();
      return;
    }

    const packageItem = state.planPackages[index];
    percent = Math.min(95, percent + 10);
    updatePackageProgress(packageItem.name, "설치 중", percent, `${packageDescription(packageItem.name)} 설치 또는 검증을 진행 중입니다.`);

    if (percent >= 95 && index < state.planPackages.length - 1) {
      updatePackageProgress(
        packageItem.name,
        "검증 대기",
        100,
        `${packageDescription(packageItem.name)} 최종 검증 결과를 기다리고 있습니다.`,
      );
      index += 1;
      percent = 5;
      updatePackageProgress(
        state.planPackages[index].name,
        "설치 중",
        percent,
        `${packageDescription(state.planPackages[index].name)} 설치를 준비하고 있습니다.`,
      );
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
      check.status === "pass" ? "설치 완료" : "실패",
      100,
      packageStatusMessage(check),
    );
  });
}

function completePendingPackageProgress() {
  stopPackageTicker();
  document.querySelectorAll("[data-package]").forEach((row) => {
    const progress = row.querySelector("progress");
    if (!progress || Number(progress.value) >= 100) {
      return;
    }
    const packageName = row.dataset.package || "";
    updatePackageProgress(
      packageName,
      "검증 완료",
      100,
      `서버 설치 완료 리포트에서 ${packageName} 패키지 최종 상태를 확인했습니다.`,
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
  const percent = failed ? 100 : Math.round((done / rows.length) * 100);
  nodes.installProgress.value = percent;
  if (nodes.activityProgressLabel) {
    nodes.activityProgressLabel.textContent = `${percent}%`;
  }
}

function setProgress(node, percent) {
  if (!node) {
    return;
  }
  const value = Math.max(0, Math.min(100, Number(percent) || 0));
  node.value = value;
  if (node === nodes.installProgress && nodes.activityProgressLabel) {
    nodes.activityProgressLabel.textContent = `${Math.round(value)}%`;
  }
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
    setActivityStatus(null, payload.message || "", percent);
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
    description: packageStatusMessage(check),
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
    database_name: report.database_name,
    database_user: report.database_user,
    site_user: report.site_user,
    web_root: report.web_root,
    security_profile: report.security_profile,
    ssh_policy: report.ssh_policy,
  });
  refreshFormState({ preservePlan: true, persist: false });
}

function isInstallCompleted(report) {
  return report?.phase === "completed";
}

function checksHaveFailure(checks) {
  return Array.isArray(checks) && checks.some((check) => check.status === "fail");
}

function failedStageFromReport(report) {
  if (!report || isInstallCompleted(report)) {
    return null;
  }
  if (report.phase === "package-failed") {
    return "packages";
  }
  if (report.phase === "vhost-failed" || checksHaveFailure(report.vhost_checks)) {
    return "vhost";
  }
  if (checksHaveFailure(report.runtime_checks)) {
    return "runtime";
  }
  if (checksHaveFailure(report.database_checks)) {
    return "database";
  }
  if (checksHaveFailure(report.certbot_checks) || String(report.problem || "").includes("TLS")) {
    return "ssl";
  }
  if (checksHaveFailure(report.app_checks)) {
    return "app";
  }
  if (report.phase === "prepared") {
    return "packages";
  }
  return null;
}

function applyInstallStagesFromReport(report) {
  installStageOrder.forEach((stage) => markStage(stage, "대기"));
  const completed = phaseCompletedStages[report?.phase] || [];
  completed.forEach((stage) => markStage(stage, "성공"));
  const failedStage = failedStageFromReport(report);
  if (failedStage) {
    markStage(failedStage, "실패");
  } else if (isInstallCompleted(report)) {
    markStage("report", "성공");
  }
  setProgress(nodes.installProgress, isInstallCompleted(report) || failedStage ? 100 : Math.round((completed.length / installStageOrder.length) * 100));
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
    if (isInstallCompleted(report)) {
      completePendingPackageProgress();
    }
  }

  applyInstallStagesFromReport(report);
  state.installCompleted = isInstallCompleted(report);
  setReportReady(true);
  refreshInstallButtonState(state.installCompleted ? null : "복구 후 다시 시도");
}

function renderInstallReport(report) {
  const completed = isInstallCompleted(report);
  const link = completed && report.app_url ? urlLink(report.app_url) : accessLink(report.domain, report.phase);
  const title = completed ? "서버 세팅 및 앱 배치 완료" : "설치 중단 리포트";
  const note = completed
    ? link.hint
    : (report.problem || "아직 모든 서버 세팅이 끝나지 않았습니다. 실패 항목을 해결한 뒤 초기화 또는 재시도를 진행하세요.");
  nodes.reportOutput.innerHTML = [
    reportSummaryCard(title, [
      ["도메인", report.domain],
      ["웹앱 링크", link.html],
      ["웹서버 / PHP", `${runtimeLabel(report.web_server)} / ${phpRuntimeLabel(report.php_version, report.php_source)}`],
      ["데이터베이스", `${databaseLabel(report.database)} (${databaseVersionLabel(report.database_version)})`],
      ["DB 이름", report.database_name || "-"],
      ["DB 계정", report.database_user || "-"],
      ["DB 비밀번호", report.database_password_policy === "user-provided-store-root-only" ? "사용자 입력값 저장" : "무작위 생성"],
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
      ["소유 파일 수", ownedFileCountLabel(report)],
      ["설정 안내서", report.setup_guide_path || "-"],
      ["복구 매니페스트", report.backup_manifest_path || "-"],
    ], note),
    completionStateCard(report),
    reportDownloadCard(report, "방금 생성됨"),
    operationsGuideCard(report, link),
    healthChecklistCard(report),
    listCard("완료된 작업", report.completed_steps),
    compactListCard("다음 단계", [
      "7단계 세부 설정에서 웹서버, PHP 런타임, DB, SSL, 메일, 웹앱 카드를 항목별로 열어 확인합니다.",
      "각 카드는 설정값, 관련 파일, 실행/재시작 명령, 검증 결과를 따로 보여줍니다.",
    ]),
    report.problem ? listCard("중단 원인", [report.problem]) : "",
  ].join("");

  hydrateIcons(nodes.reportOutput);
  applyPackageChecks(report.package_checks);
  if (completed) {
    completePendingPackageProgress();
  }
  applyInstallStagesFromReport(report);
  state.installCompleted = completed;
  setReportReady(true);
  refreshInstallButtonState(completed ? null : "복구 후 다시 시도");
  state.savedReportPayload = {
    exists: true,
    path: "방금 생성됨",
    content: JSON.stringify(report),
  };
  renderProvisionPanel(report);
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

  const completed = isInstallCompleted(report);
  const link = completed && report.app_url ? urlLink(report.app_url) : accessLink(report.domain || "example.com", report.phase);
  const title = completed ? "저장된 완료 리포트" : "저장된 중단 리포트";
  const note = completed
    ? link.hint
    : (report.problem || "아직 모든 서버 세팅이 끝나지 않았습니다. 실패 항목을 해결한 뒤 초기화 또는 재시도를 진행하세요.");
  nodes.reportOutput.innerHTML = [
    reportSummaryCard(title, [
      ["리포트 파일", payload.path],
      ["도메인", report.domain || "-"],
      ["웹앱 링크", link.html],
      ["단계", report.phase || "-"],
      ["웹서버 / PHP", `${runtimeLabel(report.web_server)} / ${phpRuntimeLabel(report.php_version, report.php_source)}`],
      ["데이터베이스", databaseLabel(report.database)],
      ["DB 이름", report.database_name || "-"],
      ["DB 계정", report.database_user || "-"],
      ["DB 비밀번호", report.database_password_policy === "user-provided-store-root-only" ? "사용자 입력값 저장" : "무작위 생성"],
      ["앱 패키지", appPackageLabel(report.app_package || report.app_profile)],
      ["앱 문서 루트", report.app_document_root || "-"],
      ["사이트 계정", report.site_user || "-"],
      ["웹루트", report.web_root || "-"],
      ["메일", mailModeLabel(report.mail_mode)],
      ["SMTP 서버", report.smtp_host || "-"],
      ["DNS/IP 확인", report.dns_check ? "수행" : "건너뜀"],
      ["설정 안내서", report.setup_guide_path || "-"],
      ["복구 매니페스트", report.backup_manifest_path || "-"],
      ["소유 파일 수", ownedFileCountLabel(report)],
    ], note),
    completionStateCard(report),
    reportDownloadCard(report, payload.path),
    operationsGuideCard(report, link),
    healthChecklistCard(report),
    compactListCard("다음 단계", [
      "7단계 세부 설정에서 웹서버, PHP 런타임, DB, SSL, 메일, 웹앱 카드를 항목별로 열어 확인합니다.",
      "각 카드는 설정값, 관련 파일, 실행/재시작 명령, 검증 결과를 따로 보여줍니다.",
    ]),
    report.problem ? listCard("문제", [report.problem]) : "",
  ].join("");
  hydrateIcons(nodes.reportOutput);
  setReportReady(true);
  restoreInstallStateFromReport(report);
  renderProvisionPanel(report);
  saveWizardState();
}

function renderResetReport(report) {
  nodes.reportOutput.innerHTML = [
    reportSummaryCard("재설치 초기화 완료", [
      ["미리보기", report.dry_run ? "예" : "아니오"],
      ["의미", "설치기가 만든 계정, DB, 서비스, 웹루트/설정 파일, 패키지, 메타데이터를 정리했습니다. Let's Encrypt 인증서는 보존했습니다."],
    ]),
    actionCard("리소스 처리", report.actions),
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
      <dd>${reportValueHtml(value)}</dd>
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

function reportValueHtml(value) {
  const text = String(value ?? "-");
  if (text.startsWith("<a ") || text.startsWith("<span ")) {
    return text;
  }
  return escapeHtml(text);
}

function ownedFileCountLabel(report = {}) {
  return Array.isArray(report.owned_files) ? `${report.owned_files.length}개` : "-";
}

function completedCheckNames(checks = []) {
  return new Set((Array.isArray(checks) ? checks : [])
    .filter((check) => check.status === "pass")
    .map((check) => check.name));
}

function completionStateRows(report = {}) {
  const appChecks = completedCheckNames(report.app_checks);
  const completed = isInstallCompleted(report);
  const appProfile = report.app_profile || report.app_package;
  const appInstallVerified = appProfile === "gnuboard7"
    ? appChecks.has("g7-install-lock")
    : completed;

  return [
    {
      label: "서버 준비",
      status: completed ? "pass" : "fail",
      message: completed
        ? "웹서버, PHP 런타임, DB, SSL, 앱 파일 배치가 완료되었습니다."
        : "중단 단계가 있어 서버 준비가 끝나지 않았습니다.",
    },
    {
      label: "앱 설치 준비",
      status: completed && report.app_url ? "pass" : "warn",
      message: completed && report.app_url
        ? `${appPackageLabel(appProfile)} 설치/준비 링크가 생성되었습니다.`
        : "앱 설치 링크는 기본 구성이 끝나야 표시됩니다.",
    },
    {
      label: "실제 앱 설치 검증",
      status: appInstallVerified ? "pass" : "manual",
      message: appInstallVerified
        ? "앱 설치 완료 신호를 확인했습니다."
        : "브라우저 설치 화면 완료 후 7단계 웹앱 카드에서 다시 확인하세요.",
    },
  ];
}

function completionStateCard(report = {}) {
  return `
    <section class="report-card">
      <h3>설치 완료 상태</h3>
      <p class="mt-2 text-sm text-base-content/60">완료는 서버 준비, 앱 설치 준비, 실제 앱 설치 검증을 분리해서 판단합니다.</p>
      <div class="result-list mt-3">
        ${completionStateRows(report).map((row) => `
          <div class="result-row" data-status="${escapeHtml(checkStatus(row.status))}">
            <div class="result-copy">
              <span>${escapeHtml(row.label)}</span>
              <p>${escapeHtml(row.message)}</p>
            </div>
            <strong>${escapeHtml(statusLabel[row.status] || row.status)}</strong>
          </div>
        `).join("")}
      </div>
    </section>
  `;
}

function reportDownloadCard(report = {}, payloadPath = "") {
  return `
    <section class="report-card">
      <h3>리포트 저장</h3>
      <p class="mt-2 text-sm text-base-content/60">비밀번호 원문은 포함하지 않고, 설치 결과와 확인 경로만 저장합니다.</p>
      <div class="download-actions mt-4">
        <button class="btn btn-sm btn-outline icon-button" type="button" data-icon="download" data-download-report="json">리포트 JSON</button>
        <button class="btn btn-sm btn-outline icon-button" type="button" data-icon="download" data-download-report="md">설정 안내서 MD</button>
        <button class="btn btn-sm btn-outline icon-button" type="button" data-icon="download" data-download-report="summary">요약 TXT</button>
      </div>
      <dl class="mt-4">
        <div><dt>서버 리포트</dt><dd>${escapeHtml(payloadPath || "/var/log/g7-installer/report.json")}</dd></div>
        <div><dt>설정 안내서</dt><dd>${escapeHtml(report.setup_guide_path || "/var/log/g7-installer/setup-guide.md")}</dd></div>
        <div><dt>복구 매니페스트</dt><dd>${escapeHtml(report.backup_manifest_path || "/var/backups/g7-installer/manifest.json")}</dd></div>
      </dl>
    </section>
  `;
}

function reportSummaryText(report = {}) {
  return [
    `도메인: ${report.domain || "-"}`,
    `상태: ${report.phase || "-"}`,
    `앱: ${appPackageLabel(report.app_profile || report.app_package)}`,
    `웹서버/PHP: ${runtimeLabel(report.web_server)} / ${phpRuntimeLabel(report.php_version, report.php_source)}`,
    `DB: ${databaseLabel(report.database)} / ${report.database_name || "-"}`,
    `웹루트: ${report.web_root || "-"}`,
    `문서 루트: ${report.app_document_root || "-"}`,
    `앱 링크: ${report.app_url || "-"}`,
    `설정 안내서: ${report.setup_guide_path || "-"}`,
    `복구 매니페스트: ${report.backup_manifest_path || "-"}`,
  ].join("\n");
}

function setupGuideMarkdown(report = {}) {
  const webService = webServiceName(report.web_server);
  const runtimeRows = report.web_server === "frankenphp"
    ? ["- FrankenPHP: sudo systemctl restart g7-frankenphp"]
    : [`- PHP-FPM: sudo systemctl restart php${report.php_version || "8.5"}-fpm`];
  return [
    `# G7 Installer 설치 요약 - ${report.domain || "unknown"}`,
    "",
    "## 완료 상태",
    ...completionStateRows(report).map((row) => `- ${row.label}: ${statusLabel[row.status] || row.status} - ${row.message}`),
    "",
    "## 주요 경로",
    `- 웹루트: ${report.web_root || "-"}`,
    `- 앱 문서 루트: ${report.app_document_root || "-"}`,
    `- 설정 안내서: ${report.setup_guide_path || "-"}`,
    `- 리포트 JSON: ${report.state_path ? "/var/log/g7-installer/report.json" : "-"}`,
    `- 복구 매니페스트: ${report.backup_manifest_path || "-"}`,
    "",
    "## 서비스 명령",
    `- 웹서버: sudo systemctl reload ${webService}`,
    ...runtimeRows,
    `- DB: sudo systemctl restart ${report.database === "mariadb" ? "mariadb" : "mysql"}`,
    "- SSL 갱신 확인: sudo certbot renew --dry-run --no-random-sleep-on-renew",
    "",
  ].join("\n");
}

function safeFilenamePart(value) {
  return String(value || "g7-installer").replace(/[^a-zA-Z0-9._-]/g, "-");
}

function downloadTextFile(filename, mime, content) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

function downloadReport(format) {
  const report = currentReport();
  if (!report || typeof report !== "object") {
    setAlert(nodes.reportStatus, "warning", "저장할 리포트 없음", "먼저 리포트를 새로고침하세요.");
    return;
  }

  const domain = safeFilenamePart(report.domain);
  if (format === "md") {
    downloadTextFile(`${domain}-setup-guide.md`, "text/markdown;charset=utf-8", setupGuideMarkdown(report));
  } else if (format === "summary") {
    downloadTextFile(`${domain}-install-summary.txt`, "text/plain;charset=utf-8", reportSummaryText(report));
  } else {
    downloadTextFile(`${domain}-report.json`, "application/json;charset=utf-8", `${JSON.stringify(report, null, 2)}\n`);
  }
  log(`리포트 저장: ${format}`);
}

function operationsGuideCard(report = {}, link = null) {
  const webService = webServiceName(report.web_server);
  const fpmService = report.php_version ? `php${report.php_version}-fpm` : "php-fpm";
  const dbService = report.database === "mariadb" ? "mariadb" : "mysql";
  const appLink = link?.html || accessLink(report.domain || "example.com", report.phase).html;
  const runtimeRow = report.web_server === "frankenphp"
    ? ["FrankenPHP 재시작", "sudo systemctl restart g7-frankenphp"]
    : ["PHP-FPM 재시작", `sudo systemctl restart ${fpmService}`];

  return reportSummaryCard("운영 접속/명령", [
    ["웹앱", appLink],
    ["웹서버 재시작", `sudo systemctl reload ${webService}`],
    runtimeRow,
    ["DB 재시작", `sudo systemctl restart ${dbService}`],
    ["SSL 갱신 점검", `sudo certbot renew --dry-run --cert-name ${report.domain || "도메인"}`],
    ["설정 안내서", report.setup_guide_path || "-"],
    ["복구 매니페스트", report.backup_manifest_path || "-"],
  ], "7단계 세부 설정에서 항목별 실행/검증 결과를 다시 확인합니다.");
}

function healthChecklistCard(report = {}) {
  const appPackage = report.app_package || report.app_profile;
  const rows = [
    "웹서버 vhost 설정 테스트와 reload 결과 확인",
    report.web_server === "frankenphp"
      ? "FrankenPHP 서비스 상태, 127.0.0.1:7080 로컬 앱서버, 업로드 용량 확인"
      : "PHP-FPM pool 사용자, 업로드 용량, 쓰기 경로 권한 확인",
    "DB 이름/계정/비밀번호 보관 위치 확인",
    "SSL 인증서 파일과 certbot.timer 갱신 상태 확인",
  ];

  if (appPackage === "gnuboard7") {
    rows.push("G7 core 빌드 산출물, /install 잠금, CKEditor 업로드 제한을 웹앱 카드에서 확인");
  } else {
    rows.push("웹앱 설치 화면과 쓰기 경로 권한을 웹앱 카드에서 확인");
  }

  return compactListCard("최종 검증 체크리스트", rows);
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

function currentReport() {
  return parseSavedReport(state.savedReportPayload);
}

function configFile(path, purpose, sensitive = false) {
  return {
    path,
    purpose,
    sensitive,
  };
}

function serverNamesPreview(domain, wwwMode = "include") {
  const root = domain || "example.com";
  if (wwwMode === "none") {
    return root;
  }
  if (wwwMode === "redirect-to-www") {
    return `${root} www.${root}`;
  }
  if (wwwMode === "redirect-to-root" || wwwMode === "include") {
    return `${root} www.${root}`;
  }
  return root;
}

function maskSensitiveText(value) {
  return String(value ?? "")
    .replace(/((?:password|passwd|secret|token|private_key|smtp_password|db_password|DB_PASSWORD|SMTP_PASSWORD)\s*[:=]\s*)[^\s"'`]+/gi, "$1******")
    .replace(/(privkey\.pem)/gi, "$1 (내용 비공개)")
    .replace(/(-----BEGIN [^-]+PRIVATE KEY-----)[\s\S]*(-----END [^-]+PRIVATE KEY-----)/gi, "$1\n******\n$2");
}

function configFilesHtml(files = []) {
  const rows = Array.isArray(files) ? files.filter((file) => file?.path) : [];
  if (!rows.length) {
    return `<div class="empty-state">이 항목에서 직접 표시할 설정 파일은 없습니다.</div>`;
  }

  return `
    <div class="config-file-list">
      ${rows.map((file) => `
        <div class="config-file-card" data-sensitive="${file.sensitive ? "true" : "false"}">
          <div>
            <strong>${escapeHtml(file.purpose || "설정 파일")}</strong>
            <span>${file.sensitive ? "민감정보 마스킹" : "읽기 전용 확인"}</span>
          </div>
          <code>${escapeHtml(file.path)}</code>
        </div>
      `).join("")}
    </div>
  `;
}

function configPreviewHtml(preview = []) {
  const lines = Array.isArray(preview) ? preview : [];
  if (!lines.length) {
    return `<pre class="config-preview">표시할 설정 요약이 없습니다.</pre>`;
  }

  return `<pre class="config-preview">${escapeHtml(lines.map(maskSensitiveText).join("\n"))}</pre>`;
}

function renderProvisionPanel(report = currentReport()) {
  if (!nodes.provisionOutput) {
    return;
  }
  if (!report || typeof report !== "object") {
    nodes.provisionOutput.innerHTML = `<div class="empty-state">6단계 결과 리포트가 생성되면 세부 설정 카드가 표시됩니다.</div>`;
    return;
  }

  const actions = provisioningActions(report);
  nodes.provisionOutput.innerHTML = `
    <section class="report-card provisioning-actions">
      <div class="provisioning-actions-heading">
        <div>
          <h3>세부 설정 카드</h3>
          <p>설정값을 항목별로 확인하고 승인 후 적용/재시작/검증을 실행합니다.</p>
        </div>
        <strong data-status="${escapeHtml(overallProvisioningStatus(actions))}">${escapeHtml(overallProvisioningLabel(actions))}</strong>
      </div>
      <div class="provisioning-action-grid">
        ${actions.map((action) => `
          <article class="provisioning-action-card" data-status="${escapeHtml(action.status)}">
            <div>
              <span>${escapeHtml(action.title)}</span>
              <strong>${escapeHtml(action.label)}</strong>
            </div>
            <p>${escapeHtml(action.summary)}</p>
            <code>${escapeHtml(action.command)}</code>
            <div class="provisioning-action-buttons">
              <button class="btn btn-sm btn-primary icon-button" type="button" data-icon="scan-line" data-provision-open="${escapeHtml(action.key)}">설정 파일/값 확인</button>
            </div>
          </article>
        `).join("")}
      </div>
    </section>
  `;
  hydrateIcons(nodes.provisionOutput);
}

function provisioningActions(report = {}) {
  const isFranken = report.web_server === "frankenphp";
  const webService = webServiceName(report.web_server);
  const webCheck = report.web_server === "apache" ? "sudo apache2ctl configtest" : "sudo nginx -t";
  const fpmService = report.php_version ? `php${report.php_version}-fpm` : "php-fpm";
  const dbService = report.database === "mariadb" ? "mariadb" : "mysql";
  const appRoot = report.web_root || "/home/사이트계정/public_html";
  const appUrl = report.app_url || accessLink(report.domain || "example.com", report.phase).html.replace(/<[^>]+>/g, "");
  const selectedApp = report.app_profile || report.app_package;
  const mailSkipped = report.mail_mode === "none";
  const certName = report.domain || "example.com";
  const phpVersion = report.php_version || "8.5";
  const webConfigPath = report.web_server === "apache" ? "/etc/apache2/sites-available/g7.conf" : "/etc/nginx/sites-available/g7.conf";
  const phpPoolPath = `/etc/php/${phpVersion}/fpm/pool.d/g7.conf`;
  const phpSapi = isFranken ? "cli" : "fpm";
  const phpIniPath = `/etc/php/${phpVersion}/${phpSapi}/php.ini`;
  const secretsPath = "/etc/g7-installer/secrets.toml";

  return [
    provisioningAction("webserver", "웹서버/vhost", report.vhost_checks, {
      summary: isFranken
        ? "도메인 요청을 Nginx edge에서 FrankenPHP 로컬 앱서버로 연결합니다."
        : "도메인 요청을 앱 문서 루트와 PHP-FPM으로 연결합니다.",
      command: `${webCheck} && sudo systemctl reload ${webService}`,
      settings: [
        ["도메인", report.domain || "-"],
        ["문서 루트", report.app_document_root || "-"],
        ["웹루트", report.web_root || "-"],
        ["설정 파일", webConfigPath],
        ["서비스", webService],
      ],
      files: [
        configFile(webConfigPath, isFranken ? "도메인 vhost, 문서 루트, FrankenPHP proxy 연결" : "도메인 vhost, 문서 루트, PHP-FPM 연결"),
        configFile(report.web_server === "apache" ? "/etc/apache2/sites-enabled/g7.conf" : "/etc/nginx/sites-enabled/g7.conf", "활성화된 vhost 링크/엔트리"),
        ...(isFranken ? [configFile("/etc/systemd/system/g7-frankenphp.service", "FrankenPHP 로컬 앱서버 systemd unit")] : []),
      ],
      preview: [
        `server_name ${serverNamesPreview(report.domain, report.www_mode)};`,
        `root ${report.app_document_root || "-"};`,
        isFranken ? "proxy_pass http://127.0.0.1:7080" : `php_socket /run/php/php${phpVersion}-fpm.sock`,
        "security_headers nosniff / frame policy / referrer policy",
      ],
    }),
    provisioningAction("php", isFranken ? "FrankenPHP / PHP 요약" : "PHP-FPM / phpinfo 요약", report.runtime_checks, {
      summary: isFranken
        ? "FrankenPHP 서비스, CLI ini 기준 PHP 정보, 필수 확장과 업로드 한도를 앱 설치 전에 확인합니다."
        : "사이트 계정 PHP 풀, FPM ini 기준 PHP 정보, 필수 확장과 업로드 한도를 앱 설치 전에 확인합니다.",
      command: isFranken
        ? `sudo systemctl restart g7-frankenphp && env PHP_INI_SCAN_DIR=/etc/php/${phpVersion}/cli/conf.d php${phpVersion} -c ${phpIniPath} -i`
        : `sudo systemctl restart ${fpmService} && env PHP_INI_SCAN_DIR=/etc/php/${phpVersion}/fpm/conf.d php${phpVersion} -c ${phpIniPath} -i`,
      settings: [
        ["PHP", phpRuntimeLabel(report.php_version, report.php_source)],
        [isFranken ? "서비스 사용자" : "Pool 사용자", report.site_user || "-"],
        [isFranken ? "앱서버" : "Pool 설정", isFranken ? "127.0.0.1:7080" : phpPoolPath],
        ["기준 php.ini", phpIniPath],
        ["추가 ini", `/etc/php/${phpVersion}/${phpSapi}/conf.d/99-g7-installer.ini`],
        ["서비스", isFranken ? "g7-frankenphp" : fpmService],
      ],
      files: [
        ...(isFranken ? [configFile("/etc/systemd/system/g7-frankenphp.service", "FrankenPHP 로컬 앱서버 unit")] : [configFile(phpPoolPath, "사이트 계정 전용 PHP-FPM pool")]),
        configFile(`/etc/php/${phpVersion}/${phpSapi}/conf.d/99-g7-installer.ini`, "업로드/메모리/opcache 주요 런타임 값"),
        configFile(phpIniPath, isFranken ? "PHP CLI 기준 php.ini" : "PHP-FPM 기준 php.ini"),
      ],
      preview: [
        isFranken ? "ExecStart=/opt/g7-frankenphp/frankenphp php-server --listen 127.0.0.1:7080" : `[g7-${report.site_user || "site"}]`,
        isFranken ? `User=${report.site_user || "-"}` : `user = ${report.site_user || "-"}`,
        "group = www-data",
        isFranken ? "public endpoint = Nginx 80/443" : "pm = dynamic",
        isFranken ? "phpinfo-summary = CLI ini 기준 PHP_VERSION / loaded_ini / scan_dir" : "phpinfo-summary = FPM ini 기준 PHP_VERSION / loaded_ini / scan_dir",
        "memory_limit / upload_max_filesize / post_max_size = 서버 RAM 기준 검증",
        "required_extensions = 앱 종류 + Redis 선택값 기준 검증",
      ],
    }),
    provisioningAction("database", "데이터베이스", report.database_checks, {
      summary: "앱 전용 DB와 DB 계정을 만들고 root 전용 비밀 파일에 저장합니다.",
      command: `sudo systemctl restart ${dbService}`,
      settings: [
        ["DB 엔진", databaseLabel(report.database)],
        ["DB 이름", report.database_name || "-"],
        ["DB 계정", report.database_user || "-"],
        ["비밀번호 정책", report.database_password_policy === "user-provided-store-root-only" ? "사용자 입력값 root-only 저장" : "무작위 생성 후 root-only 저장"],
        ["비밀번호 보관", secretsPath],
        ["서비스", dbService],
      ],
      files: [
        configFile(secretsPath, "DB 비밀번호 root-only 보관", true),
        configFile(`${appRoot}/.env`, "앱 DB 접속 환경값", true),
      ],
      preview: [
        `database = ${report.database_name || "-"}`,
        `user = ${report.database_user || "-"}`,
        "password = ******",
        "host = 127.0.0.1",
        "grants = 앱 DB에 대한 최소 권한",
      ],
    }),
    provisioningAction("ssl", "SSL/Certbot", report.certbot_checks, {
      summary: "기존 인증서를 확인하고 자동 갱신만 테스트합니다. 반복 발급은 하지 않습니다.",
      command: `sudo certbot renew --dry-run --no-random-sleep-on-renew --cert-name ${certName}`,
      settings: [
        ["인증서 이름", certName],
        ["인증서 경로", `/etc/letsencrypt/live/${certName}/fullchain.pem`],
        ["키 경로", `/etc/letsencrypt/live/${certName}/privkey.pem`],
        ["중복 발급 방지", "기존 인증서가 있으면 새 발급 생략"],
        ["갱신", "certbot.timer"],
      ],
      files: [
        configFile(`/etc/letsencrypt/live/${certName}/fullchain.pem`, "공개 인증서 체인"),
        configFile(`/etc/letsencrypt/live/${certName}/privkey.pem`, "비공개 인증서 키", true),
        configFile(`/etc/letsencrypt/renewal/${certName}.conf`, "Certbot 자동 갱신 설정"),
      ],
      preview: [
        `cert_name = ${certName}`,
        `domains = ${serverNamesPreview(report.domain, report.www_mode)}`,
        "private_key = ******",
        "renewal = certbot.timer + renew dry-run",
      ],
    }),
    provisioningAction("mail", "메일 발송", report.mail_checks, {
      summary: mailSkipped ? "메일 발송 설정을 선택하지 않아 건너뛰었습니다." : "Postfix 또는 SMTP 릴레이 발송 설정을 확인합니다.",
      command: mailSkipped ? "설정 안 함" : "sudo systemctl restart postfix",
      forceStatus: mailSkipped ? "info" : null,
      forceLabel: mailSkipped ? "건너뜀" : null,
      settings: [
        ["방식", mailModeLabel(report.mail_mode)],
        ["SMTP 서버", report.smtp_host || "-"],
        ["SMTP 포트", report.smtp_port || "-"],
        ["발신 주소", report.smtp_from || "-"],
        ["서비스", mailSkipped ? "-" : "postfix"],
      ],
      files: mailSkipped ? [] : [
        configFile("/etc/postfix/main.cf", "Postfix 발송 설정"),
        configFile(`${appRoot}/.env`, "앱 메일 발송 환경값", true),
      ],
      preview: mailSkipped ? [
        "mail = disabled",
        "회원 인증/알림 메일은 앱에서 비활성 또는 별도 설정 필요",
      ] : [
        `mail_mode = ${mailModeLabel(report.mail_mode)}`,
        `smtp_host = ${report.smtp_host || "-"}`,
        `smtp_port = ${report.smtp_port || "-"}`,
        `smtp_from = ${report.smtp_from || "-"}`,
        "smtp_password = ******",
      ],
    }),
    provisioningAction("security", "보안/방화벽", [...(report.safety_checks || []), ...(report.firewall_checks || [])], {
      summary: "신규 VPS 기준 보안 정책, SSH 정책, UFW/공개 포트 상태를 확인합니다.",
      command: "sudo ufw status verbose && sudo systemctl is-active ssh",
      settings: [
        ["보안 수준", report.security_profile || "standard"],
        ["SSH 정책", report.ssh_policy || "audit-only"],
        ["공개 포트", "22/tcp, 80/tcp, 443/tcp"],
        ["적용 방식", "자동 변경보다 점검/승인 우선"],
      ],
      files: [
        configFile("/etc/ssh/sshd_config", "SSH 접속 정책"),
        configFile("/etc/ufw/user.rules", "UFW IPv4 규칙"),
        configFile("/etc/ufw/user6.rules", "UFW IPv6 규칙"),
      ],
      preview: [
        `security_profile = ${report.security_profile || "standard"}`,
        `ssh_policy = ${report.ssh_policy || "audit-only"}`,
        "ports = 22/tcp, 80/tcp, 443/tcp",
        "rule = SSH 자동 차단을 피하기 위해 점검/승인 후 적용",
      ],
    }),
    provisioningAction("app", "웹앱/G7 건강검사", report.app_checks, {
      summary: "앱 소스, .env, 쓰기 권한, G7 빌드 산출물과 업로드 제한 위치를 확인합니다.",
      command: selectedApp === "wordpress" ? `브라우저에서 ${appUrl} 접속` : `cd ${appRoot} && php artisan about`,
      settings: [
        ["앱", appPackageLabel(selectedApp)],
        ["앱 경로", appRoot],
        ["문서 루트", report.app_document_root || "-"],
        ["쓰기 경로", "storage, bootstrap/cache"],
        ["G7 core 빌드", `${appRoot}/public/build/core/template-engine.min.js`],
        ["설치 잠금", `${appRoot}/storage/app/g7_installed`],
        ["CKEditor 업로드 제한", `${appRoot}/storage/app/plugins/sirsoft-ckeditor5/settings/setting.json`],
        [".env 권한", "0640"],
        ["접속 링크", appUrl],
        ["안내서", report.setup_guide_path || "-"],
      ],
      files: [
        configFile(`${appRoot}/.env`, "앱 환경설정", true),
        configFile(`${appRoot}/storage/app/plugins/sirsoft-ckeditor5/settings/setting.json`, "CKEditor 업로드 제한 설정"),
        configFile(`${appRoot}/storage/app/g7_installed`, "그누보드7 설치 완료 잠금"),
        configFile(report.setup_guide_path || "/var/log/g7-installer/setup-guide.md", "설치 안내서"),
      ],
      preview: [
        `APP_URL=${appUrl}`,
        `DB_DATABASE=${report.database_name || "-"}`,
        `DB_USERNAME=${report.database_user || "-"}`,
        "DB_PASSWORD=******",
        "CACHE/SESSION/QUEUE = Redis 선택값 기준",
        "CKEditor imageMaxSizeMb = 앱 설정 파일에서 확인",
      ],
    }),
  ];
}

function provisioningAction(key, title, checks, options) {
  const status = options.forceStatus || provisioningStatus(checks);
  return {
    key,
    title,
    status,
    label: options.forceLabel || provisioningStatusLabel(status),
    summary: provisioningSummary(status, options.summary),
    command: options.command,
    settings: options.settings || [],
    files: options.files || [],
    preview: options.preview || [],
    checks: Array.isArray(checks) ? checks : [],
    cta: status === "fail" ? "실패 항목 확인" : status === "warn" ? "후속 확인" : "상세 확인",
    actionLabel: status === "fail" ? "다시 점검" : "재시작/점검",
  };
}

function provisioningStatus(checks) {
  const rows = Array.isArray(checks) ? checks : [];
  if (!rows.length) {
    return "pending";
  }
  if (rows.some((check) => check.status === "fail")) {
    return "fail";
  }
  if (rows.some((check) => ["manual", "deferred", "warn", "unknown"].includes(check.status))) {
    return "warn";
  }
  if (rows.some((check) => check.status === "pass")) {
    return "pass";
  }
  if (rows.every((check) => check.status === "skipped")) {
    return "info";
  }
  return "pending";
}

function provisioningStatusLabel(status) {
  const labels = {
    pass: "완료",
    warn: "후속 확인",
    fail: "실패",
    info: "건너뜀",
    pending: "대기",
  };
  return labels[status] || "대기";
}

function provisioningSummary(status, summary) {
  if (status === "fail") {
    return `${summary} 실패 항목을 먼저 해결해야 다음 운영 확인으로 넘어갑니다.`;
  }
  if (status === "warn") {
    return `${summary} 자동 처리 뒤 사람이 확인할 후속 작업이 남았습니다.`;
  }
  return summary;
}

function overallProvisioningStatus(actions) {
  if (actions.some((action) => action.status === "fail")) {
    return "fail";
  }
  if (actions.some((action) => action.status === "warn" || action.status === "pending")) {
    return "warn";
  }
  return "pass";
}

function overallProvisioningLabel(actions) {
  return provisioningStatusLabel(overallProvisioningStatus(actions));
}

function formatProvisionActionResult(result) {
  const rows = Array.isArray(result?.checks) ? result.checks : [];
  const details = rows
    .map((check) => `- [${checkStatusLabel(check.status, checkStatus(check.status), check)}] ${checkDisplayName(check.name)}: ${checkMessage(check)}`)
    .join("\n");
  return [result?.message || "작업 결과를 확인하세요.", details].filter(Boolean).join("\n");
}

function provisionActionByKey(key, report = currentReport()) {
  return provisioningActions(report).find((action) => action.key === key) || null;
}

function checkRowsHtml(checks) {
  const rows = Array.isArray(checks) && checks.length ? checks : [{ name: "아직 점검 전", status: "pending", message: "승인하고 적용/점검을 누르면 결과가 표시됩니다." }];
  return rows.map((check) => {
    const normalizedStatus = checkStatus(check.status);
    return `
      <div class="result-row" data-status="${escapeHtml(normalizedStatus)}">
        <div class="result-copy">
          <span>${escapeHtml(checkDisplayName(check.name))}</span>
          <p>${escapeHtml(checkMessage(check))}</p>
        </div>
        <strong>${escapeHtml(checkStatusLabel(check.status, normalizedStatus, check))}</strong>
      </div>
    `;
  }).join("");
}

function openProvisionActionDialog(action) {
  if (!action || !nodes.provisionActionDialog?.showModal) {
    return;
  }

  nodes.provisionActionDialog.dataset.action = action.key;
  nodes.provisionActionTitle.textContent = action.title;
  nodes.provisionActionSummary.textContent = action.summary;
  nodes.provisionActionStatus.textContent = action.label;
  nodes.provisionActionStatus.dataset.status = action.status;
  nodes.provisionActionDetails.innerHTML = `
    <section>
      <h4>설정값</h4>
      <dl>
        ${(action.settings || []).map(([key, value]) => `
          <div>
            <dt>${escapeHtml(key)}</dt>
            <dd>${escapeHtml(value ?? "-")}</dd>
          </div>
        `).join("")}
      </dl>
    </section>
    <section>
      <h4>보안 표시 기준</h4>
      <p class="provision-muted">설정 파일은 읽기 전용으로 보여주고, DB/SMTP 비밀번호, 토큰, private key 값은 화면에 원문으로 표시하지 않습니다.</p>
    </section>
    <section class="md:col-span-2">
      <h4>수정/확인 설정 파일</h4>
      <p class="provision-muted">설치기가 생성하거나 검증하는 설정 파일 경로입니다. 실제 편집은 서버에서 백업 후 installer-owned 범위로만 수행합니다.</p>
      ${configFilesHtml(action.files)}
    </section>
    <section class="md:col-span-2">
      <h4>마스킹된 핵심 내용</h4>
      <p class="provision-muted">원문 파일 전체가 아니라 사용자가 확인해야 하는 핵심 설정만 요약합니다.</p>
      ${configPreviewHtml(action.preview)}
    </section>
    <section>
      <h4>실행/재시작 명령</h4>
      <code>${escapeHtml(action.command)}</code>
    </section>
    <section class="md:col-span-2">
      <h4>현재 검증 결과</h4>
      <div class="result-list mt-3">${checkRowsHtml(action.checks)}</div>
    </section>
  `;
  const priorResult = state.provisionActionResults[action.key];
  if (priorResult) {
    nodes.provisionActionResult.hidden = false;
    nodes.provisionActionResult.classList.remove("hidden");
    nodes.provisionActionResult.textContent = formatProvisionActionResult(priorResult);
  } else {
    nodes.provisionActionResult.hidden = true;
    nodes.provisionActionResult.classList.add("hidden");
    nodes.provisionActionResult.textContent = "";
  }
  setButtonLabel(nodes.provisionActionRun, action.actionLabel);
  nodes.provisionActionDialog.returnValue = "cancel";
  nodes.provisionActionDialog.showModal();
}

async function runProvisionAction(actionKey, button) {
  const action = provisionActionByKey(actionKey);
  if (!action) {
    setAlert(nodes.provisionStatus, "error", "세부 설정 실패", "실행할 설정 항목을 찾지 못했습니다.");
    return;
  }

  await withBusy(button, "실행 중", async () => {
    try {
      const result = await apiFetch("/api/provision/action", {
        method: "POST",
        body: JSON.stringify({ action: action.key }),
      });
      state.provisionActionResults[action.key] = result;
      const failed = result.status === "fail";
      nodes.provisionActionResult.hidden = false;
      nodes.provisionActionResult.classList.remove("hidden");
      nodes.provisionActionResult.textContent = formatProvisionActionResult(result);
      setAlert(
        nodes.provisionStatus,
        failed ? "error" : "success",
        failed ? `${action.title} 점검 실패` : `${action.title} 점검 완료`,
        result.message,
      );
      log(`세부 설정 점검: ${result.action} ${result.status}`);
      const reportPayload = await apiFetch("/api/report").catch(() => null);
      if (reportPayload?.exists) {
        state.savedReportPayload = reportPayload;
        renderProvisionPanel(parseSavedReport(reportPayload));
      }
      saveWizardState();
    } catch (error) {
      setAlert(nodes.provisionStatus, "error", "세부 설정 점검 실패", formatError(error));
      nodes.provisionActionResult.hidden = false;
      nodes.provisionActionResult.classList.remove("hidden");
      nodes.provisionActionResult.textContent = formatError(error);
      log(formatError(error));
    }
  });
}

function checksCard(title, checks, id = "") {
  const rows = Array.isArray(checks) && checks.length ? checks : [{ name: "없음", status: "pending", message: "표시할 항목이 없습니다." }];
  return `
    <section class="report-card"${id ? ` id="${escapeHtml(id)}"` : ""}>
      <h3>${escapeHtml(title)}</h3>
      <div class="result-list mt-3">
        ${rows.map((check) => {
          const normalizedStatus = checkStatus(check.status);
          return `
            <div class="result-row" data-status="${normalizedStatus}">
            <div class="result-copy">
              <span>${escapeHtml(checkDisplayName(check.name))}</span>
              <p>${escapeHtml(checkMessage(check))}</p>
            </div>
            <strong>${escapeHtml(checkStatusLabel(check.status, normalizedStatus, check))}</strong>
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

function checkDisplayName(name = "") {
  if (checkLabel[name]) {
    return checkLabel[name];
  }

  const text = String(name || "");
  if (text.endsWith("-git-head")) {
    return "Git HEAD 확인";
  }
  if (text.endsWith("-git-fsck")) {
    return "Git 무결성 검사";
  }
  if (text.endsWith("-git-clean")) {
    return "Git 작업트리 확인";
  }
  if (text.includes("-git-tracked-")) {
    return "Git 추적 파일 확인";
  }
  if (text.endsWith("-archive-test")) {
    return "압축 파일 무결성 검사";
  }
  if (text.includes("-source-file-")) {
    return "소스 파일 확인";
  }
  if (text.includes("-source-dir-")) {
    return "소스 디렉터리 확인";
  }
  if (text.includes("-deployed-file-")) {
    return "배포 파일 확인";
  }
  if (text.includes("-deployed-dir-")) {
    return "배포 디렉터리 확인";
  }
  if (text.startsWith("php-extension:")) {
    return `PHP 확장 ${text.replace("php-extension:", "")}`;
  }
  return text || "점검";
}

function checkStatusLabel(status, normalizedStatus, check = null) {
  if (check && isPackageLikeCheck(check.name, check.message)) {
    if (status === "pass") {
      return "설치 완료";
    }
    if (status === "installed") {
      return "기존 패키지";
    }
    if (status === "not-installed") {
      return "설치 대상";
    }
    if (status === "fail") {
      return "설치 실패";
    }
  }
  return statusLabel[status] || statusLabel[normalizedStatus] || status || "대기";
}

function checkMessage(checkOrStatus, maybeMessage = "") {
  const check = typeof checkOrStatus === "object" && checkOrStatus !== null
    ? checkOrStatus
    : { status: checkOrStatus, message: maybeMessage, name: "" };
  if (isPackageLikeCheck(check.name, check.message)) {
    return packageStatusMessage(check);
  }
  const labelByMessage = {
    "package was already installed before G7 installer ran": "설치 전부터 있던 패키지입니다. 되돌리기 때 보존합니다.",
    "package was absent before G7 installer ran": "설치 전에는 없던 패키지입니다. 이번 설치 대상입니다.",
    "package preinstall state is unknown": "설치 전 패키지 상태를 확인하지 못했습니다.",
  };
  return labelByMessage[check.message] || localizeMessage(check.message) || "";
}

function actionStatus(status) {
  if (["removed", "disabled", "reset", "pass", "ok", "deleted", "dropped", "purged", "reloaded"].includes(status)) {
    return "pass";
  }
  if (["skipped", "missing", "pending", "would-disable", "would-delete", "would-drop", "would-purge", "would-reload"].includes(status)) {
    return "warn";
  }
  return "fail";
}

function accessLink(domain, phase = "packages-installed") {
  if (phase !== "completed") {
    return {
      html: `<span class="text-base-content/60">사이트 페이지는 아직 생성 전입니다.</span>`,
      hint: "서버 세팅, SSL, 앱 파일 배치가 모두 끝난 뒤 웹앱 설치 링크를 제공합니다.",
    };
  }

  const href = `https://${domain}`;
  return {
    html: `<a class="link link-primary" href="${escapeHtml(href)}" target="_blank" rel="noreferrer">${escapeHtml(href)}</a>`,
    hint: "서버 세팅과 앱 파일 배치가 끝난 뒤 접속합니다.",
  };
}

function urlLink(url) {
  return {
    html: `<a class="link link-primary" href="${escapeHtml(url)}" target="_blank" rel="noreferrer">${escapeHtml(url)}</a>`,
    hint: "웹앱 설치 화면 또는 준비 페이지로 바로 이동합니다.",
  };
}

function compactListCard(title, rows, emptyText = "없음") {
  const items = Array.isArray(rows) && rows.length ? rows : [emptyText];
  return `
    <section class="report-card">
      <h3>${escapeHtml(title)}</h3>
      <ul class="report-list">
        ${items.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}
      </ul>
    </section>
  `;
}

function packagePlanCard(packages) {
  const rows = flattenPlanPackages(packages);
  return `
    <section class="report-card">
      <h3>설치할 패키지</h3>
      <div class="package-plan-list">
        ${rows.length ? rows.map((item) => `
          <div class="package-plan-item">
            <div>
              <span>${escapeHtml(item.name)}</span>
              <p>${escapeHtml(item.description)}</p>
            </div>
            <strong>패키지</strong>
          </div>
        `).join("") : `
          <div class="empty-state">설치할 패키지가 없습니다.</div>
        `}
      </div>
    </section>
  `;
}

function provisioningPlanCard(provisioning) {
  const sections = Array.isArray(provisioning) ? provisioning : [];
  return `
    <section class="report-card">
      <h3>서버 설정 계획</h3>
      <div class="result-list mt-3">
        ${sections.length ? sections.map((section) => {
          const settings = Array.isArray(section.settings) && section.settings.length
            ? section.settings.slice(0, 4).map((item) => `${item.key}: ${item.value}`).join(" / ")
            : "상세 설정 없음";
          return `
            <div class="result-row" data-status="info">
              <div class="result-copy">
                <span>${escapeHtml(section.title || "설정")}</span>
                <p>${escapeHtml(`${section.summary || ""} ${settings}`.trim())}</p>
              </div>
              <strong>설정</strong>
            </div>
          `;
        }).join("") : `
          <div class="empty-state">추가 서버 설정 계획이 없습니다.</div>
        `}
      </div>
    </section>
  `;
}

function renderPlanReport(report) {
  const files = Array.isArray(report.files)
    ? report.files.map((item) => `${item.path} (${item.action})`)
    : [];
  const services = Array.isArray(report.services)
    ? report.services.map((item) => `${item.name} (${item.action})`)
    : [];
  const ports = Array.isArray(report.ports)
    ? report.ports.map((item) => `${item.port}/${item.protocol}: ${item.purpose}`)
    : [];
  const stopConditions = Array.isArray(report.stop_conditions) ? report.stop_conditions : [];

  return [
    reportSummaryCard("선택한 설치 사양", [
      ["도메인", report.domain],
      ["웹서버 / PHP", `${runtimeLabel(report.web_server)} / ${phpRuntimeLabel(report.php_version, report.php_source)}`],
      ["데이터베이스", `${databaseLabel(report.database)} (${databaseVersionLabel(report.database_version)})`],
      ["DB 이름", report.database_name || "-"],
      ["DB 계정", report.database_user || "-"],
      ["DB 비밀번호", report.database_password_policy === "user-provided-store-root-only" ? "사용자 입력값 저장" : "무작위 생성"],
      ["앱 패키지", `${appPackageLabel(report.app_package)} - 서버 스택 준비 후 마지막 설치 대상`],
      ["사이트 계정", report.site_user],
      ["웹루트", report.web_root],
      ["앱 문서 루트", report.app_document_root || "-"],
      ["배포 모드", report.deployment_mode || "public"],
    ], "이 사양이 맞으면 아래 설치 패키지와 변경 항목을 확인한 뒤 진행하세요. 다르면 이전으로 돌아가 사양을 수정하세요."),
    packagePlanCard(report.packages),
    provisioningPlanCard(report.provisioning),
    compactListCard("생성/변경 예정 파일", files),
    compactListCard("서비스 계획", services),
    compactListCard("포트 계획", ports),
    compactListCard("진행 전 확인", [
      "맞으면 이 사양으로 진행을 눌러 5단계 기본 구성을 시작합니다.",
      "안 맞으면 이전을 눌러 3단계 설치 방식에서 사양을 수정합니다.",
      "수정 후 4단계로 돌아오면 계획은 자동으로 다시 생성됩니다.",
    ]),
    stopConditions.length ? compactListCard("중단 조건", stopConditions) : "",
  ].join("");
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
  clearActivityLog();
  installStageOrder.forEach((stage) => {
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
    ["DB 이름", payload.database_name],
    ["DB 계정", payload.database_user],
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
    title: "재설치 초기화를 실행할까요?",
    message: "신규 VPS 전용 작업입니다. 설치기가 만든 계정, DB, 서비스, 웹루트/설정 파일, 패키지, 메타데이터를 정리합니다. Let's Encrypt 인증서는 보존합니다.",
    yesClass: "btn btn-primary icon-button",
    rows: [
      ["대상", "installer가 생성한 사이트 계정, DB/DB 계정, 서비스, 웹루트/설정 파일, 새 패키지, 상태 파일"],
      ["보존", "설치 전부터 있던 패키지, 운영자가 만든 파일, Let's Encrypt 인증서"],
      ["실행 후", "서버 점검 단계로 돌아가 다시 설치할 수 있습니다."],
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

function resetWizardForRetry(options = {}) {
  const targetStep = options.targetStep || "check";
  clearWizardState();
  state.doctorReport = null;
  state.planReport = null;
  state.planSignature = null;
  state.savedReportPayload = null;
  state.recoveryStatus = null;
  setPlanReady(false);
  setDoctorPassed(false);
  setReportReady(false);
  state.installRunning = false;
  state.installCompleted = false;
  state.currentOperation = null;
  state.provisionActionResults = {};
  stopPackageTicker();
  hideAlert(nodes.planStatus);
  hideAlert(nodes.installStatus);
  hideAlert(nodes.reportStatus);
  hideReportProgress();
  resetInstallStages();
  nodes.doctorResults.innerHTML = `<div class="empty-state">아직 점검 전입니다. 점검 실행을 누르세요.</div>`;
  nodes.planOutput.innerHTML = `<div class="empty-state">선택한 사양을 바탕으로 설치 계획을 자동 생성합니다.</div>`;
  renderPackageProgress([]);
  nodes.reportOutput.innerHTML = `<div class="empty-state">아직 리포트가 없습니다.</div>`;
  if (nodes.provisionOutput) {
    nodes.provisionOutput.innerHTML = `<div class="empty-state">6단계 결과 리포트가 생성되면 세부 설정 카드가 표시됩니다.</div>`;
  }
  refreshInstallButtonState();
  renderRecoveryStatus(null);
  showStep(targetStep, { force: true });
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
    state.planSignature = planRequestSignature(optionPayload());
    nodes.planOutput.innerHTML = renderPlanReport(saved.planReport);
    renderPackageProgress(flattenPlanPackages(saved.planReport.packages));
    setPlanReady(true);
  }
  if (saved.savedReportPayload) {
    renderSavedReport(saved.savedReportPayload);
  }
  if (saved.recoveryStatus) {
    renderRecoveryStatus(saved.recoveryStatus);
  }
  if (saved.provisionActionResults && typeof saved.provisionActionResults === "object") {
    state.provisionActionResults = saved.provisionActionResults;
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
  let resetCompletionAcknowledged = false;

  try {
    state.currentOperation = action;
    setButtonLabel(button, busyText);
    if (action === "reset") {
      showOperationOverlay("초기화 중입니다.", "서버 정리 작업이 완료될 때까지 기다려 주세요.");
    }
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
        : "installer가 만든 리소스를 정리해 재설치 가능 상태로 되돌렸습니다. 인증서는 보존했습니다.",
    );
    log(successTitle);
    clearWizardState();
    state.installCompleted = false;
    state.planReport = null;
    state.planSignature = null;
    state.savedReportPayload = null;
    setPlanReady(false);
    setReportReady(false);
    refreshInstallButtonState();
    await refreshRecoveryStatus();
    recoveryCompleted = true;
    if (action === "rollback") {
      await runDoctorCheck();
    } else {
      state.currentOperation = null;
      setOperationLocked(false);
      if (button && originalText) {
        setButtonLabel(button, originalText);
      }
      await completeOperationOverlay(
        "초기화 완료되었습니다.",
        "확인을 누르면 접속 확인 단계로 이동합니다.",
      );
      resetCompletionAcknowledged = true;
    }
  } catch (error) {
    if (action === "reset") {
      hideOperationOverlay();
    }
    renderErrorReport(action === "rollback" ? "패키지 되돌리기 실패" : "리셋 실패", formatError(error));
    setAlert(statusNode, "error", action === "rollback" ? "패키지 되돌리기 실패" : "리셋 실패", formatError(error));
    log(formatError(error));
    await refreshRecoveryStatus();
  } finally {
    state.currentOperation = null;
    setOperationLocked(false);
    if (action === "reset" && !resetCompletionAcknowledged) {
      hideOperationOverlay();
    }
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
    if (action === "reset") {
      resetWizardForRetry({ targetStep: "login" });
    } else {
      showStep("check");
    }
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
    button.tabIndex = -1;
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
    showStep(event.state?.step || stepFromLocation(), { pushHistory: false });
  });

  nodes.themeToggle.addEventListener("click", () => {
    applyTheme(state.theme === "dark" ? "light" : "dark");
  });

  nodes.operationOverlayConfirm?.addEventListener("click", () => {
    hideOperationOverlay();
    const resolve = operationOverlayResolve;
    operationOverlayResolve = null;
    if (resolve) {
      resolve();
    }
  });

  document.querySelectorAll("[data-step]").forEach((button) => {
    button.addEventListener("click", () => showStep(button.dataset.step));
  });

  document.querySelectorAll("[data-next]").forEach((button) => {
    button.addEventListener("click", () => showStep(button.dataset.next));
  });

  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-report-jump]");
    if (!button) {
      return;
    }
    const target = document.getElementById(button.dataset.reportJump);
    if (target) {
      target.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  });

  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-download-report]");
    if (!button) {
      return;
    }
    downloadReport(button.dataset.downloadReport || "json");
  });

  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-promo-dismiss]");
    if (!button) {
      return;
    }

    localStorage.setItem(promoDismissStorageKey, nodes.promoPanel?.dataset.promoKey || "");
    hidePromoPanel();
  });

  document.addEventListener("click", async (event) => {
    const button = event.target.closest("[data-provision-open]");
    if (!button) {
      return;
    }
    openProvisionActionDialog(provisionActionByKey(button.dataset.provisionOpen));
  });

  nodes.provisionActionRun?.addEventListener("click", (event) => {
    event.preventDefault();
    void runProvisionAction(nodes.provisionActionDialog?.dataset.action, event.currentTarget);
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

  nodes.planButton.addEventListener("click", async (event) => {
    await withBusy(event.currentTarget, "새로고침", async () => {
      await generatePlan({ force: true });
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
      setAlert(nodes.installStatus, "error", "계정 정보 확인 필요", passwordError);
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
      setAlert(nodes.installStatus, "info", "서버 세팅 진행 중", "패키지, 사이트 계정/웹루트, vhost, PHP 런타임, DB, SSL, 앱 파일 배치 순서로 검증합니다.");
      setActivityStatus("서버 세팅 시작", "패키지 설치 전 사전 점검을 실행합니다.", 5);
      log("서버 세팅 시작");
      const report = await apiFetch("/api/install/prepare", {
        method: "POST",
        body: JSON.stringify(payload),
      });
      renderInstallReport(report);
      await refreshRecoveryStatus();
      setAlert(nodes.installStatus, "success", "서버 세팅 완료", "결과 리포트에서 웹서버, PHP, DB, SSL, 앱 경로와 재시작 명령을 확인하세요.");
      showStep("report");
      log(`서버 세팅 완료: ${report.phase}`);
    } catch (error) {
      stopPackageTicker();
      const reportPayload = await apiFetch("/api/report").catch(() => null);
      if (reportPayload?.exists) {
        renderSavedReport(reportPayload);
        restoreInstallStateFromReport(parseSavedReport(reportPayload));
      } else {
        markStage("packages", "실패");
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

  nodes.provisionRefreshButton?.addEventListener("click", async (event) => {
    await withBusy(event.currentTarget, "새로고침", async () => {
      try {
        hideAlert(nodes.provisionStatus);
        const report = await apiFetch("/api/report");
        state.savedReportPayload = report;
        if (report.exists) {
          renderSavedReport(report);
          renderProvisionPanel(parseSavedReport(report));
          setAlert(nodes.provisionStatus, "success", "세부 설정 새로고침 완료", "서버에 저장된 리포트를 기준으로 카드를 다시 그렸습니다.");
        } else {
          renderProvisionPanel(null);
          setAlert(nodes.provisionStatus, "warning", "리포트 없음", "아직 생성된 리포트가 없습니다.");
        }
        log(`세부 설정 리포트 불러오기: ${report.exists ? "있음" : "없음"}`);
      } catch (error) {
        setAlert(nodes.provisionStatus, "error", "세부 설정 새로고침 실패", formatError(error));
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
    showStep(stepFromLocation(), { pushHistory: false });
    writeStepHistory(state.activeStep, true);
    log("웹 컨트롤러 준비 완료");
    log(state.bootstrap.auth.authenticated ? "접속 확인 완료" : "접속 확인 필요");
    if (state.bootstrap.auth.client_ip) {
      log(`접속 IP 잠금 완료: ${state.bootstrap.auth.client_ip}`);
    }
    void loadPromoManifest();
  } catch (error) {
    setConnectionStatus("오류", "badge-error");
    setAlert(nodes.loginStatus, "error", "접속 확인 실패", `${formatError(error)}\n터미널에 출력된 접속 확인 주소로 다시 접속하세요.`);
    log(`${formatError(error)}\n터미널에 출력된 접속 확인 주소로 다시 접속하세요.`);
  }
}

boot();
