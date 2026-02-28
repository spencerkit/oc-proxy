function pretty(value) {
  return JSON.stringify(value, null, 2);
}

function deepClone(value) {
  return JSON.parse(JSON.stringify(value));
}

function genId(prefix) {
  return `${prefix}_${Math.random().toString(36).slice(2, 10)}`;
}

function sanitizePath(path) {
  return String(path || "").trim().replace(/[^a-zA-Z0-9_-]/g, "_");
}

function escapeHtml(value) {
  return String(value || "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

async function copyText(text) {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const input = document.createElement("textarea");
  input.value = text;
  document.body.appendChild(input);
  input.select();
  document.execCommand("copy");
  document.body.removeChild(input);
}

const state = {
  config: null,
  status: null,
  logs: [],
  currentPage: "service",
  activeGroupId: null,
  pendingDeleteGroupId: null
};

const els = {
  serviceState: document.getElementById("serviceState"),
  serviceSwitch: document.getElementById("serviceSwitch"),
  addGroupBtn: document.getElementById("addGroupBtn"),
  openSettingsBtn: document.getElementById("openSettingsBtn"),
  openLogsBtn: document.getElementById("openLogsBtn"),
  servicePage: document.getElementById("servicePage"),
  logsPage: document.getElementById("logsPage"),
  groupTabs: document.getElementById("groupTabs"),
  groupBody: document.getElementById("groupBody"),
  refreshLogsBtn: document.getElementById("refreshLogsBtn"),
  clearLogsBtn: document.getElementById("clearLogsBtn"),
  logsBox: document.getElementById("logsBox"),
  groupModal: document.getElementById("groupModal"),
  groupNameInput: document.getElementById("groupNameInput"),
  groupPathInput: document.getElementById("groupPathInput"),
  cancelAddGroupBtn: document.getElementById("cancelAddGroupBtn"),
  confirmAddGroupBtn: document.getElementById("confirmAddGroupBtn"),
  settingsModal: document.getElementById("settingsModal"),
  hostInput: document.getElementById("hostInput"),
  portInput: document.getElementById("portInput"),
  strictModeInput: document.getElementById("strictModeInput"),
  cancelSettingsBtn: document.getElementById("cancelSettingsBtn"),
  saveSettingsBtn: document.getElementById("saveSettingsBtn"),
  deleteGroupModal: document.getElementById("deleteGroupModal"),
  deleteGroupText: document.getElementById("deleteGroupText"),
  cancelDeleteGroupBtn: document.getElementById("cancelDeleteGroupBtn"),
  confirmDeleteGroupBtn: document.getElementById("confirmDeleteGroupBtn"),
  toast: document.getElementById("toast")
};

let toastTimer = null;

function showToast(message) {
  els.toast.textContent = message;
  els.toast.classList.remove("hidden");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    els.toast.classList.add("hidden");
  }, 2200);
}

function openModal(modalEl) {
  modalEl.classList.remove("hidden");
}

function closeModal(modalEl) {
  modalEl.classList.add("hidden");
}

function setPage(page) {
  state.currentPage = page;
  renderPage();
  if (page === "logs") {
    refreshLogs().catch((err) => showToast(`日志刷新失败: ${err.message}`));
  }
}

function renderPage() {
  const onLogs = state.currentPage === "logs";
  els.servicePage.classList.toggle("hidden", onLogs);
  els.logsPage.classList.toggle("hidden", !onLogs);
  els.openLogsBtn.textContent = onLogs ? "返回服务" : "日志";
  els.openLogsBtn.classList.toggle("btn-primary", onLogs);
}

function getActiveGroup() {
  return (state.config.groups || []).find((g) => g.id === state.activeGroupId) || null;
}

async function refreshStatus() {
  state.status = await window.proxyApp.getStatus();
  renderStatus();
}

