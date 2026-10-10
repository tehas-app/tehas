// Turn a running /work cycle into one readable, append-only console file
// for the dashboard.
//
//   deno run -A console.ts --rpc <work-N.jsonl> --runs <ensemble-runs dir>
//     --out <console.txt> --since <epoch seconds>
//
// Follows two sources until it is killed:
//   - the cycle's Pi RPC stream: driver notices and step boundaries,
//   - every agent transcript touched since the cycle began: what each agent
//     said, the commands it ran and the start of what came back.
//
// Output is plain text. Tokens, database passwords and private network
// addresses are masked, because an agent can print its environment and this
// file is served by the dashboard, which may be on the internet. The mask is
// a net for accidents, not a wall: an agent that means to can print a secret
// in a form no pattern knows.

const arg = (name: string): string => {
  const i = Deno.args.indexOf(`--${name}`);
  if (i < 0 || !Deno.args[i + 1]) {
    console.error(`console.ts: --${name} is required`);
    Deno.exit(2);
  }
  return Deno.args[i + 1];
};
const rpcPath = arg("rpc");
const runsDir = arg("runs");
const outPath = arg("out");
const since = Number(arg("since")) * 1000;

const out = await Deno.open(outPath, {
  create: true,
  append: true,
  mode: 0o644,
});
const enc = new TextEncoder();

function mask(s: string): string {
  return s
    .replace(/\b(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}/g, "$1***")
    .replace(/(postgres(?:ql)?:\/\/[^:\s/]+:)[^@\s]+@/g, "$1***@")
    .replace(
      /\b([A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)\S+/g,
      "$1***",
    )
    // A private network address: an agent can read the container's hosts
    // file, and a connection error names the address it failed to reach.
    .replace(
      /(?<![\d.])(?:10\.\d{1,3}|192\.168|172\.(?:1[6-9]|2\d|3[01])|169\.254|100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7]))\.\d{1,3}\.\d{1,3}(?![\d])/g,
      "[private address]",
    )
    .replace(
      /\b[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.ts\.net\b/g,
      "[private name]",
    );
}

async function emit(who: string, text: string, at?: string) {
  const t = (at ? new Date(at) : new Date()).toISOString().slice(11, 19);
  const lines = mask(text).replace(/\r/g, "").split("\n");
  const body = lines.map((l, i) =>
    i === 0 ? `${t} ${who.padEnd(10)} ${l}` : `${" ".repeat(20)}${l}`
  ).join("\n");
  await out.write(enc.encode(body + "\n"));
}

const clip = (s: string, chars: number, lines: number): string => {
  const kept = s.split("\n").slice(0, lines).join("\n").slice(0, chars);
  return kept.length < s.length ? kept.trimEnd() + "\n[...]" : kept;
};

// Byte offsets already consumed, per file, and a carry for a partial line.
const offsets = new Map<string, number>();
const carry = new Map<string, string>();
const dec = new TextDecoder();

async function newLines(path: string): Promise<string[]> {
  let size: number;
  try {
    size = (await Deno.stat(path)).size;
  } catch {
    return [];
  }
  const from = offsets.get(path) ?? 0;
  if (size <= from) return [];
  const f = await Deno.open(path);
  try {
    await f.seek(from, Deno.SeekMode.Start);
    const buf = new Uint8Array(size - from);
    let read = 0;
    while (read < buf.length) {
      const n = await f.read(buf.subarray(read));
      if (n === null) break;
      read += n;
    }
    offsets.set(path, from + read);
    const text = (carry.get(path) ?? "") + dec.decode(buf.subarray(0, read));
    const parts = text.split("\n");
    carry.set(path, parts.pop() ?? "");
    return parts.filter((l) => l.length > 0);
  } finally {
    f.close();
  }
}

// deno-lint-ignore no-explicit-any
type Json = any;

