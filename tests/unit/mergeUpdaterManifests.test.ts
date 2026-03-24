import assert from "node:assert/strict"
import path from "node:path"
import { test } from "node:test"

const {
  buildAssetUrl,
  extractAssetNameFromUrl,
  normalizeGitHubAssetName,
  normalizeManifestAssetUrls,
} = require(path.resolve(process.cwd(), "scripts/merge-updater-manifests.js")) as {
  buildAssetUrl: (repo: string, tag: string, fileName: string) => string
  extractAssetNameFromUrl: (assetUrl: string) => string
  normalizeGitHubAssetName: (fileName: string) => string
  normalizeManifestAssetUrls: (
    manifest: {
      version: string
      pub_date?: string
      platforms: Record<string, { url: string; signature: string }>
    },
    repo: string,
    tag: string
  ) => {
    version: string
    pub_date?: string
    platforms: Record<string, { url: string; signature: string }>
  }
}

test("normalizeGitHubAssetName matches the asset naming GitHub release downloads use", () => {
  assert.equal(
    normalizeGitHubAssetName("AI Open Router_0.2.14_x64-setup.exe"),
    "AI.Open.Router_0.2.14_x64-setup.exe"
  )
  assert.equal(normalizeGitHubAssetName("latest.json"), "latest.json")
})

test("extractAssetNameFromUrl decodes encoded asset paths", () => {
  assert.equal(
    extractAssetNameFromUrl(
      "https://github.com/spencerkit/ai-open-router/releases/download/v0.2.14/AI%20Open%20Router.app.tar.gz"
    ),
    "AI Open Router.app.tar.gz"
  )
})

test("buildAssetUrl rewrites release URLs to the canonical GitHub asset name", () => {
  assert.equal(
    buildAssetUrl("spencerkit/ai-open-router", "v0.2.14", "AI Open Router_0.2.14_x64-setup.exe"),
    "https://github.com/spencerkit/ai-open-router/releases/download/v0.2.14/AI.Open.Router_0.2.14_x64-setup.exe"
  )
})

test("normalizeManifestAssetUrls rewrites updater manifest downloads to canonical names", () => {
  const manifest = normalizeManifestAssetUrls(
    {
      version: "0.2.14",
      platforms: {
        "windows-x86_64": {
          url: "https://github.com/spencerkit/ai-open-router/releases/download/v0.2.14/AI%20Open%20Router_0.2.14_x64-setup.exe",
          signature: "sig",
        },
      },
    },
    "spencerkit/ai-open-router",
    "v0.2.14"
  )

  assert.equal(
    manifest.platforms["windows-x86_64"].url,
    "https://github.com/spencerkit/ai-open-router/releases/download/v0.2.14/AI.Open.Router_0.2.14_x64-setup.exe"
  )
})