function renderStatus() {
  const running = !!state.status?.running;
  const port = state.config?.server?.port || "-";
  const host = state.config?.server?.host || "-";
  const reqCount = state.status?.metrics?.requests || 0;
  const errCount = state.status?.metrics?.errors || 0;
  const latency = state.status?.metrics?.avgLatencyMs || 0;

  els.serviceSwitch.checked = running;
  els.serviceState.textContent = running
    ? `运行中 | ${host}:${port} | 请求 ${reqCount} | 错误 ${errCount} | 平均延迟 ${latency}ms`
    : `已停止 | ${host}:${port}`;
}

function renderTabs() {
  const groups = state.config.groups || [];
  els.groupTabs.innerHTML = "";

  if (groups.length === 0) {
    const empty = document.createElement("div");
    empty.textContent = "暂无分组，请先点击“添加分组”。";
    empty.className = "group-meta";
    els.groupTabs.appendChild(empty);
    return;
  }

  for (const group of groups) {
    const btn = document.createElement("button");
    btn.className = `tab-btn ${group.id === state.activeGroupId ? "active" : ""}`;
    btn.textContent = group.name;
    btn.addEventListener("click", () => {
      state.activeGroupId = group.id;
      renderTabs();
      renderGroupBody();
    });
    els.groupTabs.appendChild(btn);
  }
}

function createRuleCard(group, rule) {
  const item = document.createElement("div");
  item.className = "rule-item";
  item.innerHTML = `
    <div class="rule-grid">
      <div class="field">
        <label>模型名称</label>
        <input class="rule-model" value="${escapeHtml(rule.model)}" />
      </div>
      <div class="field">
        <label>转发方向</label>
        <select class="rule-direction">
          <option value="oc" ${rule.direction === "oc" ? "selected" : ""}>OpenAI -> Anthropic</option>
          <option value="co" ${rule.direction === "co" ? "selected" : ""}>Anthropic -> OpenAI</option>
        </select>
      </div>
      <div class="field">
        <label>Token</label>
        <input class="rule-token" type="password" value="${escapeHtml(rule.token)}" />
      </div>
      <div class="field">
        <label>API 地址</label>
        <input class="rule-api" value="${escapeHtml(rule.apiAddress)}" />
      </div>
    </div>
    <div class="rule-foot">
      <label class="active-mark">
        <input class="rule-active" type="radio" name="active_rule_${group.id}" ${group.activeRuleId === rule.id ? "checked" : ""} />
        当前生效
      </label>
      <div class="row-actions">
        <button class="save-rule btn-primary">保存规则</button>
        <button class="delete-rule btn-danger">删除</button>
      </div>
    </div>
  `;

  item.querySelector(".save-rule").addEventListener("click", async () => {
    rule.model = item.querySelector(".rule-model").value.trim();
    rule.direction = item.querySelector(".rule-direction").value;
    rule.token = item.querySelector(".rule-token").value.trim();
    rule.apiAddress = item.querySelector(".rule-api").value.trim();
    await saveConfig("规则已保存");
  });

  item.querySelector(".rule-active").addEventListener("change", async () => {
    group.activeRuleId = rule.id;
    await saveConfig("已切换生效规则");
  });

  item.querySelector(".delete-rule").addEventListener("click", async () => {
    group.rules = (group.rules || []).filter((r) => r.id !== rule.id);
    if (group.activeRuleId === rule.id) {
      group.activeRuleId = group.rules[0] ? group.rules[0].id : null;
    }
    await saveConfig("规则已删除");
  });

  return item;
}

function openDeleteGroupConfirm(group) {
  state.pendingDeleteGroupId = group.id;
  els.deleteGroupText.textContent = `将删除分组“${group.name}”（path: ${group.path}）。该操作不可撤销，确认继续吗？`;
  openModal(els.deleteGroupModal);
}

async function deletePendingGroup() {
  if (!state.pendingDeleteGroupId) {
    closeModal(els.deleteGroupModal);
    return;
  }
  state.config.groups = (state.config.groups || []).filter((g) => g.id !== state.pendingDeleteGroupId);
  if (state.activeGroupId === state.pendingDeleteGroupId) {
    state.activeGroupId = state.config.groups[0] ? state.config.groups[0].id : null;
  }
  state.pendingDeleteGroupId = null;
  closeModal(els.deleteGroupModal);
  await saveConfig("分组已删除");
}

