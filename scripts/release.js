#!/usr/bin/env node

const fs = require("node:fs")
const path = require("node:path")
const { spawnSync } = require("node:child_process")

const rootDir = path.resolve(__dirname, "..")
const packageJsonPath = path.join(rootDir, "package.json")
const cliPackageJsonPath = path.join(rootDir, "packages", "aor-cli", "package.json")
const packageLockPath = path.join(rootDir, "package-lock.json")
const cargoTomlPath = path.join(rootDir, "src-tauri", "Cargo.toml")
const tauriConfigPath = path.join(rootDir, "src-tauri", "tauri.conf.json")
const changelogPath = path.join(rootDir, "CHANGELOG.md")

const args = process.argv.slice(2)

function parseArgs(argv) {
  const out = {
    bump: "auto",
    fromTag: null,
    targetVersion: null,
    dryRun: false,
    commitsStdin: false,
  }

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i]
    if (token === "--bump") {
      out.bump = argv[i + 1]
      i += 1
    } else if (token === "--from-tag") {
      out.fromTag = argv[i + 1]
      i += 1
    } else if (token === "--version") {
      out.targetVersion = argv[i + 1]
      i += 1
    } else if (token === "--dry-run") {
      out.dryRun = true
    } else if (token === "--commits-stdin") {
      out.commitsStdin = true
    } else {
      throw new Error(`Unknown argument: ${token}`)
    }
  }

  return out
}

function runGit(args) {
  const result = spawnSync("git", args, {
    cwd: rootDir,
    encoding: "utf8",
  })

  if (result.error) {
    throw result.error
  }
  if (result.status !== 0) {
    const stderr = result.stderr ? result.stderr.trim() : "unknown git error"
    throw new Error(stderr)
  }
  return (result.stdout || "").trim()
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"))
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8")
}

function readPackageVersion() {
  return readJson(packageJsonPath).version
}

function readCargoVersion() {
  const content = fs.readFileSync(cargoTomlPath, "utf8")
  const match = content.match(/\[package][\s\S]*?\nversion\s*=\s*"([^"]+)"/)
  if (!match) {
    throw new Error("Could not parse version from src-tauri/Cargo.toml")
  }
  return match[1]
}

function validateSemver(version) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error(`Invalid semver version: ${version}`)
  }
}

function parseSemver(version) {
  validateSemver(version)
  const [major, minor, patch] = version.split(".").map(v => Number(v))
  return { major, minor, patch }
}

function bumpVersion(current, bump) {
  const parts = parseSemver(current)

  if (bump === "major") {
    return `${parts.major + 1}.0.0`
  }
  if (bump === "minor") {
    return `${parts.major}.${parts.minor + 1}.0`
  }
  if (bump === "patch") {
    return `${parts.major}.${parts.minor}.${parts.patch + 1}`
  }

  throw new Error(`Unsupported bump type: ${bump}`)
}

function getLastTag() {
  try {
    return runGit(["describe", "--tags", "--abbrev=0"])
  } catch {
    return null
  }
}

function getCommits(fromTag) {
  const range = fromTag ? `${fromTag}..HEAD` : "HEAD"
  const output = runGit(["log", range, "--no-merges", "--pretty=format:%H%x1f%s%x1f%b%x1e"])
  if (!output) {
    return []
  }

  return parseCommits(output)
}

function parseCommits(rawLog) {
  return rawLog
    .split("\x1e")
    .map(item => item.trim())
    .filter(Boolean)
    .map(item => {
      const [hash = "", subject = "", body = ""] = item.split("\x1f")
      return {
        hash,
        subject: subject.trim(),
        body: body.trim(),
      }
    })
}

function parseConventionalCommit(subject) {
  const match = subject.match(/^([a-z]+)(?:\(([^)]+)\))?(!)?:\s+(.+)$/i)
  if (!match) {
    return {
      type: "other",
      scope: null,
      breaking: false,
      description: subject,
    }
  }

  return {
    type: match[1].toLowerCase(),
    scope: match[2] || null,
    breaking: Boolean(match[3]),
    description: match[4],
  }
}

function detectBump(commits) {
  let level = "patch"

  for (const commit of commits) {
    const parsed = parseConventionalCommit(commit.subject)
    const hasBreaking = parsed.breaking || /BREAKING CHANGE:/i.test(commit.body)
    if (hasBreaking) {
      return "major"
    }
    if (parsed.type === "feat") {
      level = "minor"
    }
  }

  return level
}

function renderCommitLine(commit) {
  const parsed = parseConventionalCommit(commit.subject)
  const scopeText = parsed.scope ? `(${parsed.scope})` : ""
  const shortHash = commit.hash.slice(0, 7)
  return `- ${parsed.type}${scopeText}: ${parsed.description} (${shortHash})`
}

