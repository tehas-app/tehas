// What the kit's modules that make requests share about a request and its
// answer. No library.

/**
 * @typedef {Record<string, string> | (() => Record<string, string>)} Headers
 *   Headers to send: an object, or a function read on each request.
 * @typedef {Record<number, unknown>} Accept
 *   A status that is data: its value, or a function of the body's text.
 * @typedef {"json" | "text"} Parse
 */

/**
 * The headers to send: `headers` as given, or what it returns when it is a function.
 * @param {Headers} [headers]
 * @returns {Record<string, string>}
 */
export function headersOf(headers) {
  const h = typeof headers === "function" ? headers() : headers;
  return h ?? {};
}

/** @type {WeakMap<Function, number>} */
const ids = new WeakMap();
let next = 0;
/** @param {Function} fn */
const idOf = (fn) => {
  if (!ids.has(fn)) ids.set(fn, ++next);
  return `fn#${ids.get(fn)}`;
};

// An object's keys in order, so the same headers written in another order
// are the same source.
/** @type {(_k: string, v: any) => unknown} */
const canon = (_k, v) => {
  if (typeof v === "function") return idOf(v);
  if (!v || typeof v !== "object" || Array.isArray(v)) return v;
  /** @type {Record<string, unknown>} */
  const out = {};
  for (const k of Object.keys(v).sort()) out[k] = v[k];
  return out;
};

/**
 * The page key of a source: `<kind>:<src>`, then the options named in
 * `fields` that are set, as JSON with each object's keys sorted. A function
 * counts by its identity, so it is defined once per page, not in each island.
 * @param {string} kind
 * @param {string} src
 * @param {Record<string, unknown>} options
 * @param {string[]} fields
 */
export function keyOf(kind, src, options, fields) {
  /** @type {Record<string, unknown>} */
  const picked = {};
  for (const f of fields) if (options[f] !== undefined) picked[f] = options[f];
  if (Object.keys(picked).length === 0) return `${kind}:${src}`;
  return `${kind}:${src} ${JSON.stringify(picked, canon)}`;
}

/**
 * The server's Date header of an answer as ms, or null.
 * @param {Response} res
 */
export function dateOf(res) {
  const ms = Date.parse(res.headers?.get?.("date") ?? "");
  return Number.isFinite(ms) ? ms : null;
}

/**
 * The headers named in `names` of an answer, by lower-case name, null where absent.
 * @param {Response} res
 * @param {string[]} [names]
 */
export function pickHeaders(res, names = []) {
  /** @type {Record<string, string | null>} */
  const out = {};
  for (const n of names) out[n.toLowerCase()] = res.headers?.get?.(n) ?? null;
  return out;
}

/**
 * Whether `accept` lists `status` as data.
 * @param {Accept | undefined} accept
 * @param {number} status
 * @returns {accept is Accept}
 */
export const accepts = (accept, status) => Object.hasOwn(accept ?? {}, status);

/**
 * What an answer's body means: `accept[status]` when the status is listed
 * (a function is called with the body's text), else the text parsed by
 * `parse`, "json" or "text".
 * @param {number} status
 * @param {string} text
 * @param {{ accept?: Accept, parse?: Parse }} [options]
 * @returns {any}
 */
export function bodyOf(status, text, { accept, parse = "json" } = {}) {
  if (accepts(accept, status)) {
    const v = accept[status];
    return typeof v === "function" ? v(text) : v;
  }
  return parse === "text" ? text : JSON.parse(text);
}
