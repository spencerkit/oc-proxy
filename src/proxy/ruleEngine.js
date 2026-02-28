function deepClone(v) {
  return JSON.parse(JSON.stringify(v));
}

function getByPath(obj, path) {
  if (!path) return undefined;
  return String(path)
    .split(".")
    .reduce((acc, seg) => (acc == null ? undefined : acc[seg]), obj);
}

function setByPath(obj, path, value) {
  const parts = String(path).split(".");
  let cur = obj;
  for (let i = 0; i < parts.length - 1; i += 1) {
    const key = parts[i];
    if (!cur[key] || typeof cur[key] !== "object") {
      cur[key] = {};
    }
    cur = cur[key];
  }
  cur[parts[parts.length - 1]] = value;
}

function deleteByPath(obj, path) {
  const parts = String(path).split(".");
  let cur = obj;
  for (let i = 0; i < parts.length - 1; i += 1) {
    cur = cur ? cur[parts[i]] : undefined;
  }
  if (cur && Object.prototype.hasOwnProperty.call(cur, parts[parts.length - 1])) {
    delete cur[parts[parts.length - 1]];
  }
}

function wildcardMatch(value, matcher) {
  if (matcher == null) return true;
  if (Array.isArray(matcher)) {
    return matcher.some((m) => wildcardMatch(value, m));
  }
  const escaped = String(matcher).replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*");
  return new RegExp(`^${escaped}$`, "i").test(String(value || ""));
}

function predicateMatch(body, predicate) {
  const actual = getByPath(body, predicate.path);
  const expected = predicate.value;
  switch (predicate.op) {
    case "eq":
      return actual === expected;
    case "neq":
      return actual !== expected;
    case "exists":
      return actual !== undefined;
    case "contains":
      if (Array.isArray(actual)) return actual.includes(expected);
      if (typeof actual === "string") return actual.includes(String(expected));
      return false;
    case "in":
      return Array.isArray(expected) && expected.includes(actual);
    default:
      return false;
  }
}

function interpolate(template, vars) {
  return String(template).replace(/\$\{([^}]+)\}/g, (_, key) => {
    return vars[key] == null ? "" : String(vars[key]);
  });
}

function matchesRule(rule, context) {
  if (!rule || rule.enabled === false) return false;
  const match = rule.match || {};

  if (match.entryProtocol && match.entryProtocol !== context.entryProtocol) {
    return false;
  }

  if (match.path && !wildcardMatch(context.path, match.path)) {
    return false;
  }

  if (match.model && !wildcardMatch(context.requestedModel, match.model)) {
    return false;
  }

  if (match.headers && typeof match.headers === "object") {
    const headers = context.headers || {};
    for (const [name, expected] of Object.entries(match.headers)) {
      const actual = headers[name.toLowerCase()];
      if (!wildcardMatch(actual, expected)) {
        return false;
      }
    }
  }

  if (Array.isArray(match.bodyPredicates)) {
    for (const predicate of match.bodyPredicates) {
      if (!predicateMatch(context.body, predicate)) {
        return false;
      }
    }
  }

  return true;
}

function applyRewrite(body, rewrite, vars) {
  if (!rewrite) return body;
  const next = deepClone(body);
  if (rewrite.set && typeof rewrite.set === "object") {
    for (const [path, value] of Object.entries(rewrite.set)) {
      const resolved = typeof value === "string" ? interpolate(value, vars) : value;
      setByPath(next, path, resolved);
    }
  }
  if (Array.isArray(rewrite.remove)) {
    for (const path of rewrite.remove) {
      deleteByPath(next, path);
    }
  }
  return next;
}

function resolveRoute(config, context) {
  const headers = {};
  for (const [k, v] of Object.entries(context.headers || {})) {
    headers[k.toLowerCase()] = v;
  }

  const modelConfig = (config.models || []).find((m) => m.name === context.requestedModel);
  const providersById = new Map((config.providers || []).map((p) => [p.id, p]));

  let targetProviderId = modelConfig ? modelConfig.provider : (config.providers?.[0]?.id || null);
  let targetModel = modelConfig ? modelConfig.upstreamModel : context.requestedModel;
  let rewrittenBody = deepClone(context.body);
  let injectedHeaders = {};
  let matchedRule = null;

  const sortedRules = (config.rules || [])
    .filter((r) => r && r.enabled !== false)
    .sort((a, b) => (Number(b.priority || 0) - Number(a.priority || 0)));

  for (const rule of sortedRules) {
    if (!matchesRule(rule, { ...context, headers, body: rewrittenBody })) {
      continue;
    }
    matchedRule = rule;
    const vars = {
      requestedModel: context.requestedModel || "",
      targetModel,
      path: context.path,
      entryProtocol: context.entryProtocol,
      traceId: context.traceId || ""
    };
    const action = rule.action || {};
    if (action.targetProvider) targetProviderId = action.targetProvider;
    if (action.targetModel) targetModel = action.targetModel;
    rewrittenBody = applyRewrite(rewrittenBody, action.rewrite, vars);
    if (action.injectHeaders && typeof action.injectHeaders === "object") {
      for (const [key, value] of Object.entries(action.injectHeaders)) {
        injectedHeaders[key] = typeof value === "string" ? interpolate(value, vars) : String(value);
      }
    }
    break;
  }

  const provider = providersById.get(targetProviderId);
  if (!provider) {
    return {
      error: `No provider found for targetProvider=${targetProviderId}`
    };
  }

  return {
    provider,
    targetModel,
    body: rewrittenBody,
    injectedHeaders,
    matchedRule
  };
}

module.exports = {
  resolveRoute,
  matchesRule,
  applyRewrite
};
