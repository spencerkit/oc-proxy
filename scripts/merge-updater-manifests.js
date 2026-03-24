#!/usr/bin/env node

const fs = require("node:fs")
const path = require("node:path")

function parseArgs(argv) {
  let inputDir = "release-artifacts"
  let outputPath = path.join(inputDir, "latest.json")
  let repo = process.env.GITHUB_REPOSITORY || ""
  let tag = process.env.GITHUB_REF_NAME || ""
  let version = ""
  let pubDate = new Date().toISOString()

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i]
    if (token === "--input") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --input")
      inputDir = next
      i += 1
      continue
    }
    if (token === "--output") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --output")
      outputPath = next
      i += 1
      continue
    }
    if (token === "--repo") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --repo")
      repo = next
      i += 1
      continue
    }
    if (token === "--tag") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --tag")
      tag = next
      i += 1
      continue
    }
    if (token === "--version") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --version")
      version = next
      i += 1
      continue
    }
    if (token === "--pub-date") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --pub-date")
      pubDate = next
      i += 1
      continue
    }
    throw new Error(`Unknown argument: ${token}`)
  }

  return { inputDir, outputPath, repo, tag, version, pubDate }
}

function readManifest(filePath) {
  const raw = fs.readFileSync(filePath, "utf8")
  const parsed = JSON.parse(raw)
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`Updater manifest must be an object: ${filePath}`)
  }
  if (typeof parsed.version !== "string" || !parsed.version.trim()) {
    throw new Error(`Updater manifest missing version: ${filePath}`)
  }
  if (
    !parsed.platforms ||
    typeof parsed.platforms !== "object" ||
    Array.isArray(parsed.platforms)
  ) {
    throw new Error(`Updater manifest missing platforms map: ${filePath}`)
  }
  return parsed
}

function mergeManifest(base, incoming, filePath) {
  if (base.version !== incoming.version) {
    throw new Error(
      `Updater version mismatch in ${filePath}: ${incoming.version} != ${base.version}`
    )
  }

  if (incoming.notes) {
    if (!base.notes) {
      base.notes = incoming.notes
    } else if (base.notes !== incoming.notes) {
      throw new Error(`Updater notes mismatch in ${filePath}`)
    }
  }

  if (incoming.pub_date) {
    if (!base.pub_date || incoming.pub_date > base.pub_date) {
      base.pub_date = incoming.pub_date
    }
  }

  for (const [platform, value] of Object.entries(incoming.platforms)) {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new Error(`Updater platform entry must be an object: ${filePath} (${platform})`)
    }
    if (!base.platforms[platform]) {
      base.platforms[platform] = value
      continue
    }
    if (JSON.stringify(base.platforms[platform]) !== JSON.stringify(value)) {
      throw new Error(`Updater platform entry mismatch in ${filePath} (${platform})`)
    }
  }
}

function collectManifestPaths(rootDir) {
  const paths = []

  function visit(currentDir) {
    for (const entry of fs.readdirSync(currentDir, { withFileTypes: true })) {
      const nextPath = path.join(currentDir, entry.name)
      if (entry.isDirectory()) {
        visit(nextPath)
        continue
      }
      if (/^latest-[^.]+\.json$/.test(entry.name)) {
        paths.push(nextPath)
      }
    }
  }

  visit(rootDir)
  paths.sort()
  return paths
}

function collectFiles(rootDir) {
  const files = []

  function visit(currentDir) {
    for (const entry of fs.readdirSync(currentDir, { withFileTypes: true })) {
      const nextPath = path.join(currentDir, entry.name)
      if (entry.isDirectory()) {
        visit(nextPath)
        continue
      }
      files.push(nextPath)
    }
  }

  visit(rootDir)
  files.sort()
  return files
}

