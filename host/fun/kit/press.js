// A press (host/docs/frontend.md, "Presses"): one request, a post by default,
// with headers, a JSON body and a rule for its status. No library: a press
// runs inside Solid's action().
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

/** A press answered with a status that is neither 2xx nor accepted. */
export class PressError extends Error {
  constructor(src, status, statusText, data) {
    super(`${src}: ${status} ${statusText ?? ""}`.trim());
    this.name = "PressError";
    this.status = status;
    this.data = data;
  }
}

/**
 * Sends one request. `body` that is not a string goes as JSON with its
 * content type; `parse` reads the answer ("json", the default, or "text"; an
 * empty body is null). A status in `accept` resolves with its value instead
 * of rejecting. `signal` aborts it.
 * @param {string} src
 * @param {{ method?: string, headers?: object | (() => object), body?: any, credentials?: RequestCredentials,
 *   parse?: "json" | "text", accept?: Record<number, any>, expose?: string[], signal?: AbortSignal }} [options]
 * @returns {Promise<{ status: number, ok: boolean, data: any, headers: Record<string, string | null>, date: number | null }>}
 */
export async function press(src, options = {}) {
  const { method = "POST", body, credentials, signal, expose } = options;
  const headers = { ...headersOf(options.headers) };
  const init = { method, headers, cache: "no-store" };
  if (credentials) init.credentials = credentials;
  if (signal) init.signal = signal;
  if (body !== undefined) {
    if (typeof body === "string") init.body = body;
    else {
      init.body = JSON.stringify(body);
      if (!Object.keys(headers).some((k) => k.toLowerCase() === "content-type")) {
        headers["Content-Type"] = "application/json";
      }
    }
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