function renderGroupBody() {
  const group = getActiveGroup();
  els.groupBody.innerHTML = "";

  if (!group) {
    const empty = document.createElement("div");
    empty.className = "group-meta";
    empty.textContent = "请选择一个分组。";
    els.groupBody.appendChild(empty);
    return;
  }

  const port = state.config.server.port;
  const baseUrl = `http://localhost:${port}/oc/${group.path}`;

  const head = document.createElement("div");
  head.className = "group-head";
  head.innerHTML = `
    <div>
      <h2>${escapeHtml(group.name)}</h2>
      <div class="group-meta">分组 Path: <code>${escapeHtml(group.path)}</code></div>
      <div class="group-meta entry-line">
        <span>入口 URL:</span>
        <code class="entry-url">${escapeHtml(baseUrl)}</code>
        <button class="icon-btn copy-entry" title="复制入口 URL" aria-label="复制入口 URL">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M9 9h11v11H9z"></path>
            <path d="M4 4h11v2H6v9H4z"></path>
          </svg>
        </button>
      </div>
    </div>
    <div class="row-actions">
      <button class="add-rule btn-primary">添加规则</button>
      <button class="delete-group btn-danger">删除分组</button>
    </div>
  `;

  head.querySelector(".copy-entry").addEventListener("click", async () => {
    try {
      await copyText(baseUrl);
      showToast("入口 URL 已复制");
    } catch (err) {
      showToast(`复制失败: ${err.message}`);
    }
  });

  head.querySelector(".add-rule").addEventListener("click", async () => {
    const newRule = {
      id: genId("rule"),
      model: "",
      token: "",
      apiAddress: "https://api.anthropic.com",
      direction: "oc"
    };
    group.rules = group.rules || [];
    group.rules.push(newRule);
    if (!group.activeRuleId) {
      group.activeRuleId = newRule.id;
    }
    await saveConfig("规则已创建");
  });

  head.querySelector(".delete-group").addEventListener("click", () => {
    openDeleteGroupConfirm(group);
  });

  els.groupBody.appendChild(head);

  const list = document.createElement("div");
  list.className = "rule-list";

  if (!group.rules || group.rules.length === 0) {
    const empty = document.createElement("div");
    empty.className = "group-meta";
    empty.textContent = "该分组暂无规则，请点击“添加规则”。";
    list.appendChild(empty);
  } else {
    for (const rule of group.rules) {
      list.appendChild(createRuleCard(group, rule));
    }
  }

  els.groupBody.appendChild(list);
}

function renderLogs() {
  if (!state.logs || state.logs.length === 0) {
    els.logsBox.textContent = "暂无日志";
    return;
  }

  const items = state.logs.slice().reverse();
  const lines = [];
  for (const log of items) {
    const body = pretty(log.requestBody == null ? null : log.requestBody);
    const forwarding = log.forwardingAddress || "未进入转发（可能是路径/鉴权/规则校验失败）";
    const statusText = `HTTP ${String(log.httpStatus ?? "-")} | ${log.status || "unknown"}`;
    const errorText = log.error?.message ? ` | 原因：${log.error.message}` : "";
    lines.push(`请求：${log.requestAddress || "-"}`);
    lines.push(`状态：${statusText}${errorText}`);
    lines.push(`请求体：${body}`);
    lines.push(`准备转发到：${forwarding}`);
    lines.push("----------------------------------------");
  }
  els.logsBox.textContent = lines.join("\n");
}

function renderAll() {
  if (!state.config) return;
  if (!state.activeGroupId && state.config.groups && state.config.groups[0]) {
    state.activeGroupId = state.config.groups[0].id;
  }
  if (state.activeGroupId && !(state.config.groups || []).some((g) => g.id === state.activeGroupId)) {
    state.activeGroupId = state.config.groups[0] ? state.config.groups[0].id : null;
  }
  renderStatus();
  renderTabs();
  renderGroupBody();
  renderPage();
}

