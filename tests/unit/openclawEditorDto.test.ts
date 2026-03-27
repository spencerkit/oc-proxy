import assert from "node:assert/strict"
import { test } from "node:test"

import type { AgentConfigFile } from "../../src/renderer/types"

const openclawConfigFile = {
  targetId: "openclaw-target",
  kind: "openclaw",
  configDir: "/tmp/.openclaw",
  filePath: "/tmp/.openclaw/openclaw.json",
  content: "{}",
  sourceFiles: [],
  parsedConfig: {
    agentId: "workspace-alpha",
    providerId: "aor_shared",
  },
  openclawEditor: {
    agentId: "workspace-alpha",
    providerId: "aor_shared",
    primaryModel: "gpt-4.1-mini",
    fallbackModels: ["gpt-4o-mini"],
    apiFormat: "openai-responses",
    baseUrl: "http://127.0.0.1:8899/oc/dev/v1",
    apiKey: "secret",
  },
} satisfies AgentConfigFile

test("AgentConfigFile carries OpenClaw editor payload", () => {
  assert.equal(openclawConfigFile.openclawEditor.providerId, "aor_shared")
  assert.equal(openclawConfigFile.openclawEditor.primaryModel, "gpt-4.1-mini")
})
