// The stream source (host/docs/frontend.md, "Streams"): one connection per
// address and options per page, over fetch so it can send headers, which
// EventSource cannot. It reads text/event-stream or NDJSON, hands each event
// to the islands' `on` callbacks, and connects again with a backoff.
//
//   const board = useBoard();
//   const live = stream("/api/events", {
//     format: "sse",                         // or "ndjson"
//     headers: sessionHeaders,               // one function per page
//     on: board.actions.apply,               // (event) => void, event { type, data, id }
//   });
//   live.state()    "connecting", "open", "error" (a retry is due) or "closed"
//   live.error()    the last error, or null; a refused answer's is a StreamError
//                   with its status and its body as data (JSON, or text)
//   live.status()   the last answer's status, or 0
//   live.reconnect()  drop the connection and connect now
//
// stream() is called inside an island's root, as feed() is. The connection
// closes when the last root that asked for it is gone. While the page is
// hidden an open connection stays, and a reconnect waits until it shows;
// with `hidden: "close"` the connection closes and opens again when the page
// shows; with `hidden: "live"` it asks again while hidden too. A 204 answer
// stops it, as it stops an EventSource.
//
// Options a page moved onto the kit uses to keep the waits and the ends it
// had (host/docs/frontend.md, "Streams"):
//
//   stream("/api/events", {
//     end: (status, type) => status !== 200 || type !== "text/event-stream",  // or [401, 403]
//     retry: 3000, grow: "fixed", ended: 3000,  // an EventSource's waits; a server retry field sets both
//     broken: "end",  // a break after the open waits as an end, and only failed connects back off
//     hidden: "live",
//   });
import { createSignal, getOwner, onCleanup } from "./vendor/solid.js";
import { headersOf, keyOf } from "./request.js";
import { release, shared } from "./state.js";

const TIMEOUT = 30000;
// The most of a refused answer's body that its error carries, and the
// longest wait for it.
const BODY = 65536;
const BODY_WAIT = 2000;
const KEYED = ["format", "headers", "credentials", "parse", "hidden", "retry", "max", "grow", "ended", "end", "broken"];
const hidden = () => globalThis.document?.hidden === true;

/** An answer the stream did not read: its status, and its body as data. */
export class StreamError extends Error {
  constructor(src, status, statusText, data) {
    super(`${src}: ${status} ${statusText ?? ""}`.trim());
    this.name = "StreamError";
    this.status = status;
    this.data = data;
  }
}

