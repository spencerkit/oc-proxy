const fs = require("node:fs");
const path = require("node:path");
const { EventEmitter } = require("node:events");
const { getDefaultConfig } = require("./defaultConfig");
const { validateConfig } = require("./schema");

class ConfigStore extends EventEmitter {
  constructor(filePath) {
    super();
    this.filePath = filePath;
    this.config = null;
  }

  initialize() {
    const dir = path.dirname(this.filePath);
    fs.mkdirSync(dir, { recursive: true });

    const resetToDefault = () => {
      const defaultConfig = getDefaultConfig();
      fs.writeFileSync(this.filePath, JSON.stringify(defaultConfig, null, 2), "utf-8");
      this.config = defaultConfig;
    };

    if (!fs.existsSync(this.filePath)) {
      resetToDefault();
      return;
    }

    let parsed;
    try {
      const raw = fs.readFileSync(this.filePath, "utf-8");
      parsed = JSON.parse(raw);
    } catch {
      resetToDefault();
      return;
    }

    const normalized = this.normalizeConfig(parsed);
    const result = validateConfig(normalized);
    if (!result.valid) {
      resetToDefault();
      return;
    }

    this.config = normalized;
  }

  get() {
    return JSON.parse(JSON.stringify(this.config));
  }

  save(nextConfig) {
    const normalized = this.normalizeConfig(nextConfig);
    const result = validateConfig(normalized);
    if (!result.valid) {
      const err = new Error(`Config validation failed: ${result.errors.join("; ")}`);
      err.statusCode = 400;
      err.details = result.errors;
      throw err;
    }
    this.config = JSON.parse(JSON.stringify(normalized));
    fs.writeFileSync(this.filePath, JSON.stringify(this.config, null, 2), "utf-8");
    this.emit("updated", this.get());
    return this.get();
  }

  normalizeConfig(input) {
    const defaults = getDefaultConfig();
    const source = input && typeof input === "object" ? input : {};

    return {
      ...defaults,
      ...source,
      server: {
        ...defaults.server,
        ...(source.server || {})
      },
      compat: {
        ...defaults.compat,
        ...(source.compat || {})
      },
      ui: {
        ...defaults.ui,
        ...(source.ui || {}),
        launchOnStartup: !!(source.ui && source.ui.launchOnStartup)
      },
      logging: {
        ...defaults.logging,
        ...(source.logging || {})
      },
      groups: Array.isArray(source.groups) ? source.groups : defaults.groups
    };
  }
}

module.exports = {
  ConfigStore
};
