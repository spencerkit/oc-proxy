// @ts-nocheck
function clone(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
}

function shouldMaskKey(key, rules) {
  const lower = String(key || "").toLowerCase();
  return rules.some((rule) => lower.includes(rule.toLowerCase()));
}

function redactValue(value) {
  if (typeof value !== "string") {
    return "[REDACTED]";
  }
  if (value.length <= 8) {
    return "[REDACTED]";
  }
  return `${value.slice(0, 4)}...[REDACTED]...${value.slice(-2)}`;
}

function walk(node, rules) {
  if (Array.isArray(node)) {
    return node.map((v) => walk(v, rules));
  }
  if (!node || typeof node !== "object") {
    return node;
  }
  const out = {};
  for (const [key, value] of Object.entries(node)) {
    if (shouldMaskKey(key, rules)) {
      out[key] = redactValue(value);
      continue;
    }
    out[key] = walk(value, rules);
  }
  return out;
}

function redactPayload(payload, rules) {
  const safeRules = Array.isArray(rules) ? rules : [];
  return walk(clone(payload), safeRules);
}

module.exports = {
  redactPayload
};
