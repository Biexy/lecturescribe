declare module "react" {
  export type ReactNode = unknown;
  export type Key = string | number;
  export type Dispatch<T> = (value: T) => void;
  export type SetStateAction<T> = T | ((previous: T) => T);
  export interface RefObject<T> {
    current: T | null;
  }
  export interface MutableRefObject<T> {
    current: T;
  }
  export interface Context<T> {
    Provider: unknown;
    Consumer: unknown;
    displayName?: string;
  }
  export interface ErrorInfo {
    componentStack?: string;
  }
  export const Fragment: unknown;
  export function createElement(type: unknown, props?: unknown, ...children: unknown[]): unknown;
  export function createContext<T>(defaultValue: T): Context<T>;
  export function useContext<T>(context: Context<T>): T;
  export function useState<T>(
    initial: T | (() => T),
  ): [T, Dispatch<SetStateAction<T>>];
  export function useReducer<S, A>(
    reducer: (state: S, action: A) => S,
    initialState: S,
  ): [S, Dispatch<A>];
  export function useEffect(
    effect: () => void | (() => void),
    dependencies?: readonly unknown[],
  ): void;
  export function useLayoutEffect(
    effect: () => void | (() => void),
    dependencies?: readonly unknown[],
  ): void;
  export function useMemo<T>(factory: () => T, dependencies: readonly unknown[]): T;
  export function useCallback<T extends (...args: any[]) => any>(
    callback: T,
    dependencies: readonly unknown[],
  ): T;
  export function useRef<T>(initial: T): MutableRefObject<T>;
  export function useRef<T>(initial: T | null): RefObject<T>;
  export function useDeferredValue<T>(value: T): T;
  export function useId(): string;
  export function memo<T>(component: T): T;
  export function startTransition(scope: () => void): void;
  const React: {
    Fragment: unknown;
    createElement: typeof createElement;
  };
  export default React;
}

declare module "react/jsx-runtime" {
  export const Fragment: unknown;
  export function jsx(type: unknown, props: unknown, key?: string): unknown;
  export function jsxs(type: unknown, props: unknown, key?: string): unknown;
}

declare module "react/jsx-dev-runtime" {
  export const Fragment: unknown;
  export function jsxDEV(
    type: unknown,
    props: unknown,
    key: string | undefined,
    isStaticChildren: boolean,
    source: unknown,
    self: unknown,
  ): unknown;
}

declare module "react-reconciler" {
  const createReconciler: (hostConfig: Record<string, unknown>) => any;
  export default createReconciler;
}

declare module "react-reconciler/constants.js" {
  export const ConcurrentRoot: number;
  export const DefaultEventPriority: number;
  export const DiscreteEventPriority: number;
  export const ContinuousEventPriority: number;
  export const NoEventPriority: number;
}

declare module "scheduler" {
  export function unstable_scheduleCallback(...args: unknown[]): unknown;
  export function unstable_cancelCallback(callback: unknown): void;
  export function unstable_shouldYield(): boolean;
  export function unstable_now(): number;
}

declare namespace JSX {
  type Element = unknown;
  interface IntrinsicAttributes {
    key?: string | number;
  }
  interface ElementChildrenAttribute {
    children: {};
  }
  interface IntrinsicElements {
    [elementName: string]: any;
  }
}
