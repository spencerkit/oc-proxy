import type { Action } from "@relax-state/core"
import { useActions as useRelaxActions, useRelaxValue } from "@relax-state/react"

type ActionHookReturn<P extends readonly unknown[]> = {
  [K in keyof P]: P[K] extends (...args: infer A) => infer R ? (...args: A) => R : never
}

export const useActions = <const P extends readonly unknown[]>(actions: P): ActionHookReturn<P> => {
  return useRelaxActions(actions as unknown as Action[]) as ActionHookReturn<P>
}

export { useRelaxValue }
