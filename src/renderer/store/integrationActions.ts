import { action } from "@relax-state/core"
import type {
  AgentConfig,
  AgentConfigFile,
  IntegrationClientKind,
  IntegrationTarget,
  IntegrationWriteResult,
  WriteAgentConfigResult,
} from "@/types"
import { bridge } from "@/utils/bridge"
import {
  integrationTargetsErrorState,
  integrationTargetsLoadingState,
  integrationTargetsState,
} from "./integrationState"

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message
  }
  if (typeof error === "string" && error.trim()) {
    return error
  }
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === "string" && message.trim()) {
      return message
    }
  }
  return fallback
}

function requirePayload<P>(payload: P | undefined, name: string): P {
  if (payload === undefined) {
    throw new Error(`${name} requires a payload`)
  }
  return payload
}

export const loadIntegrationTargetsAction = action<void, Promise<IntegrationTarget[]>>(
  async store => {
    store.set(integrationTargetsLoadingState, true)
    store.set(integrationTargetsErrorState, null)
    try {
      const targets = await bridge.integrationListTargets()
      store.set(integrationTargetsState, targets)
      return targets
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to load integration targets")
      store.set(integrationTargetsErrorState, errorMessage)
      throw error
    } finally {
      store.set(integrationTargetsLoadingState, false)
    }
  }
)

export const clearIntegrationTargetsAction = action<void, void>(store => {
  store.set(integrationTargetsState, [])
  store.set(integrationTargetsErrorState, null)
})

export const pickIntegrationDirectoryAction = action<
  { initialDir?: string; kind?: IntegrationClientKind },
  Promise<string | null>
>(async (_store, payload) => {
  const { initialDir, kind } = payload ?? {}
  return await bridge.integrationPickDirectory(initialDir, kind)
})

export const addIntegrationTargetAction = action<
  { kind: IntegrationClientKind; configDir: string },
  Promise<IntegrationTarget>
>(async (store, payload) => {
  const request = requirePayload(payload, "addIntegrationTargetAction")
  try {
    store.set(integrationTargetsErrorState, null)
    const created = await bridge.integrationAddTarget(request.kind, request.configDir)
    const current = store.get(integrationTargetsState)
    store.set(integrationTargetsState, [...current, created])
    return created
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to add integration target")
    store.set(integrationTargetsErrorState, errorMessage)
    throw error
  }
})

export const updateIntegrationTargetAction = action<
  { targetId: string; configDir: string },
  Promise<IntegrationTarget>
>(async (store, payload) => {
  const request = requirePayload(payload, "updateIntegrationTargetAction")
  try {
    store.set(integrationTargetsErrorState, null)
    const updated = await bridge.integrationUpdateTarget(request.targetId, request.configDir)
    const current = store.get(integrationTargetsState)
    store.set(
      integrationTargetsState,
      current.map(item => (item.id === updated.id ? updated : item))
    )
    return updated
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to update integration target")
    store.set(integrationTargetsErrorState, errorMessage)
    throw error
  }
})

export const removeIntegrationTargetAction = action<
  { targetId: string },
  Promise<{ ok: boolean; removed: boolean }>
>(async (store, payload) => {
  const request = requirePayload(payload, "removeIntegrationTargetAction")
  try {
    store.set(integrationTargetsErrorState, null)
    const result = await bridge.integrationRemoveTarget(request.targetId)
    if (result.removed) {
      const current = store.get(integrationTargetsState)
      store.set(
        integrationTargetsState,
        current.filter(item => item.id !== request.targetId)
      )
    }
    return result
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to remove integration target")
    store.set(integrationTargetsErrorState, errorMessage)
    throw error
  }
})

export const readAgentConfigAction = action<{ targetId: string }, Promise<AgentConfigFile>>(
  async (_store, payload) => {
    const request = requirePayload(payload, "readAgentConfigAction")
    return await bridge.integrationReadAgentConfig(request.targetId)
  }
)

export const writeAgentConfigAction = action<
  { targetId: string; config: AgentConfig },
  Promise<WriteAgentConfigResult>
>(async (_store, payload) => {
  const request = requirePayload(payload, "writeAgentConfigAction")
  return await bridge.integrationWriteAgentConfig(request.targetId, request.config)
})

export const writeAgentConfigSourceAction = action<
  { targetId: string; content: string; sourceId?: string },
  Promise<WriteAgentConfigResult>
>(async (_store, payload) => {
  const request = requirePayload(payload, "writeAgentConfigSourceAction")
  return await bridge.integrationWriteAgentConfigSource(
    request.targetId,
    request.content,
    request.sourceId
  )
})

export const writeGroupEntryAction = action<
  { groupId: string; targetIds: string[] },
  Promise<IntegrationWriteResult>
>(async (_store, payload) => {
  const request = requirePayload(payload, "writeGroupEntryAction")
  return await bridge.integrationWriteGroupEntry(request.groupId, request.targetIds)
})
