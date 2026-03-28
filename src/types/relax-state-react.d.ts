declare module "@relax-state/react" {
  import type { PropsWithChildren, ReactElement } from "react"
  import type { Action, Value } from "@relax-state/core"

  type AnyFn = (...args: never[]) => unknown

  type BoundAction<TAction> = TAction extends AnyFn
    ? (...args: Parameters<TAction>) => ReturnType<TAction>
    : never

  export function useRelaxValue<T>(state: Value<T>): T

  export function useActions<const TActions extends readonly Action[]>(
    actions: TActions
  ): {
    [K in keyof TActions]: BoundAction<TActions[K]>
  }

  export function RelaxProvider(props: PropsWithChildren): ReactElement | null
}
