// The stream source (host/docs/frontend.md, "Streams"): one connection per
// address and options per page, over fetch so it can send headers, which
// EventSource cannot. It reads text/event-stream or NDJSON, hands each event
// to the islands' `on` callbacks, and connects again with a backoff.
//
//   const board = useBoard();
//   const live = stream("/api/events", {
//     format: "sse",                         // or "ndjson"
//     headers: () => ({ "X-Session": session() }),
//     on: board.actions.apply,               // (event) => void, event { type, data, id }
//   });
//   live.state()    "connecting", "open", "error" (a retry is due) or "closed"
//   live.error()    the last error, or null
//   live.status()   the last answer's status, or 0
//   live.reconnect()  drop the connection and connect now
//
// stream() is called inside an island's root, as feed() is. The connection
// closes when the last root that asked for it is gone. While the page is
// hidden an open connection stays, and a reconnect waits until it shows;
// with `hidden: "close"` the connection closes and opens again when the page
// shows. A 204 answer stops it, as it stops an EventSource.
import { createSignal, getOwner, onCleanup } from "./vendor/solid.js";
import { headersOf, keyOf } from "./request.js";
import { release, shared } from "./state.js";

const TIMEOUT = 30000;
const KEYED = ["format", "headers", "credentials", "parse", "hidden", "retry", "max"];
const hidden = () => globalThis.document?.hidden === true;

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
    else if (field === "retry" && /^\d+$/.test(value)) s.base = Math.max(min, Number(value));
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
  const s = { listeners: new Map(), out: null, timer: 0, tries: 0, base: min, lastId: "", due: false, done: false, gone: false };

  const emit = (type, text, id) => {
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

  const later = () => {
    if (s.gone || s.done) return;
    const wait = Math.min(max, s.base * 2 ** s.tries);
    s.tries += 1;
    clearTimeout(s.timer);
    s.timer = setTimeout(() => (hidden() ? (s.due = true) : connect()), wait);
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
    try {
      const headers = { ...headersOf(opts.headers) };
      if (s.lastId) headers["Last-Event-ID"] = s.lastId;
      const init = { headers, cache: "no-store", signal: out.signal };
      if (opts.credentials) init.credentials = opts.credentials;
      const res = await fetch(src, init);
      clearTimeout(limit);
      if (s.out !== out) return;
      setStatus(res.status);
      if (res.status === 204) {
        s.done = true;
        setState("closed");
        return;
      }
      if (!res.ok) {
        const e = new Error(`${src}: ${res.status} ${res.statusText ?? ""}`.trim());
        e.status = res.status;
        throw e;
      }
      s.tries = 0;
      setError(null);
      setState("open");
      await read(res, out);
      if (s.out !== out) return;
      // The server ended the stream: ask again.
      s.out = null;
      setState("connecting");
      later();
    } catch (e) {
      clearTimeout(limit);
      if (s.out !== out) return;
      s.out = null;
      setError(out.signal.aborted ? out.signal.reason : e);
      setState("error");
      later();
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
 * `data` is), `hidden` ("keep", the default, or "close"), and `retry` and
 * `max`, the first and the longest wait before connecting again in ms (1000
 * and 30000). `on` gets each event `{ type, data, id }` while its reader
 * lives; the same function given by two readers gets each event once.
 * @param {string} src
 * @param {{ format?: "sse" | "ndjson", headers?: object | (() => object), credentials?: RequestCredentials,
 *   parse?: "json" | "text", hidden?: "keep" | "close", retry?: number, max?: number,
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