// At most BODY bytes of an answer, read for at most BODY_WAIT ms, as JSON
// when it is JSON, else text. An answer with no body is "".
async function bodyOf(res) {
  let text = "";
  const reader = res.body?.getReader?.();
  if (!reader) return text;
  const decoder = new TextDecoder();
  let timer;
  const late = new Promise((r) => (timer = setTimeout(() => r(null), BODY_WAIT)));
  try {
    let size = 0;
    for (;;) {
      const next = await Promise.race([reader.read(), late]);
      if (next === null || size >= BODY) {
        reader.cancel().catch(() => {});
        break;
      }
      if (next.done) break;
      const { value } = next;
      size += value.byteLength;
      text += decoder.decode(value.subarray(0, Math.max(0, value.byteLength - Math.max(0, size - BODY))), { stream: true });
    }
  } catch {
    // What came before the failure is the body.
  } finally {
    clearTimeout(timer);
  }
  text += decoder.decode();
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

// Whether an answer ends the stream for good: `end` lists statuses or is a
// function of the status and the content type.
function ends(end, status, type) {
  if (Array.isArray(end)) return end.includes(status);
  return typeof end === "function" && end(status, type) === true;
}

// Reads text/event-stream lines into events (the WHATWG rules: data lines
// joined by "\n", a blank line ends an event, an event with no data is
// dropped, an id counts for Last-Event-ID once its event is dispatched, retry
// sets the wait, no shorter than the first wait the reader asked for).
function sse(emit, s, min) {
  let data = null;
  let type = "";
  let id = s.lastId;
  return (line) => {
    if (line === "") {
      s.lastId = id;
      if (data !== null) emit(type || "message", data, id);
      data = null;
      type = "";
      return;
    }
    if (line.startsWith(":")) return;
    const at = line.indexOf(":");
    const field = at < 0 ? line : line.slice(0, at);
    let value = at < 0 ? "" : line.slice(at + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "data") data = data === null ? value : `${data}\n${value}`;
    else if (field === "event") type = value;
    else if (field === "id" && !value.includes("\0")) id = value;
    else if (field === "retry" && /^\d+$/.test(value)) {
      s.base = Math.max(min, Number(value));
      s.served = true;
    }
  };
}

function ndjson(emit) {
  return (line) => {
    if (line.trim() !== "") emit("message", line, null);
  };
}

function make(src, opts) {
  const [state, setState] = createSignal("connecting");
  const [error, setError] = createSignal(null);
  const [status, setStatus] = createSignal(0);
  const min = Math.max(100, opts.retry ?? 1000);
  const max = Math.max(min, opts.max ?? 30000);
  const fixed = opts.grow === "fixed";
  const ended = Number.isFinite(opts.ended) ? Math.max(0, opts.ended) : null;
  const live = opts.hidden === "live";
  const broke = opts.broken === "end";
  // heard: this connection brought an event; quiet: clean ends in a row
  // that brought none; served: the server sent a retry field.
  const s = { listeners: new Map(), out: null, timer: 0, tries: 0, base: min, lastId: "", due: false, done: false, gone: false };
  s.quiet = 0;

  const emit = (type, text, id) => {
    s.heard = true;
    let data = text;
    if (opts.parse !== "text") {
      try {
        data = JSON.parse(text);
      } catch (e) {
        setError(new Error(`${src}: an event that is not JSON: ${e.message}`));
        return;
      }
    }
    const event = Object.freeze({ type, data, id });
    for (const on of [...s.listeners.keys()]) {
      try {
        on(event);
      } catch (e) {
        setError(e);
      }
    }
  };

  // `after` is the wait when the server ended the stream, when set, `spent`
  // the time already gone since the answer came, and `fail` whether this
  // wait counts as a try. No wait is longer than max.
  const later = (after, spent = 0, fail = true) => {
    if (s.gone || s.done) return;
    const wait = Math.min(max, after ?? (fixed ? s.base : s.base * 2 ** s.tries));
    if (fail) s.tries += 1;
    clearTimeout(s.timer);
    s.timer = setTimeout(() => (hidden() && !live ? (s.due = true) : connect()), Math.max(0, wait - spent));
  };

  // The wait after a clean end with `ended` set. A server retry field
  // replaces it, as it does for an EventSource. A second clean end in a row
  // with no event backs off, so a server that ends every stream at once is
  // not asked in a loop.
  const afterEnd = () => {
    if (s.served) return s.base;
    s.quiet = s.heard ? 0 : s.quiet + 1;
    if (s.quiet < 2) return ended;
    return Math.max(ended, fixed ? s.base : s.base * 2 ** (s.quiet - 2));
  };

  async function read(res, out) {
    const nd = opts.format === "ndjson";
    const line = nd ? ndjson(emit) : sse(emit, s, min);
    const decoder = new TextDecoder();
    const reader = res.body.getReader();
    out.signal.addEventListener("abort", () => reader.cancel().catch(() => {}));
    let rest = "";
    for (;;) {
      const { done, value } = await reader.read();
      if (s.out !== out) return;
      rest += done ? decoder.decode() : decoder.decode(value, { stream: true });
      // A "\r" at the end may be the first half of "\r\n".
      const lines = rest.split(/\r\n|\r(?!$)|\n/);
      rest = lines.pop();
      for (const l of lines) line(l);
      if (done) {
        if (rest.endsWith("\r")) line(rest.slice(0, -1));
        // NDJSON counts a last line with no newline; an SSE event not yet
        // ended by a blank line is dropped, as EventSource drops it.
        else if (nd) line(rest);
        return;
      }
    }
  }

  async function connect() {
    clearTimeout(s.timer);
    s.due = false;
    s.out?.abort();
    if (s.gone || s.done) return;
    const out = new AbortController();
    s.out = out;
    setState("connecting");
    const limit = setTimeout(() => out.abort(new Error(`${src}: no answer in ${TIMEOUT} ms`)), TIMEOUT);
    let spent = 0;
    let opened = false;
    try {
      const headers = { ...headersOf(opts.headers) };
      if (s.lastId) headers["Last-Event-ID"] = s.lastId;
      const init = { headers, cache: "no-store", signal: out.signal };
      if (opts.credentials) init.credentials = opts.credentials;
      const res = await fetch(src, init);
      if (s.out !== out) {
        clearTimeout(limit);
        return;
      }
      setStatus(res.status);
      if (res.status === 204) {
        clearTimeout(limit);
        s.done = true;
        setState("closed");
        return;
      }
      const type = (res.headers?.get?.("content-type") ?? "").split(";")[0].trim().toLowerCase();
      const end = ends(opts.end, res.status, type);
      if (!res.ok || end) {
        clearTimeout(limit);
        const came = Date.now();
        const e = new StreamError(src, res.status, res.statusText, await bodyOf(res));
        spent = Date.now() - came;
        if (s.out !== out) return;
        if (end) {
          s.out = null;
          s.done = true;
          setError(e);
          setState("closed");
          return;
        }
        throw e;
      }
      clearTimeout(limit);
      s.tries = 0;
      s.heard = false;
      setError(null);
      setState("open");
      opened = true;
      await read(res, out);
      if (s.out !== out) return;
      // The server ended the stream: ask again.
      s.out = null;
      setState("connecting");
      later(ended === null ? undefined : afterEnd(), 0, !broke);
    } catch (e) {
      clearTimeout(limit);
      if (s.out !== out) return;
      s.out = null;
      setError(out.signal.aborted ? out.signal.reason : e);
      setState("error");
      if (opened && broke) later(ended === null ? undefined : afterEnd(), 0, false);
      else later(undefined, spent);
    }
  }

  const close = () => {
    clearTimeout(s.timer);
    const out = s.out;
    s.out = null;
    out?.abort();
  };

  const onVisible = () => {
    if (s.gone || s.done) return;
    if (hidden()) {
      if (opts.hidden === "close") {
        close();
        s.due = true;
        setState("closed");
      }
    } else if (s.due) {
      s.tries = 0;
      connect();
    }
  };
  globalThis.document?.addEventListener?.("visibilitychange", onVisible);
  onCleanup(() => {
    s.gone = true;
    close();
    globalThis.document?.removeEventListener?.("visibilitychange", onVisible);
  });

  if (hidden() && opts.hidden === "close") {
    s.due = true;
    setState("closed");
  } else connect();

  s.api = Object.freeze({
    src,
    state,
    error,
    status,
    reconnect: () => {
      if (s.gone) return;
      s.done = false;
      s.tries = 0;
      connect();
    },
  });
  return s;
}

/**
 * The stream of `src` for this page, made on first use. Readers that give
 * the same options share one connection: `format` ("sse", the default, or
 * "ndjson"), `headers` (an object, or a function read on each connect),
 * `credentials`, `parse` ("json", the default, or "text": what an event's
 * `data` is), `hidden` ("keep", the default, "close" or "live"), and `retry`
 * and `max`, the first and the longest wait before connecting again in ms
 * (1000 and 30000). `grow` is "double", the default, or "fixed": the wait
 * stays `retry`. `ended` is the wait in ms after the server ends a stream
 * cleanly, 0 or more; by default the first wait. A server `retry:` field
 * replaces it, and a second clean end in a row with no event backs off as
 * a failure does. `broken` is "fail", the default, or "end": a stream that
 * breaks after it opened waits as a clean end does, and an end or a break
 * starts the backoff again, so only failed connects grow it. `end` ends
 * the stream for good on an answer it lists, a status array or a function
 * of the status and the content type that returns true: state "closed",
 * with status() and error() kept. `on` gets each event `{ type, data, id }` while its reader
 * lives; the same function given by two readers gets each event once.
 * @param {string} src
 * @param {{ format?: "sse" | "ndjson", headers?: object | (() => object), credentials?: RequestCredentials,
 *   parse?: "json" | "text", hidden?: "keep" | "close" | "live", retry?: number, max?: number,
 *   grow?: "double" | "fixed", ended?: number, broken?: "fail" | "end", end?: number[] | ((status: number, type: string) => boolean),
 *   on?: (event: { type: string, data: any, id: string | null }) => void }} [options]
 * @returns {{ src: string, state: () => "connecting" | "open" | "error" | "closed", error: () => unknown,
 *   status: () => number, reconnect: () => void }}
 */
export function stream(src, options = {}) {
  if (!getOwner()) throw new Error(`stream ${src}: call it inside an island's root`);
  const key = keyOf("stream", src, options, KEYED);
  const s = shared(key, () => make(src, options));
  const on = options.on;
  if (on) s.listeners.set(on, (s.listeners.get(on) ?? 0) + 1);
  s.readers = (s.readers ?? 0) + 1;
  onCleanup(() => {
    if (on) {
      const n = s.listeners.get(on) - 1;
      if (n > 0) s.listeners.set(on, n);
      else s.listeners.delete(on);
    }
    s.readers -= 1;
    if (s.readers === 0) release(key);
  });
  return s.api;
}
