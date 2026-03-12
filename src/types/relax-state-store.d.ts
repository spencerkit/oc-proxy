declare module "@relax-state/store" {
  import type { State, Value } from "@relax-state/core"

  export interface Store {
    get<T>(state: Value<T>): T
    set<T>(state: State<T>, value: T): void
    effect<T>(state: Value<T>, fn: (value: { oldValue: T; newValue: T }) => void): () => void
  }

  export const createStore: () => Store
  export const getRuntimeStore: () => Store
  export const setRuntimeStore: (store: Store) => void
  export const resetRuntimeStore: () => void
}
