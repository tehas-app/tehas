// What the kit's modules that make requests share about a request and its
// answer. No library.

/** The headers to send: `headers` as given, or what it returns when it is a function. */
export function headersOf(headers) {
  const h = typeof headers === "function" ? headers() : headers;
  return h ?? {};
}

const ids = new WeakMap();
let next = 0;
const idOf = (fn) => {
  if (!ids.has(fn)) ids.set(fn, ++next);
  return `fn#${ids.get(fn)}`;
};

// An object's keys in order, so the same headers written in another order
// are the same source.
const canon = (_k, v) => {
  if (typeof v === "function") return idOf(v);
  if (!v || typeof v !== "object" || Array.isArray(v)) return v;
  const out = {};
  for (const k of Object.keys(v).sort()) out[k] = v[k];
  return out;
};

/**
 * The page key of a source: `<kind>:<src>`, then the options named in
 * `fields` that are set, as JSON with each object's keys sorted. A function
 * counts by its identity, so it is defined once per page, not in each island.
 */
export function keyOf(kind, src, options, fields) {
  const picked = {};
  for (const f of fields) if (options[f] !== undefined) picked[f] = options[f];
  if (Object.keys(picked).length === 0) return `${kind}:${src}`;
  return `${kind}:${src} ${JSON.stringify(picked, canon)}`;
}

/** The server's Date header of an answer as ms, or null. */
export function dateOf(res) {
  const ms = Date.parse(res.headers?.get?.("date") ?? "");
  return Number.isFinite(ms) ? ms : null;
}

/** The headers named in `names` of an answer, by lower-case name, null where absent. */
export function pickHeaders(res, names = []) {
  const out = {};
  for (const n of names) out[n.toLowerCase()] = res.headers?.get?.(n) ?? null;
  return out;
}

/** Whether `accept` lists `status` as data. */
export const accepts = (accept, status) => Object.hasOwn(accept ?? {}, status);

/**
 * What an answer's body means: `accept[status]` when the status is listed
 * (a function is called with the body's text), else the text parsed by
 * `parse`, "json" or "text".
 */
export function bodyOf(status, text, { accept, parse = "json" } = {}) {
  if (accepts(accept, status)) {
    const v = accept[status];
    return typeof v === "function" ? v(text) : v;
  }
  return parse === "text" ? text : JSON.parse(text);
}
