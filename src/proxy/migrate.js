const { getDefaultConfig } = require("./defaultConfig");

function normalizeBaseUrl(baseURL) {
  if (!baseURL || typeof baseURL !== "string") return "";
  return baseURL.replace(/\/+$/, "");
}

function isObject(value) {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function isStockLegacyTemplate(oldConfig) {
  if (!oldConfig || !Array.isArray(oldConfig.providers) || !Array.isArray(oldConfig.models)) {
    return false;
  }
  if (oldConfig.providers.length !== 2 || oldConfig.models.length !== 2) {
    return false;
  }

  const modelNames = new Set(oldConfig.models.map((m) => m?.name));
  const providerIds = new Set(oldConfig.providers.map((p) => p?.id));
  const allEmptyKeys = oldConfig.providers.every((p) => !p?.apiKey);

  return modelNames.has("claude-3-5-sonnet")
    && modelNames.has("gpt-4o-mini")
    && providerIds.has("anthropic-default")
    && providerIds.has("openai-default")
    && allEmptyKeys;
}

function isStockAutoGroups(config) {
  if (!config || !Array.isArray(config.groups) || config.groups.length !== 2) {
    return false;
  }

  const pathSet = new Set(config.groups.map((g) => g?.path));
  if (!pathSet.has("claude-3-5-sonnet") || !pathSet.has("gpt-4o-mini")) {
    return false;
  }

  return config.groups.every((group) => {
    if (!group || !Array.isArray(group.rules) || group.rules.length !== 1) return false;
    const rule = group.rules[0];
    if (!rule || rule.id !== group.activeRuleId) return false;
    return !rule.token;
  });
}

function isStockSeedGroup(group) {
  if (!group || !Array.isArray(group.rules) || group.rules.length !== 1) {
    return false;
  }
  const rule = group.rules[0];
  if (!rule || group.activeRuleId !== rule.id) {
    return false;
  }

  const isClaudeSeed = group.id === "group_claude-3-5-sonnet"
    && group.path === "claude-3-5-sonnet"
    && rule.model === "claude-3-5-sonnet-latest"
    && rule.apiAddress === "https://api.anthropic.com"
    && rule.direction === "oc"
    && !rule.token;

  const isGptSeed = group.id === "group_gpt-4o-mini"
    && group.path === "gpt-4o-mini"
    && rule.model === "gpt-4o-mini"
    && rule.apiAddress === "https://api.openai.com"
    && rule.direction === "co"
    && !rule.token;

  return isClaudeSeed || isGptSeed;
}

function pruneStockSeedGroups(config) {
  if (!config || !Array.isArray(config.groups)) {
    return { changed: false, config };
  }
  const before = config.groups.length;
  config.groups = config.groups.filter((group) => !isStockSeedGroup(group));
  return {
    changed: config.groups.length !== before,
    config
  };
}

function migrateLegacyConfig(oldConfig) {
  const next = getDefaultConfig();

  if (oldConfig && oldConfig.server) {
    next.server.host = oldConfig.server.host || next.server.host;
    next.server.port = Number.isInteger(oldConfig.server.port) ? oldConfig.server.port : next.server.port;
    next.server.authEnabled = !!oldConfig.server.authEnabled;
    next.server.localBearerToken = oldConfig.server.localBearerToken || "";
  }

  if (oldConfig && oldConfig.compat && typeof oldConfig.compat.strictMode === "boolean") {
    next.compat.strictMode = oldConfig.compat.strictMode;
  }

  if (oldConfig && isObject(oldConfig.ui)) {
    if (["light", "dark"].includes(oldConfig.ui.theme)) {
      next.ui.theme = oldConfig.ui.theme;
    }
    if (["en-US", "zh-CN"].includes(oldConfig.ui.locale)) {
      next.ui.locale = oldConfig.ui.locale;
    }
    if (typeof oldConfig.ui.launchOnStartup === "boolean") {
      next.ui.launchOnStartup = oldConfig.ui.launchOnStartup;
    }
  }

  if (oldConfig && oldConfig.logging) {
    if (typeof oldConfig.logging.captureBody === "boolean") {
      next.logging.captureBody = oldConfig.logging.captureBody;
    }
    if (Array.isArray(oldConfig.logging.redactRules)) {
      next.logging.redactRules = oldConfig.logging.redactRules.slice();
    }
  }

  if (oldConfig && Array.isArray(oldConfig.groups)) {
    next.groups = JSON.parse(JSON.stringify(oldConfig.groups));
  }

  if (!oldConfig || !Array.isArray(oldConfig.models) || !Array.isArray(oldConfig.providers)) {
    return next;
  }

  if (isStockLegacyTemplate(oldConfig)) {
    return next;
  }

  const providers = new Map(oldConfig.providers.map((p) => [p.id, p]));
  const groups = [];

  for (const model of oldConfig.models) {
    if (!model || !model.provider) continue;
    const provider = providers.get(model.provider);
    if (!provider) continue;

    const ruleId = `rule_${Math.random().toString(36).slice(2, 8)}`;
    const groupId = `group_${model.name || Math.random().toString(36).slice(2, 8)}`;
    const safePath = String(model.name || "default").replace(/[^a-zA-Z0-9_-]/g, "_");

    groups.push({
      id: groupId,
      name: model.name || "default",
      path: safePath || "default",
      activeRuleId: ruleId,
      rules: [
        {
          id: ruleId,
          model: model.upstreamModel || model.name || "model-default",
          token: provider.apiKey || "",
          apiAddress: normalizeBaseUrl(provider.baseURL) || "https://api.anthropic.com",
          direction: provider.protocol === "openai" ? "co" : "oc"
        }
      ]
    });
  }

  if (groups.length > 0) {
    next.groups = groups;
  }

  return next;
}

module.exports = {
  migrateLegacyConfig,
  isStockAutoGroups,
  pruneStockSeedGroups
};
