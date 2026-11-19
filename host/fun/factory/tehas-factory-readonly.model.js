// A read-only factory (TEHAS_READ_ONLY=1) opens no pull request: what each of
// its starts left, from live/status.json, the branch in its workspace and the
// pull request it would have opened. Pure: the clock is nowMs.
import { ago } from "../host-data.js";
import { MAX, NO_HOST, chips } from "./tehas-factory-issues.model.js";

const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : {});

export function readonlyModel(status, nowMs, host = NO_HOST) {
  const ro = obj(obj(status).readOnly);
  const on = Boolean(ro.on);
  const starts = on && Array.isArray(ro.starts) ? ro.starts.filter((r) => r && typeof r === "object") : [];
  return {
    on,
    n: starts.length ? "(" + starts.length + ")" : "",
    rows: starts.slice(0, MAX).map((r) => {
      const pr = r.pr && typeof r.pr === "object" ? r.pr : null;
      return {
        issue: r.issue,
        notes: chips(host.notes(r.issue)),
        text: pr ? String(pr.title ?? "") : r.how + " started, no pull request yet",
        where: (pr && pr.branch ? pr.branch + " in " : "") + (r.workspace || "") + (r.at ? ", " + ago(r.at, nowMs) : ""),
        body: pr && pr.body ? String(pr.body) : "",
      };
    }),
  };
}