function normalizeArch(rawArch) {
  if (!rawArch) return null

  switch (rawArch.toLowerCase()) {
    case "x64":
    case "x86_64":
    case "amd64":
      return "x86_64"
    case "x86":
    case "i386":
    case "i686":
      return "i686"
    case "arm64":
    case "aarch64":
      return "aarch64"
    case "armv7":
    case "armhf":
      return "armv7"
    case "riscv64":
      return "riscv64"
    default:
      return null
  }
}

function parseMacArchFromDmg(fileName) {
  const match = fileName.match(/_(x64|aarch64|arm64|universal)\.dmg$/i)
  if (!match) return null
  return normalizeArch(match[1])
}

function parseWindowsUpdater(fileName) {
  const match = fileName.match(/_(x64|x86|arm64)(?:-setup)?\.(exe|msi)(?:\.zip)?$/i)
  if (!match) return null

  return {
    os: "windows",
    arch: normalizeArch(match[1]),
    installer: match[2].toLowerCase() === "msi" ? "msi" : "nsis",
  }
}

function parseLinuxUpdater(fileName) {
  const match = fileName.match(
    /_(amd64|x86_64|x86|i386|i686|arm64|aarch64|armv7|armhf|riscv64)\.(AppImage(?:\.tar\.gz)?|deb|rpm)$/i
  )
  if (!match) return null

  const extension = match[2].toLowerCase()
  return {
    os: "linux",
    arch: normalizeArch(match[1]),
    installer: extension.startsWith("appimage") ? "appimage" : extension,
  }
}

function parseMacUpdater(fileName, inferredArch) {
  if (!fileName.endsWith(".app.tar.gz")) {
    return null
  }

  const match = fileName.match(/_(x64|aarch64|arm64|universal)\.app\.tar\.gz$/i)
  const arch = normalizeArch(match ? match[1] : inferredArch)
  if (!arch) {
    throw new Error(`Unable to determine macOS updater architecture for ${fileName}`)
  }

  return {
    os: "darwin",
    arch,
    installer: "app",
  }
}

function normalizeGitHubAssetName(fileName) {
  return fileName.replace(/ /g, ".")
}

function extractAssetNameFromUrl(assetUrl) {
  let parsed
  try {
    parsed = new URL(assetUrl)
  } catch {
    throw new Error(`Invalid updater asset URL: ${assetUrl}`)
  }

  const fileName = decodeURIComponent(path.basename(parsed.pathname))
  if (!fileName) {
    throw new Error(`Updater asset URL does not include a file name: ${assetUrl}`)
  }
  return fileName
}

function buildAssetUrl(repo, tag, fileName) {
  if (!repo.trim()) {
    throw new Error("Missing GitHub repository. Pass --repo when synthesizing latest.json")
  }
  if (!tag.trim()) {
    throw new Error("Missing GitHub tag. Pass --tag when synthesizing latest.json")
  }

  const normalizedFileName = normalizeGitHubAssetName(fileName)
  return `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(normalizedFileName)}`
}

function normalizeManifestAssetUrls(manifest, repo, tag) {
  const platforms = {}

  for (const [platform, value] of Object.entries(manifest.platforms)) {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new Error(`Updater platform entry must be an object: ${platform}`)
    }

    platforms[platform] = {
      ...value,
      url:
        typeof value.url === "string" && value.url.trim()
          ? buildAssetUrl(repo, tag, extractAssetNameFromUrl(value.url))
          : value.url,
    }
  }

  return {
    ...manifest,
    platforms,
  }
}