async function saveConfig(message) {
  const result = await window.proxyApp.saveConfig(deepClone(state.config));
  state.config = result.config;
  state.status = result.status || state.status;
  renderAll();

  if (result.restarted) {
    showToast("重启完成");
  } else if (message) {
    showToast(message);
  }
}

async function refreshLogs() {
  state.logs = await window.proxyApp.listLogs(100);
  renderLogs();
}

async function addGroup() {
  const name = els.groupNameInput.value.trim();
  const path = sanitizePath(els.groupPathInput.value.trim());

  if (!name || !path) {
    showToast("请填写分组名称和 path");
    return;
  }
  if ((state.config.groups || []).some((g) => g.path === path)) {
    showToast("该 path 已存在");
    return;
  }

  const group = {
    id: genId("group"),
    name,
    path,
    activeRuleId: null,
    rules: []
  };

  state.config.groups.push(group);
  state.activeGroupId = group.id;
  await saveConfig("分组已创建");

  els.groupNameInput.value = "";
  els.groupPathInput.value = "";
  closeModal(els.groupModal);
}

async function onToggleService() {
  if (els.serviceSwitch.checked) {
    await window.proxyApp.startServer();
    showToast("服务已启动");
  } else {
    await window.proxyApp.stopServer();
    showToast("服务已停止");
  }
  await refreshStatus();
}

function openSettings() {
  els.hostInput.value = state.config.server.host;
  els.portInput.value = String(state.config.server.port);
  els.strictModeInput.checked = !!state.config.compat.strictMode;
  openModal(els.settingsModal);
}

async function saveSettings() {
  const nextPort = Number(els.portInput.value);
  if (!Number.isInteger(nextPort) || nextPort < 1 || nextPort > 65535) {
    showToast("端口必须是 1-65535 的整数");
    return;
  }

  state.config.server.host = els.hostInput.value.trim() || "0.0.0.0";
  state.config.server.port = nextPort;
  state.config.compat.strictMode = !!els.strictModeInput.checked;

  await saveConfig("设置已保存");
  closeModal(els.settingsModal);
}

els.serviceSwitch.addEventListener("change", () => {
  onToggleService().catch((err) => showToast(`操作失败: ${err.message}`));
});

els.addGroupBtn.addEventListener("click", () => {
  openModal(els.groupModal);
});

els.cancelAddGroupBtn.addEventListener("click", () => {
  closeModal(els.groupModal);
});

els.confirmAddGroupBtn.addEventListener("click", () => {
  addGroup().catch((err) => showToast(`创建失败: ${err.message}`));
});

els.openSettingsBtn.addEventListener("click", openSettings);

els.openLogsBtn.addEventListener("click", () => {
  setPage(state.currentPage === "logs" ? "service" : "logs");
});

els.cancelSettingsBtn.addEventListener("click", () => closeModal(els.settingsModal));
els.saveSettingsBtn.addEventListener("click", () => {
  saveSettings().catch((err) => showToast(`保存失败: ${err.message}`));
});

els.cancelDeleteGroupBtn.addEventListener("click", () => {
  state.pendingDeleteGroupId = null;
  closeModal(els.deleteGroupModal);
});

els.confirmDeleteGroupBtn.addEventListener("click", () => {
  deletePendingGroup().catch((err) => showToast(`删除失败: ${err.message}`));
});

els.refreshLogsBtn.addEventListener("click", () => {
  refreshLogs().catch((err) => showToast(`日志刷新失败: ${err.message}`));
});

els.clearLogsBtn.addEventListener("click", async () => {
  await window.proxyApp.clearLogs();
  await refreshLogs();
  showToast("日志已清空");
});

(async function init() {
  state.config = await window.proxyApp.getConfig();
  state.status = await window.proxyApp.getStatus();
  renderAll();
  await refreshLogs();

  setInterval(() => {
    refreshStatus().catch(() => {
      // ignore interval refresh errors
    });
  }, 3000);

  setInterval(() => {
    refreshLogs().catch(() => {
      // ignore interval refresh errors
    });
  }, 3000);
})();
