// The state row of the plain dashboard: the factory's state, the all-hands and
// read-only marks, how old the queue is and the link to all issues, from
// live/status.json and live/running.json. Pure: the clock is nowMs.
import "../console-tabs.js";
import { ago, webHref } from "../host-data.js";

const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : {});

export function stateModel(status, running, nowMs) {
  const s = obj(status);
  const v = globalThis.TEHAS_CONSOLE_TABS.view(running, null);
  // A paused factory still finishes the cycle it has. Saying "working" alone
  // would hide the pause; saying "paused" alone would hide the cycle.
  const finishing = v.tabs.filter((t) => t.running).map((t) => t.issue);
  let pill;
  if (s.paused) pill = ["pause", finishing.length ? "paused, finishing #" + finishing.join(", #") : "paused"];
  else if (v.running) pill = ["run", v.running > 1 ? "working on " + v.running : "working"];
  else if (s.noResources) pill = ["pause", "no resources"];
  else pill = ["idle", "idle"];
  return {
    pill: "pill " + pill[0],
    state: pill[1],
    // Reported only: all-hands changes nothing in this factory.
    allHands: s.allHands === true,
    readOnly: Boolean(obj(s.readOnly).on),
    fresh: "queue read " + ago(s.generatedAt, nowMs),
    all: webHref(obj(s.links).all),
  };
}