async function rpcLine(line: string) {
  let r: Json;
  try {
    r = JSON.parse(line);
  } catch {
    return;
  }
  if (r.type === "extension_ui_request" && r.method === "notify") {
    await emit("driver", String(r.message ?? ""));
  } else if (
    r.type === "entry_appended" &&
    r.entry?.customType === "ensemble:lifecycle"
  ) {
    const d = r.entry.data ?? {};
    const step = d.stepNumber ? `step ${d.stepNumber}/${d.stepTotal} ` : "";
    if (d.kind === "step-started") {
      await emit(
        "driver",
        `--- ${step}${d.label} started (round ${d.round ?? 1})`,
        r.entry.timestamp,
      );
    } else if (d.kind === "step-completed") {
      const secs = Math.round((d.elapsedMs ?? 0) / 1000);
      await emit(
        "driver",
        `--- ${step}${d.label} done in ${secs}s`,
        r.entry.timestamp,
      );
    } else if (d.kind === "dispatched" && d.role !== "work-driver") {
      await emit(
        "driver",
        `agent started: ${d.label ?? d.role}`,
        r.entry.timestamp,
      );
    }
  }
}

function describeCall(name: string, a: Json): string {
  if (name === "bash") return `$ ${a?.command ?? ""}`;
  const target = a?.path ?? a?.file_path ?? a?.pattern ?? a?.query ?? "";
  return `${name} ${typeof target === "string" ? target : ""}`.trim();
}

async function transcriptLine(role: string, line: string) {
  let e: Json;
  try {
    e = JSON.parse(line);
  } catch {
    return;
  }
  if (e.type !== "message" || !e.message) return;
  const m = e.message;
  if (m.role === "assistant") {
    for (const c of m.content ?? []) {
      if (c.type === "text" && c.text?.trim()) {
        await emit(role, clip(c.text.trim(), 1200, 16), e.timestamp);
      } else if (c.type === "toolCall") {
        await emit(
          role,
          clip(describeCall(c.name, c.arguments), 600, 6),
          e.timestamp,
        );
      }
    }
    if (m.errorMessage) {
      await emit(role, `! ${m.errorMessage}`, e.timestamp);
    }
  } else if (m.role === "toolResult") {
    const text = (m.content ?? []).filter((c: Json) => c.type === "text")
      .map((c: Json) => c.text).join("\n").trim();
    if (text) {
      await emit(
        m.isError ? "  ! error" : "  result",
        clip(text, 700, 10),
        e.timestamp,
      );
    }
  }
}

async function transcripts(): Promise<string[]> {
  const found: string[] = [];
  try {
    for await (const day of Deno.readDir(runsDir)) {
      if (!day.isDirectory) continue;
      const dir = `${runsDir}/${day.name}`;
      for await (const f of Deno.readDir(dir)) {
        if (!f.isFile || !f.name.endsWith(".json")) continue;
        const path = `${dir}/${f.name}`;
        if (offsets.has(path)) {
          found.push(path);
          continue;
        }
        const st = await Deno.stat(path);
        if ((st.mtime?.getTime() ?? 0) >= since) found.push(path);
      }
    }
  } catch {
    // The runs directory may not exist until the first agent starts.
  }
  return found.sort();
}

let stop = false;
Deno.addSignalListener("SIGTERM", () => stop = true);
Deno.addSignalListener("SIGINT", () => stop = true);

await emit("tehas", "console attached, waiting for the cycle");
for (;;) {
  for (const line of await newLines(rpcPath)) await rpcLine(line);
  for (const path of await transcripts()) {
    // <job id in two dash-separated parts>-<role>.json
    const file = path.slice(path.lastIndexOf("/") + 1).replace(/\.json$/, "");
    const role = file.split("-").slice(2).join("-") || file;
    for (const line of await newLines(path)) await transcriptLine(role, line);
  }
  if (stop) break;
  await new Promise((r) => setTimeout(r, 1500));
}
await emit("tehas", "cycle ended, console detached");
out.close();
