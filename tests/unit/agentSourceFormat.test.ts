import assert from "node:assert/strict"
import { test } from "node:test"

import type { AgentConfig } from "../../src/renderer/types"
import {
  formatAgentSourceDraft,
  getDirtySourceIds,
  hasDirtySourceDrafts,
  mergeReloadedFormDraftState,
  mergeReloadedSourceDrafts,
} from "../../src/renderer/utils/agentSourceFormat"

test("formatAgentSourceDraft pretty prints OpenClaw JSON source", () => {
  const result = formatAgentSourceDraft(
    "openclaw",
    '{"models":{"providers":{"aor_shared":{"api":"openai-responses"}}}}'
  )

  assert.equal(
    result,
    '{\n  "models": {\n    "providers": {\n      "aor_shared": {\n        "api": "openai-responses"\n      }\n    }\n  }\n}'
  )
})

test("getDirtySourceIds returns every changed source tab", () => {
  const result = getDirtySourceIds(
    [
      {
        sourceId: "primary",
        label: "openclaw.json",
        filePath: "/tmp/openclaw.json",
        content: "{}",
      },
      {
        sourceId: "models",
        label: "models.json",
        filePath: "/tmp/models.json",
        content: "{}",
      },
    ],
    {
      primary: "{}",
      models: '{\n  "providers": {}\n}',
    }
  )

  assert.deepEqual(result, ["models"])
})

test("mergeReloadedFormDraftState preserves unsaved form drafts during source reload", () => {
  const currentFormData: AgentConfig = {
    url: "http://localhost:8080/oc/dev/v1",
    apiToken: "draft-token",
    model: "gpt-4.1",
  }

  const result = mergeReloadedFormDraftState(
    {
      formData: currentFormData,
      timeoutText: "",
      fallbackModelsText: "gpt-4.1-mini",
    },
    {
      formData: {
        url: "http://localhost:8080/oc/dev/v1",
        apiToken: "saved-token",
        model: "gpt-4.1",
      },
      timeoutText: "",
      fallbackModelsText: "",
    },
    true
  )

  assert.deepEqual(result, {
    formData: currentFormData,
    timeoutText: "",
    fallbackModelsText: "gpt-4.1-mini",
  })
})

test("mergeReloadedSourceDrafts preserves inactive dirty tabs after saving another file", () => {
  const result = mergeReloadedSourceDrafts(
    [
      {
        sourceId: "primary",
        label: "openclaw.json",
        filePath: "/tmp/openclaw.json",
        content: "{}",
      },
      {
        sourceId: "models",
        label: "models.json",
        filePath: "/tmp/models.json",
        content: "{}",
      },
    ],
    {
      primary: '{\n  "agents": {}\n}',
      models: '{\n  "providers": {}\n}',
    },
    [
      {
        sourceId: "primary",
        label: "openclaw.json",
        filePath: "/tmp/openclaw.json",
        content: '{\n  "agents": {}\n}',
      },
      {
        sourceId: "models",
        label: "models.json",
        filePath: "/tmp/models.json",
        content: "{}",
      },
    ],
    "primary"
  )

  assert.deepEqual(result, {
    primary: '{\n  "agents": {}\n}',
    models: '{\n  "providers": {}\n}',
  })
})

test("hasDirtySourceDrafts checks all source tabs", () => {
  const result = hasDirtySourceDrafts(
    [
      {
        sourceId: "primary",
        label: "openclaw.json",
        filePath: "/tmp/openclaw.json",
        content: "{}",
      },
      {
        sourceId: "models",
        label: "models.json",
        filePath: "/tmp/models.json",
        content: "{}",
      },
    ],
    {
      primary: "{}",
      models: '{\n  "providers": {}\n}',
    }
  )

  assert.equal(result, true)
})

test("formatAgentSourceDraft leaves invalid source unchanged", () => {
  const result = formatAgentSourceDraft("openclaw", "{")

  assert.equal(result, "{")
})
