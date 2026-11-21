// The feed store (host/docs/frontend.md, "Feeds"): one source per address and
// options per page, shared by every island that reads it, with one poller.
// It waits while the page is hidden and keeps the last good answer when a
// poll fails.
//
//   const status = feed("/live/status.json", { every: 10000 });
//   status.data()     the parsed file; <Loading> until the first answer, <Errored> if it fails
//   status.error()    the error of the last poll, or null; the data stays the last good one
//   status.at()       when the data last changed, in ms
//   status.refresh()  poll now; resolves { ok, status, data } or { ok: false, status, error }
//   status.put(value) a value the page has, such as a press's answer: data with no fetch
//   status.meta()     the last answer: { status, headers, date, at }, or null before one
//   isPending(() => status.data())  true while any poll is out, even one that changes nothing
//
//   const plan = feed("/api/plan", {
//     headers: sessionHeaders,  // one function per page, or an object; read on each poll
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
import { createMemo, createSignal, flush, getOwner, onCleanup, untrack } from "./vendor/solid.js";
import { accepts, bodyOf, dateOf, headersOf, keyOf, pickHeaders } from "./request.js";
import { release, shared } from "./state.js";
/** @import { Accept, Headers, Parse } from "./request.js" */

/**
 * @typedef {{ every?: number, headers?: Headers, credentials?: RequestCredentials, parse?: Parse,
 *   accept?: Accept, expose?: string[] }} FeedOptions
 * @typedef {{ status: number, headers: Record<string, string | null>, date: number | null, at: number }} Meta
 * @typedef {{ readonly ok: true, readonly status: number, readonly data: any }
 *   | { readonly ok: false, readonly status: number, readonly error: unknown }} FeedAnswer
 *   What refresh() resolves with; it never rejects.
 * @typedef {{ readonly src: string, readonly data: () => any, readonly error: () => unknown,
 *   readonly at: () => number, readonly meta: () => Meta | null, readonly refresh: () => Promise<FeedAnswer>,
 *   readonly put: (value: any) => void }} Feed
 */

/**
 * @typedef {{ token: string | null, data: any, at: number, error: unknown, put?: string | null }} Entry
 * @typedef {{ periods: number[], every: number, timer: number, due: boolean, out: AbortController | null | undefined,
 *   again: boolean, gone: boolean, waiting: ((a: FeedAnswer) => void)[], giving: boolean,
 *   run?: number, cur?: Entry | null, asked?: number, retime: () => void, api: Feed }} Source
 */

const MIN_EVERY = 1000;
const TIMEOUT = 30000;
const hidden = () => globalThis.document?.hidden === true;

// The options that make a source of its own; `every` is shared by its readers.
const KEYED = ["headers", "credentials", "parse", "accept", "expose"];

// A put value as JSON, to know a poll that brings the same data; null when
// it has none.
/** @param {unknown} v */
function json(v) {
  try {
    return JSON.stringify(v) ?? null;
  } catch {
    return null;
  }
}
/** @type {(put: string | null | undefined, data: unknown) => boolean} */
const same = (put, data) => put !== null && put === json(data);

/**
 * @param {string} src
 * @param {FeedOptions} opts
 * @returns {Source}
 */
