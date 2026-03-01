function isObject(v) {
  return !!v && typeof v === "object" && !Array.isArray(v);
}

function isNonEmptyString(v) {
  return typeof v === "string" && v.trim().length > 0;
}

function validateConfig(config) {
  const errors = [];

  if (!isObject(config)) {
    return { valid: false, errors: ["Config must be an object"] };
  }

  if (!isObject(config.server)) {
    errors.push("server must be an object");
  } else {
    if (!isNonEmptyString(config.server.host)) {
      errors.push("server.host must be a non-empty string");
    }
    if (!Number.isInteger(config.server.port) || config.server.port < 1 || config.server.port > 65535) {
      errors.push("server.port must be an integer between 1 and 65535");
    }
    if (typeof config.server.authEnabled !== "boolean") {
      errors.push("server.authEnabled must be boolean");
    }
    if (config.server.authEnabled && !isNonEmptyString(config.server.localBearerToken)) {
      errors.push("server.localBearerToken must be set when authEnabled=true");
    }
  }

  if (!isObject(config.compat) || typeof config.compat.strictMode !== "boolean") {
    errors.push("compat.strictMode must be boolean");
  }

  if (!isObject(config.ui)) {
    errors.push("ui must be an object");
  } else {
    if (!["light", "dark"].includes(config.ui.theme)) {
      errors.push("ui.theme must be light|dark");
    }
    if (!["en-US", "zh-CN"].includes(config.ui.locale)) {
      errors.push("ui.locale must be en-US|zh-CN");
    }
    if (typeof config.ui.launchOnStartup !== "boolean") {
      errors.push("ui.launchOnStartup must be boolean");
    }
  }

  if (!isObject(config.logging)) {
    errors.push("logging must be an object");
  } else {
    if (typeof config.logging.captureBody !== "boolean") {
      errors.push("logging.captureBody must be boolean");
    }
    if (!Array.isArray(config.logging.redactRules) || !config.logging.redactRules.every((v) => typeof v === "string")) {
      errors.push("logging.redactRules must be string[]");
    }
  }

  if (!Array.isArray(config.groups)) {
    errors.push("groups must be an array");
  } else {
    const seenGroupId = new Set();
    const seenGroupPath = new Set();

    for (const group of config.groups) {
      if (!isObject(group)) {
        errors.push("group entry must be object");
        continue;
      }

      if (!isNonEmptyString(group.id)) {
        errors.push("group.id is required");
      } else if (seenGroupId.has(group.id)) {
        errors.push(`duplicate group.id: ${group.id}`);
      } else {
        seenGroupId.add(group.id);
      }

      if (!isNonEmptyString(group.name)) {
        errors.push(`group.name is required for ${group.id || "unknown"}`);
      }

      if (!isNonEmptyString(group.path)) {
        errors.push(`group.path is required for ${group.id || "unknown"}`);
      } else {
        if (!/^[a-zA-Z0-9_-]+$/.test(group.path)) {
          errors.push(`group.path must match [a-zA-Z0-9_-]+ for ${group.id || "unknown"}`);
        }
        if (seenGroupPath.has(group.path)) {
          errors.push(`duplicate group.path: ${group.path}`);
        } else {
          seenGroupPath.add(group.path);
        }
      }

      if (!Array.isArray(group.rules)) {
        errors.push(`group.rules must be an array for ${group.id || "unknown"}`);
        continue;
      }

      const ruleIds = new Set();
      for (const rule of group.rules) {
        if (!isObject(rule)) {
          errors.push(`rule entry must be object in group ${group.id || "unknown"}`);
          continue;
        }

        if (!isNonEmptyString(rule.id)) {
          errors.push(`rule.id is required in group ${group.id || "unknown"}`);
        } else if (ruleIds.has(rule.id)) {
          errors.push(`duplicate rule.id ${rule.id} in group ${group.id || "unknown"}`);
        } else {
          ruleIds.add(rule.id);
        }

        if (typeof rule.model !== "string") {
          errors.push(`rule.model must be string for ${rule.id || "unknown"}`);
        }
        if (typeof rule.token !== "string") {
          errors.push(`rule.token must be string for ${rule.id || "unknown"}`);
        }
        if (typeof rule.apiAddress !== "string") {
          errors.push(`rule.apiAddress must be string for ${rule.id || "unknown"}`);
        }
        if (!["oc", "co"].includes(rule.direction)) {
          errors.push(`rule.direction must be oc|co for ${rule.id || "unknown"}`);
        }
      }

      if (group.activeRuleId != null) {
        if (typeof group.activeRuleId !== "string") {
          errors.push(`group.activeRuleId must be string|null for ${group.id || "unknown"}`);
        } else if (!ruleIds.has(group.activeRuleId)) {
          errors.push(`group.activeRuleId not found in rules for ${group.id || "unknown"}`);
        }
      }
    }
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

module.exports = {
  validateConfig
};
