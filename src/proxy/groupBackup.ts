// @ts-nocheck
function isObject(value) {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function createGroupsBackupPayload(groups) {
  return {
    format: "oa-proxy-groups-backup",
    version: 1,
    exportedAt: new Date().toISOString(),
    groups: clone(Array.isArray(groups) ? groups : [])
  };
}

function extractGroupsFromImportPayload(input) {
  if (Array.isArray(input)) {
    return clone(input);
  }

  if (isObject(input) && Array.isArray(input.groups)) {
    return clone(input.groups);
  }

  if (isObject(input) && isObject(input.config) && Array.isArray(input.config.groups)) {
    return clone(input.config.groups);
  }

  const err = new Error("Invalid import JSON: expected a groups array");
  err.statusCode = 400;
  throw err;
}

module.exports = {
  createGroupsBackupPayload,
  extractGroupsFromImportPayload
};
