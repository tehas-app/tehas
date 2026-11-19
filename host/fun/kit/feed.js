// The feed store (host/docs/frontend.md, "Feeds"): one source per /live/
// address per page, shared by every island that reads it, with one poller.
// It waits while the page is hidden and keeps the last good answer when a
// poll fails.
//
//   const status = feed("/live/status.json", { every: 10000 });
//   status.data()     the parsed file; <Loading> until the first answer, <Errored> if it fails
//   status.error()    the error of the last poll, or null; the data stays the last good one
//   status.at()       when the data last changed, in ms
//   status.refresh()  poll now
//   isPending(() => status.data())  true while a poll that will change the data is out
//
// feed() is called inside an island's root. The feed lives under the page's
// owner (state.js) and stops when the last root that asked for it is gone.
import { createMemo, createSignal, getOwner, onCleanup } from "./vendor/solid.js";
import { release, shared } from "./state.js";

const hidden = () => globalThis.document?.hidden === true;

function make(src) {
  const [tick, setTick] = createSignal(0);
  const abort = new AbortController();
  const entry = createMemo(async (prev) => {
    tick();
    try {
      const res = await fetch(src, { cache: "no-store", signal: abort.signal });
      if (!res.ok) throw new Error(`${src}: ${res.status} ${res.statusText}`.trim());
      const text = await res.text();
      if (prev?.text === text) return prev.error ? { ...prev, error: null } : prev;
      return { text, data: JSON.parse(text), at: Date.now(), error: null };
    } catch (error) {
      if (!prev) throw error;
      return { ...prev, error };
    }
  });
  const f = { src, every: Infinity, users: 0, timer: 0, due: false };
  const poll = () => {
    f.due = false;
    setTick((n) => n + 1);
    schedule();
  };
  const schedule = () => {
    clearTimeout(f.timer);
    if (f.every === Infinity) return;
    f.timer = setTimeout(() => (hidden() ? (f.due = true) : poll()), f.every);
  };
  const onVisible = () => {
    if (!hidden() && f.due) poll();
  };
  globalThis.document?.addEventListener?.("visibilitychange", onVisible);
  onCleanup(() => {
    clearTimeout(f.timer);
    abort.abort();
    globalThis.document?.removeEventListener?.("visibilitychange", onVisible);
  });
  f.schedule = schedule;
  f.api = Object.freeze({
    src,
    data: () => entry().data,
    error: () => entry().error,
    at: () => entry().at,
    refresh: poll,
  });
  return f;
}

/**
 * The feed of `src` for this page, made on first use. `every` is the poll
 * period in ms; the shortest any reader asks for wins, and none means no poll.
 * @param {string} src
 * @param {{ every?: number }} [options]
 * @returns {{ src: string, data: () => any, error: () => unknown, at: () => number, refresh: () => void }}
 */
export function feed(src, { every = Infinity } = {}) {
  if (!getOwner()) throw new Error(`feed ${src}: call it inside an island's root`);
  const key = `feed:${src}`;
  const f = shared(key, () => make(src));
  f.users += 1;
  if (every < f.every) {
    f.every = every;
    f.schedule();
  }
  onCleanup(() => {
    f.users -= 1;
    if (f.users === 0) release(key);
  });
  return f.api;
}
