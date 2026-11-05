// Drives five lean endings through pi-rukas's own work-lean verb and prints
// the schema-5 document each one writes: the fixtures beside this file, read by
// console/cycle/tests/lean.rs. Made at pi-rukas v0.14.53: copy this file to its
// extension/smoke-tests/lib/, run `bun run smoke-tests/lib/ends.ts` there, and
// write `.<name>.doc` of each ending to <name>.json.
import type { Step } from "../../src/lean/state.ts";
import { fakeRun, verbRun } from "./work-lean-verb-fixture.ts";

const PR = "https://github.com/acme/widget/pull/41";
const HEAD = "b".repeat(39) + "2";
let t = 1_793_525_000_000;
function s(state: Step["state"], outcome: Step["outcome"]): Step {
  t += 60_000;
  return { key: `7:${state}:0`, state, startedAt: t, endedAt: t + 30_000, outcome };
}
const dev = [{ role: "developer", ms: 10, ok: true, usage: { input: 1200, output: 300, cacheRead: 50, cacheWrite: 0, cost: 0, turns: 6 } }];
const front = () => [s("ready", { kind: "ok" }), s("specify", { kind: "ok", dispatches: dev }), s("build", { kind: "ok", dispatches: dev })];
const publish = (draft: boolean) => s("publish", { kind: "ok", prUrl: PR, draft, publishedHead: HEAD, reason: `published ${draft ? "draft " : ""}PR ${PR} at ${HEAD.slice(0, 12)}` });
const handback = (report: string) => s("handback", { kind: "done", posted: false, channel: "host", report, reason: "handback comment not posted: the host delivers it" } as never);

const ends: Record<string, { code: number; state: "landed" | "handed-back" | "not-ready"; steps: Step[] }> = {
  landed: { code: 0, state: "landed", steps: [...front(), s("verify", { kind: "green", verifiedHead: HEAD }), publish(false), s("review", { kind: "done" }), s("ci", { kind: "green" }), s("land", { kind: "merged", reason: `merged ${PR}` } as never)] },
  green: { code: 10, state: "handed-back", steps: [...front(), s("verify", { kind: "green", verifiedHead: HEAD }), publish(false), s("review", { kind: "done" }), s("ci", { kind: "green" }), s("land", { kind: "refused", reason: "autoMerge is not true in .pi/work.json at the run's base (the config was read, and it did not grant the merge)" }), handback(`The pull request ${PR} is green and waits for a person to merge it.`)] },
  draft: { code: 10, state: "handed-back", steps: [...front(), s("verify", { kind: "red", reason: "acceptance test test-ac2 is red" }), publish(true), handback(`The gate is red: acceptance test test-ac2 is red. The draft pull request ${PR} holds the work so far.`)] },
  nopr: { code: 10, state: "handed-back", steps: [s("ready", { kind: "ok" }), s("specify", { kind: "ok", dispatches: dev }), s("build", { kind: "exhausted", reason: "the developer produced no change", dispatches: dev }), handback("The build produced no change, so nothing was published.")] },
  notready: { code: 11, state: "not-ready", steps: [s("ready", { kind: "fail", failedRules: ["no pi-spec v1 block"] } as never), s("decline", { kind: "done", posted: false, channel: "host", reason: "no pi-spec v1 block" } as never)] },
};
const out: Record<string, unknown> = {};
for (const [name, e] of Object.entries(ends)) {
  const base = fakeRun({ code: e.code, state: e.state, steps: e.steps });
  base.run.branch = "";
  const r = await verbRun(["7"], async () => ({ summary: `work-lean #7: ${e.state} (exit ${e.code})`, result: base }));
  out[name] = { code: r.code, doc: r.doc, violations: r.violations };
}
console.log(JSON.stringify(out, null, 1));
