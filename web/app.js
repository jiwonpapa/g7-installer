const statusNode = document.querySelector("#connection-status");
const logNode = document.querySelector("#live-log");
const domainNode = document.querySelector("#prefill-domain");
const modeNode = document.querySelector("#prefill-mode");
const authNode = document.querySelector("#auth-mode");

function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  logNode.textContent += `\n[${timestamp}] ${message}`;
  logNode.scrollTop = logNode.scrollHeight;
}

async function loadBootstrap() {
  const response = await fetch("/api/bootstrap");
  if (!response.ok) {
    throw new Error(`bootstrap failed: ${response.status}`);
  }

  return response.json();
}

async function boot() {
  try {
    const payload = await loadBootstrap();
    statusNode.textContent = "connected";
    statusNode.className = "ml-2 font-medium text-emerald-300";
    domainNode.textContent = payload.domain || "not selected";
    modeNode.textContent = payload.local_test ? "local-test" : "public";
    authNode.textContent = `${payload.auth.mode} / ${payload.auth.status}`;
    log("web controller bootstrap loaded");
  } catch (error) {
    statusNode.textContent = "error";
    statusNode.className = "ml-2 font-medium text-red-300";
    log(error.message);
  }
}

boot();
