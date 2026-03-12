import { state } from "@relax-state/core"
import type { IntegrationTarget } from "@/types"

export const integrationTargetsState = state<IntegrationTarget[]>([], "integration.targets")
export const integrationTargetsLoadingState = state<boolean>(false, "integration.loading")
export const integrationTargetsErrorState = state<string | null>(null, "integration.error")
