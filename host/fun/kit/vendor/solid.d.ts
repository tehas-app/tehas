// Types for the kit's Solid 2 (vendor/solid.js), its export list only
// (tools/vendor-solid), written by hand from the packages' own .d.ts files:
// solid-js 2.0.3, @solidjs/signals 2.0.3, @solidjs/web 2.0.3 and
// @solidjs/html 2.0.1. A checkJs tsc that resolves ./vendor/solid.js reads
// this file in its place. It is never served.

declare const OWNER: unique symbol;
declare const REFRESH: unique symbol;

export type Accessor<T> = () => T;
export type Setter<T> = (value: T | ((prev: T) => T)) => T;
export type Equals<T> = false | ((prev: T, next: T) => boolean);
/** An owner, from getOwner. */
export type Owner = { readonly [OWNER]: true };
/** A memo or a derived store, which refresh can run again. */
export type Refreshable<T> = T & { readonly [REFRESH]: true };
export type SetStore<T> = (fn: (draft: T) => T | void) => void;
export type Component = (props: any) => any;
/** What a render function may return. */
export type JSXElement = Node | string | number | boolean | null | undefined | JSXElement[];

/** A function as the value makes a derived signal: use the second form for that. */
export function createSignal<T>(value: Exclude<T, Function>, options?: { equals?: Equals<T> }): [Accessor<T>, Setter<T>];
export function createSignal<T>(fn: (prev: T | undefined) => T, options?: { equals?: Equals<T> }): [Accessor<T>, Setter<T>];
/** An async memo: a compute that returns a promise gives what it resolves to. */
export function createMemo<T>(
  compute: (prev: T | undefined) => T | PromiseLike<T> | AsyncIterable<T>,
  options?: { equals?: Equals<T> },
): Refreshable<Accessor<T>>;
export function createStore<T extends object>(init: T): [T, SetStore<T>];
export function createRoot<T>(init: (dispose: () => void) => T): T;
export function createEffect<T>(
  compute: (prev: T | undefined) => T,
  effect: ((value: T, prev?: T) => (() => void) | void) | {
    effect: (value: T, prev?: T) => (() => void) | void;
    error: (err: unknown, cleanup: () => void) => void;
  },
  options?: { defer?: boolean },
): void;
export function createRenderEffect<T>(compute: (prev: T | undefined) => T, effect: (value: T, prev?: T) => (() => void) | void): void;
export function onCleanup(fn: () => void): () => void;
export function onSettled(fn: () => void | (() => void)): void;
export function untrack<T>(fn: () => T): T;
export function flush(): void;
export function isPending(fn: () => unknown): boolean;
export function latest<T>(fn: () => T): T;
export function refresh(target: Refreshable<unknown>): Promise<unknown>;
export function action<A extends unknown[], R>(fn: (...args: A) => Generator<unknown, R, any> | AsyncGenerator<unknown, R, any>): (...args: A) => Promise<R>;
export function createOptimistic<T>(value: Exclude<T, Function>): [Accessor<T>, Setter<T>];
export function createOptimistic<T>(fn: (prev: T | undefined) => T, options?: { equals?: Equals<T> }): [Accessor<T>, Setter<T>];
export function createOptimisticStore<T extends object>(init: T): [T, SetStore<T>];
export function createOptimisticStore<T extends object>(fn: (draft: T) => T | void, seed: T): [Refreshable<T>, SetStore<T>];
export function snapshot<T>(value: T): T;
export function getOwner(): Owner | null;
export function runWithOwner<T>(owner: Owner | null, fn: () => T): T;
export const For: Component;
export const Show: Component;
export const Switch: Component;
export const Match: Component;
export const Loading: Component;
export const Errored: Component;
export function render(code: () => JSXElement, element: Node): () => void;
/** A node, or an array of nodes when the template has several at the top. */
export function html(strings: TemplateStringsArray, ...values: unknown[]): any;
