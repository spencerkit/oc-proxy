import assert from "node:assert/strict"
import { test } from "node:test"

import {
  buildImportRequest,
  canConfirmImportRequest,
  getImportModeWarningKey,
  type ImportSource,
} from "../../src/renderer/utils/importMode"

test("buildImportRequest attaches overwrite mode for file imports", () => {
  const result = buildImportRequest({
    source: "file" satisfies ImportSource,
    mode: "overwrite",
    jsonText: "ignored",
  })

  assert.deepEqual(result, {
    source: "file",
    payload: {
      mode: "overwrite",
    },
  })
})

test("buildImportRequest attaches incremental mode and json text for clipboard imports", () => {
  const result = buildImportRequest({
    source: "clipboard" satisfies ImportSource,
    mode: "incremental",
    jsonText: '{"groups":[]}',
  })

  assert.deepEqual(result, {
    source: "clipboard",
    payload: {
      jsonText: '{"groups":[]}',
      mode: "incremental",
    },
  })
})

test("getImportModeWarningKey returns overwrite-specific warning copy", () => {
  assert.equal(getImportModeWarningKey("incremental"), "settings.importModeIncrementalWarning")
  assert.equal(getImportModeWarningKey("overwrite"), "settings.importModeOverwriteWarning")
})

test("canConfirmImportRequest requires JSON text only for clipboard imports", () => {
  assert.equal(canConfirmImportRequest({ source: "file", jsonText: "" }), true)
  assert.equal(canConfirmImportRequest({ source: "clipboard", jsonText: "   " }), false)
  assert.equal(canConfirmImportRequest({ source: "clipboard", jsonText: '{"groups":[]}' }), true)
})
