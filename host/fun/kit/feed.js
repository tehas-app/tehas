// The feed store (host/docs/frontend.md, "Feeds"): one source per address and
// options per page, shared by every island that reads it, with one poller.
// It waits while the page is hidden and keeps the last good answer when a
// poll fails.
//
//   const status = feed("/live/status.json", { every: 10000 });
//   status.data()     the parsed file; <Loading> until the first answer, <Errored> if it fails
//   status.error()    the error of the last poll, or null; the data stays the last good one
//   status.at()       when the data last changed, in ms
//   status.refresh()  poll now
//   status.meta()     the last answer: { status, headers, date, at }, or null before one
//   isPending(() => status.data())  true while any poll is out, even one that changes nothing
//
//   const plan = feed("/api/plan", {
//     headers: () => ({ "X-Session": session() }),  // or an object; read on each poll
//     credentials: "same-origin",
//     parse: "json",                                // or "text"
//     accept: { 404: { releases: [] } },            // a status that is data, not an error
//     expose: ["X-Session"],                        // the headers meta() gives
//   });
//
// feed() is called inside an island's root. The feed lives under the page's
// owner (state.js) and stops when the last root that asked for it is gone.
// One fetch is out at a time: the next poll falls due a period after the last
// one lands, and a fetch with no answer in TIMEOUT ms is aborted.
import { createMemo, createSignal, getOwner, onCleanup, untrack } from "./vendor/solid.js";
import { accepts, bodyOf, dateOf, headersOf, keyOf, pickHeaders } from "./request.js";
import { release, shared } from "./state.js";

const MIN_EVERY = 1000;
const TIMEOUT = 30000;
const hidden = () => globalThis.document?.hidden === true;

// The options that make a source of its own; `every` is shared by its readers.
const KEYED = ["headers", "credentials", "parse", "accept", "expose"];

function make(src, opts) {
  const [tick, setTick] = createSignal(0);
  const [meta, setMeta] = createSignal(null);
  const f = { periods: [], every: Infinity, timer: 0, due: false, out: undefined, again: false, gone: false };
  const entry = createMemo(async (prev) => {
    tick();
    const out = new AbortController();
    f.out = out;
    const limit = setTimeout(() => out.abort(new Error(`${src}: no answer in ${TIMEOUT} ms`)), TIMEOUT);
    try {
      const init = { cache: "no-store", signal: out.signal };
      // Untracked: a signal the function reads must not start a fetch of its own.
      if (opts.headers) init.headers = untrack(() => headersOf(opts.headers));
      if (opts.credentials) init.credentials = opts.credentials;
      const res = await fetch(src, init);
      // Its own signal, so a new Date leaves the data's readers be. It lands
      // a moment before the data does, while the poll is still pending.
      setMeta({ status: res.status, headers: pickHeaders(res, opts.expose), date: dateOf(res), at: Date.now() });
      if (!res.ok && !accepts(opts.accept, res.status)) {
        const error = new Error(`${src}: ${res.status} ${res.statusText ?? ""}`.trim());
        error.status = res.status;
        throw error;
      }
      const text = await res.text();
      const token = `${res.status} ${text}`;
      if (prev?.token === token) return prev.error ? { ...prev, error: null } : prev;
      return { token, data: bodyOf(res.status, text, opts), at: Date.now(), error: null };
    } catch (error) {
      if (!prev) throw error;
      return { ...prev, error };
    } finally {
      clearTimeout(limit);
      // After the memo has its value: a tick now would replace it.
      queueMicrotask(() => landed(out));
    }
  });
  const landed = (out) => {
    if (f.out !== out) return;
    f.out = null;
    if (f.gone) return;
    if (f.again) poll();
    else schedule();
  };
  const poll = () => {
    if (f.out) {
      f.again = true;
      return;
    }
    clearTimeout(f.timer);
    f.due = false;
    f.again = false;
    setTick((n) => n + 1);
  };
  const schedule = () => {
    clearTimeout(f.timer);
    // out is undefined until the first fetch, an AbortController while one is out.
    if (f.out !== null || f.gone || f.every === Infinity) return;
    f.timer = setTimeout(() => (hidden() ? (f.due = true) : poll()), f.every);
  };
  // The shortest period a reader asks for, again when one comes or goes.
  f.retime = () => {
    const every = Math.min(Infinity, ...f.periods);
    if (every === f.every) return;
    f.every = every;
    schedule();
  };
  const onVisible = () => {
    if (!hidden() && f.due) poll();
  };
  globalThis.document?.addEventListener?.("visibilitychange", onVisible);
  onCleanup(() => {
    f.gone = true;
    clearTimeout(f.timer);
    f.out?.abort();
    globalThis.document?.removeEventListener?.("visibilitychange", onVisible);
  });
  f.api = Object.freeze({
    src,
    data: () => entry().data,
    error: () => entry().error,
    at: () => entry().at,
    meta,
    refresh: poll,
  });
  return f;
}

/**
 * The feed of `src` for this page, made on first use. `every` is the poll
 * period in ms, at least 1000; the shortest any reader asks for wins while
 * that reader lives, and none means no poll. Readers that give the same
 * other options share one source: `headers` (an object, or a function read
 * on each poll), `credentials`, `parse` ("json" or "text"), `accept` (a
 * status that is data: its value, or a function of the body's text) and
 * `expose` (the answer's headers that meta() gives).
 * @param {string} src
 * @param {{ every?: number, headers?: object | (() => object), credentials?: RequestCredentials,
 *   parse?: "json" | "text", accept?: Record<number, any>, expose?: string[] }} [options]
 * @returns {{ src: string, data: () => any, error: () => unknown, at: () => number,
 *   meta: () => null | { status: number, headers: Record<string, string | null>, date: number | null, at: number },
 *   refresh: () => void }}
 */
export function feed(src, options = {}) {
  const { every = Infinity } = options;
  if (!getOwner()) throw new Error(`feed ${src}: call it inside an island's root`);
  const key = keyOf("feed", src, options, KEYED);
  const f = shared(key, () => make(src, options));
  const period = Number.isFinite(every) ? Math.max(MIN_EVERY, every) : Infinity;
  f.periods.push(period);
  f.retime();
  onCleanup(() => {
    f.periods.splice(f.periods.indexOf(period), 1);
    if (f.periods.length === 0) release(key);
    else f.retime();
  });
  return f.api;
}
