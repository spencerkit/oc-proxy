const fs = require("node:fs")
const _os = require("node:os")
const path = require("node:path")
const https = require("node:https")
const tar = require("tar")

const rootDir = path.resolve(__dirname, "..")
const pkg = require(path.join(rootDir, "package.json"))

function resolvePlatform() {
  switch (process.platform) {
    case "win32":
      return "windows"
    case "darwin":
      return "darwin"
    case "linux":
      return "linux"
    default:
      return null
  }
}

function resolveArch() {
  switch (process.arch) {
    case "x64":
      return "x64"
    case "arm64":
      return "arm64"
    default:
      return null
  }
}

function resolveAssetName() {
  const platform = resolvePlatform()
  const arch = resolveArch()
  if (!platform || !arch) return null
  return `ai-open-router-${platform}-${arch}.tar.gz`
}

function resolveDownloadUrl(assetName) {
  return `https://github.com/spencerkit/ai-open-router/releases/download/v${pkg.version}/${assetName}`
}

function download(url, destination) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, response => {
      if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
        return resolve(download(response.headers.location, destination))
      }
      if (response.statusCode !== 200) {
        return reject(
          new Error(`download failed: ${response.statusCode} ${response.statusMessage || ""}`)
        )
      }
      const file = fs.createWriteStream(destination)
      response.pipe(file)
      file.on("finish", () => {
        file.close(resolve)
      })
    })
    request.on("error", reject)
  })
}

async function main() {
  const assetName = resolveAssetName()
  if (!assetName) {
    console.error("Unsupported platform/arch for ai-open-router CLI.")
    process.exit(1)
  }

  const url = resolveDownloadUrl(assetName)
  const vendorDir = path.join(rootDir, "vendor")
  const archivePath = path.join(vendorDir, assetName)
  const binName = process.platform === "win32" ? "ai-open-router.exe" : "ai-open-router"
  const binPath = path.join(vendorDir, binName)

  fs.mkdirSync(vendorDir, { recursive: true })

  console.log(`Downloading ${url}`)
  await download(url, archivePath)

  console.log("Extracting binary...")
  await tar.x({
    file: archivePath,
    cwd: vendorDir,
  })

  if (!fs.existsSync(binPath)) {
    throw new Error(`binary not found after extract: ${binPath}`)
  }

  if (process.platform !== "win32") {
    fs.chmodSync(binPath, 0o755)
  }

  fs.unlinkSync(archivePath)
  console.log("ai-open-router installed.")
}

main().catch(error => {
  console.error(error.message || error)
  process.exit(1)
})
