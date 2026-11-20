// Page state shared by the islands of one page (host/docs/frontend.md, "Page
// state"). A module is evaluated once per page, so the owner made here is the
// page's: what lives under it outlasts any one island's root.
//
//   export const useBoard = pageStore("board", {
//     state: () => ({ picked: null }),
//     actions: (set) => ({
//       pick(n) { set((d) => { d.picked = n; }); },
//     }),
//   });
//   const board = useBoard();   // the same object in every island
//   board.state.picked          // a reactive read
//   board.actions.pick(42)      // the only way to write; lands on the next microtask
import { createRoot, createStore, getOwner, runWithOwner, untrack } from "./vendor/solid.js";

const owner = createRoot(() => getOwner());
/** @type {Map<string, { value: any, dispose: () => void }>} */
const made = new Map();

/**
 * The value of `key` for this page, made by `make` in a root of its own under
 * the page's owner on the first call. Every later call gets the same value.
 * @template T
 * @param {string} key
 * @param {() => T} make
 * @returns {T}
 */
export function shared(key, make) {
  if (!made.has(key)) {
    made.set(
      key,
      runWithOwner(owner, () => createRoot((dispose) => ({ value: make(), dispose }))),
    );
  }
  return /** @type {{ value: T }} */ (made.get(key)).value;
}

/**
 * Disposes the value of `key`; the next `shared(key)` makes it again.
 * @param {string} key
 */
export function release(key) {
  made.get(key)?.dispose();
  made.delete(key);
}

/** Disposes all the page's state, for a test. */
export function resetPage() {
  for (const key of [...made.keys()]) release(key);
}

/** @param {object} actions */
const names = (actions) => Object.keys(actions).sort().join();

/**
 * A named page store. Returns `use()`, which gives `{ state, actions }`: the
 * store, read-only to its readers, and the actions `spec.actions` makes from
 * its setter. The first `use()` on the page makes it; a second definition of
 * the name must have the same actions.
 * @template {object} S
 * @template {Record<string, (...args: any[]) => any>} A
 * @param {string} name
 * @param {{ state: () => S, actions: (set: (fn: (draft: S) => S | void) => void, state: S) => A }} spec
 * @returns {() => { state: S, actions: A }}
 */
export function pageStore(name, spec) {
  return () => {
    const entry = shared(`store:${name}`, () => {
      const [state, set] = createStore(spec.state());
      const actions = Object.freeze(spec.actions(set, state));
      return { spec, store: Object.freeze({ state, actions }) };
    });
    // Made against the store's state, untracked: actions may read it as they are made.
    if (entry.spec !== spec && names(entry.store.actions) !== names(untrack(() => spec.actions(() => {}, entry.store.state)))) {
      throw new Error(`page store ${name} is defined twice with other actions`);
    }
    return entry.store;
  };
}
