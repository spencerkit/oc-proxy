import type { GroupImportMode } from "../types/proxy"

export type ImportSource = "file" | "clipboard"

export type ImportRequest =
  | {
      source: "file"
      payload: {
        mode: GroupImportMode
      }
    }
  | {
      source: "clipboard"
      payload: {
        jsonText: string
        mode: GroupImportMode
      }
    }

export function buildImportRequest(input: {
  source: ImportSource
  mode: GroupImportMode
  jsonText: string
}): ImportRequest {
  if (input.source === "file") {
    return {
      source: "file",
      payload: {
        mode: input.mode,
      },
    }
  }

  return {
    source: "clipboard",
    payload: {
      jsonText: input.jsonText,
      mode: input.mode,
    },
  }
}

export function canConfirmImportRequest(input: {
  source: ImportSource
  jsonText: string
}): boolean {
  return input.source === "file" || input.jsonText.trim().length > 0
}

export function getImportModeWarningKey(
  mode: GroupImportMode
): "settings.importModeIncrementalWarning" | "settings.importModeOverwriteWarning" {
  return mode === "overwrite"
    ? "settings.importModeOverwriteWarning"
    : "settings.importModeIncrementalWarning"
}
