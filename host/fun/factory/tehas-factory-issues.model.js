// One issue list of the plain dashboard (in line, waiting for review, stopped,
// completed) from live/status.json: the first ten rows, the count and a link
// to the rest. `host` is what the host bridge says (host-bridge.js): where a
// ticket links and its notes. Pure: the clock is nowMs.
import { ago, webHref } from "../host-data.js";

export const MAX = 10;

const LISTS = {
  queued: ["In line", "Nothing is waiting. Label an issue tehas:ready.", "updatedAt"],
  awaiting: ["Waiting for your review", "No pull request is waiting.", "updatedAt"],
  stopped: ["Stopped, needs attention", "Nothing is stopped.", "updatedAt"],
  completed: ["Completed", "Nothing completed yet.", "closedAt"],
};

const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : {});
export const NO_HOST = { ticket: () => null, notes: () => [] };

// A host's notes on a ticket as chips: a host page opens in the same tab,
// another site in a new one.
export function chips(list) {
  return (Array.isArray(list) ? list : []).map((n) => ({
    text: String(n.text),
    href: n.link || null,
    away: Boolean(n.link) && !n.link.startsWith("/host/"),
  }));
}

export function issuesModel(status, props, nowMs, host = NO_HOST) {
  const name = Object.hasOwn(LISTS, props.list) ? props.list : "queued";
  const [title, empty, when] = LISTS[name];
  const s = obj(status);
  const items = (Array.isArray(s[name]) ? s[name] : []).filter((it) => it && typeof it === "object");
  return {
    name,
    title,
    empty,
    n: items.length ? "(" + items.length + ")" : "",
    rows: items.slice(0, MAX).map((it) => ({
      number: it.number,
      title: String(it.title ?? ""),
      href: host.ticket(s.repo, it.url) || webHref(it.url),
      notes: chips(host.notes(it.number)),
      when: ago(it[when] || it.updatedAt, nowMs),
    })),
    more: items.length > MAX ? { text: "all " + items.length + " on GitHub", href: webHref(obj(s.links)[name]) } : null,
  };
}