function synthesizeManifest({ inputDir, repo, tag, version, pubDate }) {
  if (!version.trim()) {
    throw new Error("Missing release version. Pass --version when synthesizing latest.json")
  }

  const files = collectFiles(inputDir)
  const basenames = files.map(filePath => path.basename(filePath))

  const signatureByAsset = new Map()
  for (const filePath of files) {
    const fileName = path.basename(filePath)
    if (!fileName.endsWith(".sig") || fileName === "latest.json.sig") {
      continue
    }
    const assetName = fileName.slice(0, -4)
    signatureByAsset.set(assetName, fs.readFileSync(filePath, "utf8").trim())
  }

  const macDmgArchs = new Set(basenames.map(parseMacArchFromDmg).filter(value => value !== null))
  const fallbackMacArch = macDmgArchs.size === 1 ? [...macDmgArchs][0] : null

  const candidates = []
  for (const fileName of [...signatureByAsset.keys()].sort()) {
    if (fileName === "latest.json" || /^latest-[^.]+\.json$/.test(fileName)) {
      continue
    }

    const detected =
      parseWindowsUpdater(fileName) ||
      parseMacUpdater(fileName, fallbackMacArch) ||
      parseLinuxUpdater(fileName)

    if (!detected) {
      continue
    }

    const baseTarget = `${detected.os}-${detected.arch}`
    const exactTarget = `${baseTarget}-${detected.installer}`
    const value = {
      url: buildAssetUrl(repo, tag, fileName),
      signature: signatureByAsset.get(fileName),
    }

    candidates.push({ baseTarget, exactTarget, value, fileName })
  }

  if (candidates.length === 0) {
    const hasSignatures = signatureByAsset.size > 0
    const hint = hasSignatures
      ? "Signature files were found, but none matched supported updater asset names. Check the file naming conventions for macOS (.app.tar.gz), Windows (.*-setup.exe/.msi), or Linux (.AppImage/.deb/.rpm)."
      : "No signature files were found. The updater manifest is synthesized from *.sig files generated by Tauri when signing is enabled. Ensure TAURI_SIGNING_PRIVATE_KEY (and TAURI_SIGNING_PRIVATE_KEY_PASSWORD if needed) are set and bundle.createUpdaterArtifacts is true."
    throw new Error(
      `No updater artifacts found in ${inputDir}. Found files: ${basenames.join(", ")}\n${hint}`
    )
  }

  const platforms = {}
  const baseTargetGroups = new Map()

  for (const candidate of candidates) {
    const existing = platforms[candidate.exactTarget]
    if (existing && JSON.stringify(existing) !== JSON.stringify(candidate.value)) {
      throw new Error(
        `Conflicting updater assets for target ${candidate.exactTarget}: ${candidate.fileName}`
      )
    }
    platforms[candidate.exactTarget] = candidate.value

    const group = baseTargetGroups.get(candidate.baseTarget) || []
    group.push(candidate)
    baseTargetGroups.set(candidate.baseTarget, group)
  }

  for (const [baseTarget, group] of baseTargetGroups.entries()) {
    if (group.length !== 1) {
      continue
    }
    platforms[baseTarget] = group[0].value
  }

  const sortedPlatforms = Object.fromEntries(
    Object.entries(platforms).sort(([left], [right]) => left.localeCompare(right))
  )

  return {
    version,
    pub_date: pubDate,
    platforms: sortedPlatforms,
  }
}

function main() {
  const { inputDir, outputPath, repo, tag, version, pubDate } = parseArgs(process.argv.slice(2))
  const manifestPaths = collectManifestPaths(inputDir)

  let merged
  if (manifestPaths.length > 0) {
    const first = readManifest(manifestPaths[0])
    merged = {
      version: first.version,
      notes: first.notes,
      pub_date: first.pub_date,
      platforms: { ...first.platforms },
    }

    for (const manifestPath of manifestPaths.slice(1)) {
      mergeManifest(merged, readManifest(manifestPath), manifestPath)
    }
  } else {
    merged = synthesizeManifest({ inputDir, repo, tag, version, pubDate })
  }

  merged = normalizeManifestAssetUrls(merged, repo, tag)

  fs.mkdirSync(path.dirname(outputPath), { recursive: true })
  fs.writeFileSync(outputPath, `${JSON.stringify(merged, null, 2)}\n`)
  console.log(`Prepared updater manifest: ${outputPath}`)
}

if (require.main === module) {
  main()
}

module.exports = {
  buildAssetUrl,
  extractAssetNameFromUrl,
  normalizeGitHubAssetName,
  normalizeManifestAssetUrls,
}