function renderChangelogSection(version, commits, dateText) {
  const groups = {
    breaking: [],
    feat: [],
    fix: [],
    maintenance: [],
  }

  for (const commit of commits) {
    const parsed = parseConventionalCommit(commit.subject)
    const hasBreaking = parsed.breaking || /BREAKING CHANGE:/i.test(commit.body)
    if (hasBreaking) {
      groups.breaking.push(renderCommitLine(commit))
      continue
    }
    if (parsed.type === "feat") {
      groups.feat.push(renderCommitLine(commit))
      continue
    }
    if (parsed.type === "fix") {
      groups.fix.push(renderCommitLine(commit))
      continue
    }
    groups.maintenance.push(renderCommitLine(commit))
  }

  const parts = [`## v${version} - ${dateText}`]

  if (groups.breaking.length > 0) {
    parts.push("### Breaking Changes")
    parts.push(...groups.breaking)
  }
  if (groups.feat.length > 0) {
    parts.push("### Features")
    parts.push(...groups.feat)
  }
  if (groups.fix.length > 0) {
    parts.push("### Fixes")
    parts.push(...groups.fix)
  }
  if (groups.maintenance.length > 0 || commits.length === 0) {
    parts.push("### Maintenance")
    if (groups.maintenance.length > 0) {
      parts.push(...groups.maintenance)
    } else {
      parts.push("- Internal maintenance")
    }
  }

  return parts.join("\n")
}

function updateCargoVersion(content, version) {
  const next = content.replace(/(\[package][\s\S]*?\nversion\s*=\s*")([^"]+)(")/, `$1${version}$3`)
  if (next === content) {
    throw new Error("Failed to update version in src-tauri/Cargo.toml")
  }
  return next
}

function stripChangelogHeader(content) {
  return content
    .replace(/^# Changelog\s*/i, "")
    .replace(/^All notable changes to this project will be documented in this file\.\s*/i, "")
    .trim()
}

function updateChangelog(section) {
  const existing = fs.existsSync(changelogPath) ? fs.readFileSync(changelogPath, "utf8") : ""
  const history = stripChangelogHeader(existing)
  const header =
    "# Changelog\n\nAll notable changes to this project will be documented in this file."

  const blocks = [header, section]
  if (history) {
    blocks.push(history)
  }
  return `${blocks.join("\n\n")}\n`
}

function writeVersionFiles(nextVersion, changelogSection, dryRun) {
  const packageJson = readJson(packageJsonPath)
  const cliPackageJson = readJson(cliPackageJsonPath)
  const tauriConfig = readJson(tauriConfigPath)
  const cargoToml = fs.readFileSync(cargoTomlPath, "utf8")

  packageJson.version = nextVersion
  cliPackageJson.version = nextVersion
  tauriConfig.version = nextVersion

  const nextCargoToml = updateCargoVersion(cargoToml, nextVersion)
  const nextChangelog = updateChangelog(changelogSection)

  if (dryRun) {
    return
  }

  writeJson(packageJsonPath, packageJson)
  writeJson(cliPackageJsonPath, cliPackageJson)
  writeJson(tauriConfigPath, tauriConfig)
  fs.writeFileSync(cargoTomlPath, nextCargoToml, "utf8")
  fs.writeFileSync(changelogPath, nextChangelog, "utf8")

  if (fs.existsSync(packageLockPath)) {
    const packageLock = readJson(packageLockPath)
    packageLock.version = nextVersion
    if (packageLock.packages?.[""]) {
      packageLock.packages[""].version = nextVersion
    }
    writeJson(packageLockPath, packageLock)
  }
}

function main() {
  const options = parseArgs(args)

  const packageVersion = readPackageVersion()
  const cliPackageVersion = readJson(cliPackageJsonPath).version
  const cargoVersion = readCargoVersion()
  const tauriVersion = readJson(tauriConfigPath).version
  const versions = new Set([packageVersion, cliPackageVersion, cargoVersion, tauriVersion])
  if (versions.size !== 1) {
    throw new Error(
      `Version mismatch before release preparation: package=${packageVersion}, cli=${cliPackageVersion}, cargo=${cargoVersion}, tauri=${tauriVersion}`
    )
  }

  const currentVersion = packageVersion
  const fromTag = options.fromTag || (options.commitsStdin ? null : getLastTag())
  const commits = options.commitsStdin
    ? parseCommits(fs.readFileSync(0, "utf8"))
    : getCommits(fromTag)

  let nextVersion = options.targetVersion
  if (nextVersion) {
    validateSemver(nextVersion)
  } else {
    const bump = options.bump || "auto"
    if (!["auto", "patch", "minor", "major"].includes(bump)) {
      throw new Error(`Invalid bump type: ${bump}`)
    }
    if (bump === "auto" && commits.length === 0) {
      throw new Error("No commits found since the last tag. Use --version or --bump explicitly.")
    }

    const resolvedBump = bump === "auto" ? detectBump(commits) : bump
    nextVersion = bumpVersion(currentVersion, resolvedBump)
  }

  if (nextVersion === currentVersion) {
    throw new Error(`Version unchanged (${currentVersion}). Refusing to continue.`)
  }

  const dateText = new Date().toISOString().slice(0, 10)
  const section = renderChangelogSection(nextVersion, commits, dateText)

  writeVersionFiles(nextVersion, section, options.dryRun)

  if (process.env.GITHUB_OUTPUT) {
    fs.appendFileSync(process.env.GITHUB_OUTPUT, `version=${nextVersion}\n`, "utf8")
  }

  console.log(`[release] current version: ${currentVersion}`)
  console.log(`[release] next version: ${nextVersion}`)
  if (fromTag) {
    console.log(`[release] commits analyzed: ${fromTag}..HEAD (${commits.length})`)
  } else {
    console.log(`[release] commits analyzed: HEAD (${commits.length})`)
  }
  if (options.dryRun) {
    console.log("[release] dry run only. no files written.")
  } else {
    console.log(
      "[release] updated files: package.json, packages/aor-cli/package.json, package-lock.json, src-tauri/Cargo.toml, src-tauri/tauri.conf.json, CHANGELOG.md"
    )
  }
}

main()
