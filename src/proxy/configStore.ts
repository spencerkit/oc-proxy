// @ts-nocheck
const fs = require("node:fs")
const path = require("node:path")
const { EventEmitter } = require("node:events")
const { getDefaultConfig } = require("./defaultConfig")
const { validateConfig } = require("./schema")

class ConfigStore extends EventEmitter {
  constructor(filePath) {
    super()
    this.filePath = filePath
    this.config = null
  }

  initialize() {
    const dir = path.dirname(this.filePath)
    fs.mkdirSync(dir, { recursive: true })

    const resetToDefault = () => {
      const defaultConfig = getDefaultConfig()
      fs.writeFileSync(this.filePath, JSON.stringify(defaultConfig, null, 2), "utf-8")
      this.config = defaultConfig
    }

    if (!fs.existsSync(this.filePath)) {
      resetToDefault()
      return
    }

    let parsed = null
    try {
      const raw = fs.readFileSync(this.filePath, "utf-8")
      parsed = JSON.parse(raw)
    } catch {
      resetToDefault()
      return
    }

    const normalized = this.normalizeConfig(parsed)
    const result = validateConfig(normalized)
    if (!result.valid) {
      resetToDefault()
      return
    }

    this.config = normalized
  }

  get() {
    return JSON.parse(JSON.stringify(this.config))
  }

  save(nextConfig) {
    const normalized = this.normalizeConfig(nextConfig)
    const result = validateConfig(normalized)
    if (!result.valid) {
      const err = new Error(`Config validation failed: ${result.errors.join("; ")}`)
      err.statusCode = 400
      err.details = result.errors
      throw err
    }
    this.config = JSON.parse(JSON.stringify(normalized))
    fs.writeFileSync(this.filePath, JSON.stringify(this.config, null, 2), "utf-8")
    this.emit("updated", this.get())
    return this.get()
  }

  normalizeConfig(input) {
    const defaults = getDefaultConfig()
    const source = input && typeof input === "object" ? input : {}

    return {
      ...defaults,
      ...source,
      server: {
        ...defaults.server,
        ...(source.server || {}),
        host: defaults.server.host,
      },
      compat: {
        ...defaults.compat,
        ...(source.compat || {}),
      },
      ui: {
        ...defaults.ui,
        ...(source.ui || {}),
        locale: source.ui && source.ui.locale === "zh-CN" ? "zh-CN" : "en-US",
        localeMode:
          source.ui && Object.hasOwn(source.ui, "localeMode")
            ? source.ui.localeMode === "manual"
              ? "manual"
              : "auto"
            : source.ui && source.ui.locale === "zh-CN"
              ? "manual"
              : "auto",
        launchOnStartup: !!source.ui?.launchOnStartup,
        closeToTray:
          source.ui && Object.hasOwn(source.ui, "closeToTray")
            ? !!source.ui.closeToTray
            : defaults.ui.closeToTray,
      },
      logging: {
        ...defaults.logging,
        ...(source.logging || {}),
        captureBody:
          source.logging && Object.hasOwn(source.logging, "captureBody")
            ? !!source.logging.captureBody
            : defaults.logging.captureBody,
        redactRules:
          source.logging &&
          Array.isArray(source.logging.redactRules) &&
          source.logging.redactRules.every(v => typeof v === "string")
            ? source.logging.redactRules
            : defaults.logging.redactRules,
      },
      groups: Array.isArray(source.groups) ? source.groups : defaults.groups,
    }
  }
}

module.exports = {
  ConfigStore,
}
