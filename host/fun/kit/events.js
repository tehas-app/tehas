// The factory's events on a page (host/docs/api.md, "Events"): the names of
// api/events.txt, a page-level target and `raise`, which dispatches an
// envelope as a CustomEvent named `tehas:<event>`. No library.
//
//   import { events, raise } from "./kit/events.js";
//   events.addEventListener("tehas:cycle.ended", (e) => show(e.detail.data));
//   raise(envelope);   // throws a TypeError on another name, field or when
//
// raise checks the envelope's fields and the name. It does not check each
// name's `data` or the shape of its key: api/event-page.schema.json does.
//
// Only an `after` envelope reaches a page: gates never do. tehas's own pages
// raise nothing on it until 2.1; a page that learns a factory fact from its
// own stream may.

/**
 * @typedef {{ readonly api: 1, readonly source?: string, readonly event: string,
 *   readonly when: "after", readonly key: string, readonly at: string,
 *   readonly data: Record<string, unknown> }} Envelope
 */

/** The names of api/events.txt, in its order. */
export const names = Object.freeze([
  "cycle.started",
  "cycle.parked",
  "question.asked",
  "cycle.ended",
  "change.offered",
  "factory.limit-changed",
  "ticket.acted",
  "ticket.closed",
  "change.merged",
]);

/** The page's target: one per page, as a module is evaluated once. */
export const events = new EventTarget();

const FIELDS = new Set(["api", "source", "event", "when", "key", "at", "data"]);
const TIME = /^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]Z$/;
const KEY = /^\S{1,200}$/u;
const SOURCE = /^[a-z0-9-]{1,64}$/;

/**
 * Why `e` is no envelope a page takes, or "" when it is one.
 * @param {any} e
 */
function wrong(e) {
  if (!e || typeof e !== "object" || Array.isArray(e)) return "not an object";
  for (const k of Object.keys(e)) if (!FIELDS.has(k)) return `the field ${k} is not an envelope's`;
  if (e.api !== 1) return "api is not 1";
  if (!names.includes(e.event)) return `the name ${e.event} is not in api/events.txt`;
  if (e.when !== "after") return "only an after envelope reaches a page";
  if (typeof e.key !== "string" || !KEY.test(e.key)) return "the key is not 1 to 200 characters with no white space";
  if (typeof e.at !== "string" || !TIME.test(e.at)) return "at is not a whole-second UTC time";
  if ("source" in e && (typeof e.source !== "string" || !SOURCE.test(e.source))) return "the source is not a factory's name";
  if (!e.data || typeof e.data !== "object" || Array.isArray(e.data)) return "data is not an object";
  return "";
}

/**
 * Dispatches `envelope` on `events` as `tehas:<event>`, its `detail` the
 * envelope. It does not bubble.
 * @param {Envelope} envelope
 */
export function raise(envelope) {
  const why = wrong(envelope);
  if (why) throw new TypeError(`raise: ${why}`);
  events.dispatchEvent(new CustomEvent(`tehas:${envelope.event}`, { detail: envelope }));
}