function make(src, opts) {
  const [tick, setTick] = createSignal(0);
  const [meta, setMeta] = createSignal(/** @type {Meta | null} */ (null));
  const [given, give] = createSignal(/** @type {Entry | null} */ (null), { equals: false });
  /** @type {Source} */
  const f = /** @type {any} */ ({ periods: [], every: Infinity, timer: 0, due: false, out: undefined, again: false, gone: false, waiting: [], giving: false });
  // f.asked is the tick the memo last asked for, f.run counts its runs and
  // f.cur is the entry it holds, which a put compares itself with.
  // A put takes the place of a poll out, whose answer was asked before it.
  const entry = createMemo((/** @type {Entry | undefined} */ prev) => {
    const t = tick();
    const g = given();
    f.run = (f.run ?? 0) + 1;
    if (f.giving) {
      f.giving = false;
      f.cur = g;
      // A poll asked in the same flush as the put goes out after it.
      if (t !== f.asked) {
        f.asked = t;
        queueMicrotask(poll);
      }
      return /** @type {Entry} */ (g);
    }
    f.asked = t;
    return ask(prev);
  });
  /**
   * @param {Entry | undefined} prev
   * @returns {Promise<Entry>}
   */
  async function ask(prev) {
    const out = new AbortController();
    f.out = out;
    const run = f.run;
    // The refresh calls that asked for this poll hear its answer.
    const waiting = f.waiting;
    f.waiting = [];
    let status = 0;
    /** @type {FeedAnswer | undefined} */
    let heard;
    // Unless a put has taken its place, the entry this poll gives.
    /** @param {Entry} e */
    const hold = (e) => {
      if (f.run === run) f.cur = e;
      return e;
    };
    const limit = setTimeout(() => out.abort(new Error(`${src}: no answer in ${TIMEOUT} ms`)), TIMEOUT);
    try {
      /** @type {RequestInit} */
      const init = { cache: "no-store", signal: out.signal };
      // Untracked: a signal the function reads must not start a fetch of its own.
      if (opts.headers) init.headers = untrack(() => headersOf(opts.headers));
      if (opts.credentials) init.credentials = opts.credentials;
      const res = await fetch(src, init);
      // Its own signal, so a new Date leaves the data's readers be. It lands
      // a moment before the data does, while the poll is still pending.
      setMeta({ status: res.status, headers: pickHeaders(res, opts.expose), date: dateOf(res), at: Date.now() });
      status = res.status;
      if (!res.ok && !accepts(opts.accept, res.status)) {
        const error = /** @type {Error & { status?: number }} */ (new Error(`${src}: ${res.status} ${res.statusText ?? ""}`.trim()));
        error.status = res.status;
        throw error;
      }
      const text = await res.text();
      const token = `${res.status} ${text}`;
      if (prev?.token === token) {
        heard = { ok: true, status, data: prev.data };
        return hold(prev.error ? { ...prev, error: null } : prev);
      }
      const data = bodyOf(res.status, text, opts);
      heard = { ok: true, status, data };
      // The data a put gave: the entry stays, so its readers are left be.
      if (prev?.put !== undefined && same(prev.put, data)) {
        prev.token = token;
        prev.put = undefined;
        return hold(prev.error ? { ...prev, error: null } : prev);
      }
      return hold({ token, data, at: Date.now(), error: null });
    } catch (error) {
      heard = { ok: false, status, error };
      if (!prev) throw error;
      return hold({ ...prev, error });
    } finally {
      clearTimeout(limit);
      queueMicrotask(() => landed(out, waiting, Object.freeze(/** @type {FeedAnswer} */ (heard))));
    }
  }
  /** @type {(out: AbortController, waiting: ((a: FeedAnswer) => void)[], heard: FeedAnswer) => void} */
  const landed = (out, waiting, heard) => {
    // A step later the memo has taken the answer, so a refresh that hears it
    // finds it in data() and its effects have run.
    queueMicrotask(() => {
      try {
        flush();
      } finally {
        for (const done of waiting) done(heard);
      }
    });
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
    if (f.gone) return;
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
  /** @returns {FeedAnswer} */
  const goneError = () => Object.freeze({ ok: false, status: 0, error: new Error(`${src}: the feed is gone`) });
  onCleanup(() => {
    f.gone = true;
    clearTimeout(f.timer);
    f.out?.abort();
    for (const done of f.waiting.splice(0)) done(goneError());
    globalThis.document?.removeEventListener?.("visibilitychange", onVisible);
  });
  f.api = Object.freeze({
    src,
    data: () => entry().data,
    error: () => entry().error,
    at: () => entry().at,
    meta,
    refresh: () =>
      new Promise(/** @param {(a: FeedAnswer) => void} done */ (done) => {
        if (f.gone) {
          done(goneError());
          return;
        }
        f.waiting.push(done);
        poll();
      }),
    put: (/** @type {unknown} */ data) => {
      if (f.gone) return;
      // The same data as the readers have, with no poll out and no error:
      // nothing to tell.
      if (!f.out && !f.giving && f.cur && !f.cur.error) {
        const j = json(data);
        if (j !== null && j === (f.cur.put ?? json(f.cur.data))) return;
      }
      f.giving = true;
      give({ token: null, data, put: json(data), at: Date.now(), error: null });
    },
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
 *
 * refresh() polls now and returns a promise of that poll's answer, which
 * never rejects: `{ ok, status, data }` when it is data, `{ ok: false,
 * status, error }` when it failed, timed out or was aborted (status 0 when
 * no answer came). A refresh while a poll is out hears the poll that
 * follows it. put(value) makes `value` the data of every reader at once,
 * with no fetch, as if an answer had brought it; the poll goes on at its
 * period. A put while a poll is out takes the place of that poll's
 * answer, which was asked before the value was known.
 * @param {string} src
 * @param {FeedOptions} [options]
 * @returns {Feed}
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
