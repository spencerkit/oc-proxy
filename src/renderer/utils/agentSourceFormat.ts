import type { AgentConfig, AgentSourceFile, IntegrationClientKind } from "@/types"

export function formatAgentSourceDraft(kind: IntegrationClientKind, source: string): string {
  if (kind !== "openclaw") {
    return source
  }

  try {
    return JSON.stringify(JSON.parse(source), null, 2)
  } catch {
    return source
  }
}

export function getDirtySourceIds(
  sourceFiles: AgentSourceFile[],
  sourceDrafts: Record<string, string>
): string[] {
  return sourceFiles
    .filter(file => (sourceDrafts[file.sourceId] ?? file.content) !== file.content)
    .map(file => file.sourceId)
}

export function hasDirtySourceDrafts(
  sourceFiles: AgentSourceFile[],
  sourceDrafts: Record<string, string>
): boolean {
  return getDirtySourceIds(sourceFiles, sourceDrafts).length > 0
}

export function mergeReloadedFormDraftState(
  current: {
    formData: AgentConfig
    timeoutText: string
    fallbackModelsText: string
  },
  next: {
    formData: AgentConfig
    timeoutText: string
    fallbackModelsText: string
  },
  preserveCurrent: boolean
): {
  formData: AgentConfig
  timeoutText: string
  fallbackModelsText: string
} {
  return preserveCurrent ? current : next
}

export function mergeReloadedSourceDrafts(
  previousSourceFiles: AgentSourceFile[],
  previousSourceDrafts: Record<string, string>,
  nextSourceFiles: AgentSourceFile[],
  savedSourceId?: string
): Record<string, string> {
  const previousFilesById = new Map(previousSourceFiles.map(file => [file.sourceId, file]))

  return Object.fromEntries(
    nextSourceFiles.map(file => {
      if (file.sourceId === savedSourceId) {
        return [file.sourceId, file.content]
      }

      const previousFile = previousFilesById.get(file.sourceId)
      const previousDraft = previousSourceDrafts[file.sourceId]
      const previousWasDirty =
        previousFile !== undefined &&
        previousDraft !== undefined &&
        previousDraft !== previousFile.content

      return [file.sourceId, previousWasDirty ? previousDraft : file.content]
    })
  )
}
