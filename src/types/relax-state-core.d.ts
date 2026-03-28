declare module "@relax-state/core" {
  import type { Store } from "@relax-state/store"

  export interface Value<T> {
    readonly name?: string
    readonly __relaxValueBrand?: T
  }

  export interface State<T> extends Value<T> {
    readonly __relaxStateBrand?: T
  }

  export type Getter = <T>(state: Value<T>) => T

  export interface ComputedOptions<T> {
    name?: string
    get: (get: Getter) => T
  }

  export interface ActionOptions {
    name?: string
  }

  type CallableAction<P, R> = [P] extends [undefined]
    ? () => R
    : [P] extends [undefined]
      ? () => R
      : undefined extends P
        ? (payload?: P) => R
        : (payload: P) => R

  export type Action<P = unknown, R = unknown> = {
    readonly name?: string
  } & CallableAction<P, R>

  export function state<T>(initialValue: T, name?: string): State<T>

  export function computed<T>(options: ComputedOptions<T>): Value<T>

  export function action<R>(handler: (store: Store) => R, options?: ActionOptions): Action<void, R>

  export function action<P, R>(
    handler: (store: Store, payload: P) => R,
    options?: ActionOptions
  ): Action<P, R>

  export function action<P, R>(
    handler: (store: Store, payload?: P) => R,
    options?: ActionOptions
  ): Action<P | undefined, R>
}
