// A press (host/docs/frontend.md, "Presses"): one request, a post by default,
// with headers, a JSON or form body and a rule for its status. No library: a
// press runs inside Solid's action().
//
//   const save = action(function* (n) {
//     setPicked(n);                                  // createOptimistic: shows at once
//     yield press("/api/pick", { body: { n } });     // a failure rejects; the guess reverts
//     live.refresh();                                // the truth, held with the action
//   });
//
// press() resolves { status, ok, data, headers, date } and rejects with a
// PressError (status, data) for a status that is neither 2xx nor in `accept`.
import { accepts, bodyOf, dateOf, headersOf, pickHeaders } from "./request.js";
/** @import { Accept, Headers, Parse } from "./request.js" */

/**
 * @typedef {{ method?: string, headers?: Headers, body?: unknown, credentials?: RequestCredentials, parse?: Parse,
 *   accept?: Accept, expose?: string[], signal?: AbortSignal }} PressOptions
 * @typedef {{ status: number, ok: boolean, data: any, headers: Record<string, string | null>,
 *   date: number | null }} PressAnswer
 */

/** A press answered with a status that is neither 2xx nor accepted. */
export class PressError extends Error {
  /**
   * @param {string} src
   * @param {number} status
   * @param {string | undefined} statusText
   * @param {any} data
   */
  constructor(src, status, statusText, data) {
    super(`${src}: ${status} ${statusText ?? ""}`.trim());
    this.name = "PressError";
    this.status = status;
    this.data = data;
  }
}

// What fetch sends as it is, with its own content type: a form's data among them.
const NATIVE = ["FormData", "URLSearchParams", "Blob", "ArrayBuffer"];
/**
 * @param {unknown} b
 * @returns {b is BodyInit}
 */
const native = (b) => ArrayBuffer.isView(b) || NATIVE.some((n) => /** @type {any} */ (globalThis)[n] && b instanceof /** @type {any} */ (globalThis)[n]);
/** @param {unknown} b */
const plain = (b) =>
  ["number", "boolean"].includes(typeof b) ||
  (typeof b === "object" && (b === null || Array.isArray(b) || [Object.prototype, null].includes(Object.getPrototypeOf(b))));

/**
 * Sends one request. `body` that is a string, FormData, URLSearchParams, a
 * Blob or bytes goes as it is; a plain object, an array, a number, a boolean
 * or null goes as JSON with its content type; anything else is refused.
 * `parse` reads the answer ("json", the default, or "text"; an empty body is
 * null). A status in `accept` resolves with its value instead
 * of rejecting. `signal` aborts it.
 * @param {string} src
 * @param {PressOptions} [options]
 * @returns {Promise<PressAnswer>}
 */
export async function press(src, options = {}) {
  const { method = "POST", body, credentials, signal, expose } = options;
  const headers = { ...headersOf(options.headers) };
  /** @type {RequestInit} */
  const init = { method, headers, cache: "no-store" };
  if (credentials) init.credentials = credentials;
  if (signal) init.signal = signal;
  if (body !== undefined) {
    if (typeof body === "string" || native(body)) init.body = body;
    else if (plain(body)) {
      init.body = JSON.stringify(body);
      if (!Object.keys(headers).some((k) => k.toLowerCase() === "content-type")) {
        headers["Content-Type"] = "application/json";
      }
    } else throw new TypeError(`${src}: a body press cannot send`);
  }
  const res = await fetch(src, init);
  const text = await res.text();
  const meta = { status: res.status, headers: pickHeaders(res, expose), date: dateOf(res) };
  const listed = accepts(options.accept, res.status);
  if (res.ok || listed) {
    return { ...meta, ok: res.ok, data: text === "" && !listed ? null : bodyOf(res.status, text, options) };
  }
  let data = text;
  try {
    data = JSON.parse(text);
  } catch {
    // The body stays text.
  }
  throw new PressError(src, res.status, res.statusText, data);
}
