#!/usr/bin/env node

const fs = require("node:fs")
const path = require("node:path")

const rootDir = path.resolve(__dirname, "..")
const packageJsonPath = path.join(rootDir, "package.json")
const cliPackageJsonPath = path.join(rootDir, "packages", "aor-cli", "package.json")
const cargoTomlPath = path.join(rootDir, "src-tauri", "Cargo.toml")
const tauriConfigPath = path.join(rootDir, "src-tauri", "tauri.conf.json")

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"))
}

function readCargoVersion(filePath) {
  const content = fs.readFileSync(filePath, "utf8")
  const match = content.match(/\[package][\s\S]*?\nversion\s*=\s*"([^"]+)"/)
  if (!match) {
    throw new Error(`Could not read package version from ${filePath}`)
  }
  return match[1]
}

const packageVersion = readJson(packageJsonPath).version
const cliPackageVersion = readJson(cliPackageJsonPath).version
const cargoVersion = readCargoVersion(cargoTomlPath)
const tauriVersion = readJson(tauriConfigPath).version

const versions = [
  { file: "package.json", value: packageVersion },
  { file: "packages/aor-cli/package.json", value: cliPackageVersion },
  { file: "src-tauri/Cargo.toml", value: cargoVersion },
  { file: "src-tauri/tauri.conf.json", value: tauriVersion },
]

const uniqueVersions = Array.from(new Set(versions.map(entry => entry.value)))

if (uniqueVersions.length === 1) {
  console.log(`[version:check] versions are in sync: ${uniqueVersions[0]}`)
  process.exit(0)
}

console.error("[version:check] version mismatch detected:")
for (const { file, value } of versions) {
  console.error(`- ${file}: ${value}`)
}
process.exit(1)
