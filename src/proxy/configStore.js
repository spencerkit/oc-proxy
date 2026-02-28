const fs = require("node:fs");
const path = require("node:path");
const { EventEmitter } = require("node:events");
const { getDefaultConfig } = require("./defaultConfig");
const { validateConfig } = require("./schema");
const { migrateLegacyConfig, isStockAutoGroups, pruneStockSeedGroups } = require("./migrate");

class ConfigStore extends EventEmitter {
  constructor(filePath) {
    super();
    this.filePath = filePath;
    this.config = null;
  }

  initialize() {
    const dir = path.dirname(this.filePath);
    fs.mkdirSync(dir, { recursive: true });

    if (!fs.existsSync(this.filePath)) {
      const defaultConfig = getDefaultConfig();
      fs.writeFileSync(this.filePath, JSON.stringify(defaultConfig, null, 2), "utf-8");
      this.config = defaultConfig;
      return;
    }

    const raw = fs.readFileSync(this.filePath, "utf-8");
    const parsed = JSON.parse(raw);
    let result = validateConfig(parsed);

    if (!result.valid) {
      const migrated = migrateLegacyConfig(parsed);
      result = validateConfig(migrated);
      if (!result.valid) {
        throw new Error(`Invalid config at ${this.filePath}: ${result.errors.join("; ")}`);
      }
      fs.writeFileSync(this.filePath, JSON.stringify(migrated, null, 2), "utf-8");
      this.config = migrated;
      return;
    }

    if (isStockAutoGroups(parsed)) {
      parsed.groups = [];
      fs.writeFileSync(this.filePath, JSON.stringify(parsed, null, 2), "utf-8");
    }
    const pruned = pruneStockSeedGroups(parsed);
    if (pruned.changed) {
      fs.writeFileSync(this.filePath, JSON.stringify(pruned.config, null, 2), "utf-8");
    }
    this.config = parsed;
  }

  get() {
    return JSON.parse(JSON.stringify(this.config));
  }

  save(nextConfig) {
    const result = validateConfig(nextConfig);
    if (!result.valid) {
      const err = new Error(`Config validation failed: ${result.errors.join("; ")}`);
      err.statusCode = 400;
      err.details = result.errors;
      throw err;
    }
    this.config = JSON.parse(JSON.stringify(nextConfig));
    fs.writeFileSync(this.filePath, JSON.stringify(this.config, null, 2), "utf-8");
    this.emit("updated", this.get());
    return this.get();
  }
}

module.exports = {
  ConfigStore
};
