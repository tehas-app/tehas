// The plain dashboard in a real browser: renders host/docs/_layout.html around
// host/docs/dashboard.frag as the docs server would for /dashboard, serves it
// with host/fun and the samples of test/fixtures/live under a strict CSP,
// loads it in headless Chromium and checks what the islands drew.
//
//   node test/browser/dashboard-check.mjs [--shot <png>] [--root <dir>] [--no-check] [--staged]
//
// --shot saves a full-page screenshot, --root serves another tree's host/ and
// test/fixtures/ (an older checkout, to compare), and --no-check only loads
// the page. --staged first stages host/ with `console update assets`, as
// install.sh does, and serves the stage: the hashed names a home gets. It
// builds the console with cargo unless TEHAS_CONSOLE names one. The layout's template is stood in for: every condition is false,
// so the else branches are drawn, and the page is the fragment's.
//
// A second load checks a host whose dispatcher has not written status.json
// yet: its first read fails and running.json is never there.
//
// Not part of the gate, which has no browser: it prints "skip" and exits 0
// when no Chromium is found. TEHAS_CHROMIUM names one; otherwise it looks for
// Playwright's headless shell under ~/.cache/ms-playwright.
//
// Exit: 0 passed or skipped, 1 a check failed, 2 a usage error.
import { createServer } from "node:http";
import { execFile, execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { extname, join, normalize, resolve } from "node:path";

const args = process.argv.slice(2);
const option = (name) => {
  const at = args.indexOf(name);
  return at < 0 ? null : args[at + 1] ?? null;
};
if (args.some((a, i) => a.startsWith("--") && !["--shot", "--root", "--no-check", "--staged"].includes(a) && args[i - 1] !== "--shot" && args[i - 1] !== "--root")) {
  console.error("usage: dashboard-check.mjs [--shot <png>] [--root <dir>] [--no-check] [--staged]");
  process.exit(2);
}
const ROOT = option("--root") ? resolve(option("--root")) + "/" : new URL("../..", import.meta.url).pathname;
const SHOT = option("--shot") && resolve(option("--shot"));
const CHECK = !args.includes("--no-check");
const HERE = new URL(".", import.meta.url).pathname;
// The tree the page and /fun/ come from: host/ itself, or a stage of it.
const STAGE = args.includes("--staged") ? stage() : null;
const HOST = STAGE ?? join(ROOT, "host/");
const MOUNTS = { "/fun/": join(HOST, "fun/"), "/live/": join(ROOT, "test/fixtures/live/"), "/check/": HERE };
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".png": "image/png", ".txt": "text/plain" };
const CSP = "default-src 'self'; script-src 'self'; style-src 'self'; base-uri 'none'";
// The samples' time, 2026-11-17T12:00:00Z: the bridge's answer is dated by it.
const T = Date.parse("2026-11-17T12:00:00Z");

function stage() {
  const dir = mkdtempSync(join(tmpdir(), "dashboard-stage-")) + "/";
  process.on("exit", () => rmSync(dir, { recursive: true, force: true }));
  const host = join(ROOT, "host");
  if (process.env.TEHAS_CONSOLE) execFileSync(process.env.TEHAS_CONSOLE, ["update", "assets", host, dir], { stdio: "inherit" });
  else {
    const manifest = join(ROOT, "console/Cargo.toml");
    execFileSync("cargo", ["run", "--quiet", "--locked", "--manifest-path", manifest, "-p", "console", "--", "update", "assets", host, dir], { stdio: "inherit" });
  }
  console.log(`note  serving the stage of ${host}`);
  return dir;
}

function chromium() {
  if (process.env.TEHAS_CHROMIUM) return { bin: process.env.TEHAS_CHROMIUM, env: {} };
  const cache = join(homedir(), ".cache/ms-playwright");
  if (!existsSync(cache)) return null;
  const shells = readdirSync(cache).filter((d) => d.startsWith("chromium_headless_shell-")).sort();
  const bin = shells.length && join(cache, shells.at(-1), "chrome-linux/headless_shell");
  if (!bin || !existsSync(bin)) return null;
  const libs = join(cache, "extra-libs/root/usr/lib/x86_64-linux-gnu");
  return { bin, env: existsSync(libs) ? { LD_LIBRARY_PATH: libs } : {} };
}

// The layout with its template actions taken out: every if, with and range
// is false, so only else branches are drawn; $body is the fragment and
// $design the default theme. Enough for this page, not a template engine.
function page(probe) {
  const layout = readFileSync(join(HOST, "docs/_layout.html"), "utf8");
  let body = readFileSync(join(HOST, "docs/dashboard.frag"), "utf8");
  if (probe) body += `\n<script type="module" src="/check/${probe}"></script>\n`;
  const values = { "$body": body, "$design": "beesknees26", "$title": "Dashboard", "$description": "" };
  const out = [];
  const stack = [];
  const on = () => stack.every((s) => s);
  const parts = layout.split(/(\{\{-?[\s\S]*?-?\}\})/);
  for (let i = 0; i < parts.length; i++) {
    let part = parts[i];
    if (!part.startsWith("{{")) {
      if (parts[i - 1]?.endsWith("-}}")) part = part.trimStart();
      if (parts[i + 1]?.startsWith("{{-")) part = part.trimEnd();
      if (on()) out.push(part);
      continue;
    }
    const action = part.replace(/^\{\{-?\s*/, "").replace(/\s*-?\}\}$/, "");
    const word = action.split(/\s+/)[0];
    if (["if", "with", "range"].includes(word)) stack.push(false);
    else if (word === "else") stack.push(!stack.pop());
    else if (word === "end") stack.pop();
    else if (on() && action in values) out.push(values[action]);
  }
  return out.join("").replace(/^\s+/, "");
}

// What the docs server answers, the samples for /live/ and a console that
// grows on each read; /host/bridge.json is a host's answer with one note.
// failing counts the 404s a /live/ address still answers before its file.
let failing = {};
// The /fun/ addresses the server had no file for, and the hashed ones it served.
const missing = [];
const hashed = new Set();
function serve(req, res) {
  const url = new URL(req.url, "http://x");
  const path = decodeURIComponent(url.pathname);
  const head = { "content-security-policy": CSP, "cache-control": "no-store" };
  if (path === "/" || path === "/dashboard") {
    res.writeHead(200, { ...head, "content-type": "text/html" }).end(page(url.searchParams.get("probe")));
    return;
  }
  if (failing[path] > 0) {
    failing[path] -= 1;
    res.writeHead(404, head).end("not found");
    return;
  }
  if (path === "/host/bridge.json") {
    const answer = { bridge: 1, time: new Date().toISOString(), ticketNotes: [{ issue: 42, text: "2 votes" }] };
    res.writeHead(200, { ...head, "content-type": TYPES[".json"] }).end(JSON.stringify(answer));
    return;
  }
  const console = /^\/live\/console-(\d+)\.txt$/.exec(path);
  if (console) {
    const text = Array.from({ length: 40 }, (_, i) => `#${console[1]} line ${i + 1} of the console\n`).join("");
    const range = /^bytes=(\d+)-$/.exec(req.headers.range ?? "");
    const from = range ? Number(range[1]) : 0;
    const bytes = Buffer.from(text).subarray(from);
    res.writeHead(range ? 206 : 200, { ...head, "content-type": "text/plain" }).end(bytes);
    return;
  }
  const mount = Object.keys(MOUNTS).find((m) => path.startsWith(m));
  const file = mount && normalize(join(MOUNTS[mount], path.slice(mount.length)));
  if (!file || !file.startsWith(MOUNTS[mount]) || !existsSync(file) || !statSync(file).isFile()) {
    if (path.startsWith("/fun/")) missing.push(path);
    res.writeHead(404, head).end("not found");
    return;
  }
  if (/\.[0-9a-f]{12}\.(js|css)$/.test(path)) hashed.add(path);
  const headers = { ...head, "content-type": TYPES[extname(file)] ?? "application/octet-stream" };
  if (mount === "/live/") headers.date = new Date(T).toUTCString();
  res.writeHead(200, headers).end(readFileSync(file));
}

const browser = chromium();
if (!browser) {
  console.log("skip  the dashboard in a browser (no Chromium; set TEHAS_CHROMIUM)");
  process.exit(0);
}

const server = createServer(serve).listen(0, "127.0.0.1");
await new Promise((r) => server.once("listening", r));
const url = `http://127.0.0.1:${server.address().port}/dashboard`;
const run = (extra, probe) =>
  new Promise((done, fail) =>
    execFile(
      browser.bin,
      ["--headless", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--virtual-time-budget=20000", ...extra, probe ? `${url}?probe=${probe}` : url],
      { env: { ...process.env, ...browser.env }, timeout: 90000, maxBuffer: 1 << 25 },
      (err, out, errOut) => (err ? fail(new Error(`${err.message}\n${errOut}`)) : done(out)),
    ),
  );

try {
  if (SHOT) {
    await run(["--window-size=1280,4200", `--screenshot=${SHOT}`]);
    console.log(`shot  ${SHOT}`);
  }
  if (!CHECK) process.exit(0);
  const probe = async (file) => {
    const dom = await run(["--window-size=1280,2000", "--dump-dom"], file);
    const raw = /<pre id="probe-result"[^>]*>([^<]*)<\/pre>/.exec(dom)?.[1];
    if (!raw) return null;
    const found = JSON.parse(raw.replaceAll("&quot;", '"').replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&amp;", "&"));
    if (process.env.DASHBOARD_CHECK_DUMP) console.log(JSON.stringify(found, null, 1));
    return found;
  };
  const found = await probe("dashboard-probe.js");
  failing = { "/live/status.json": 1, "/live/running.json": Infinity };
  const late = await probe("dashboard-recover-probe.js");
  if (!found || !late) {
    console.log("FAIL  the dashboard in a browser: the page wrote no result");
    if (missing.length) console.log(`FAIL  no file for ${[...new Set(missing)].join(" ")}`);
    process.exit(1);
  }
  const s = found.steps;
  const r = late.steps;
  const checks = [
    ["no error in the check", !found.error, found.error],
    ["no CSP violation", found.violations.length === 0, found.violations],
    ["every /fun/ address has a file", missing.length === 0, missing],
    ...(STAGE ? [["the islands, the kit and the sheets by hashed name", [...hashed].filter((p) => /^\/fun\/(factory|kit)\//.test(p)).length >= 9, [...hashed]]] : []),
    ["every island mounted", s.unmounted?.length === 0, s.unmounted],
    ["the server HTML stood first", s.firstPill === "loading", s.firstPill],
    ["the state says the factory is working", s.pill === "working" && s.pillClass === "pill run", [s.pill, s.pillClass]],
    ["the queue was read", /^queue read /.test(s.fresh ?? ""), s.fresh],
    ["the link to all issues", s.all === "https://github.com/acme/widget/issues", s.all],
    ["the four lists", JSON.stringify(s.counts) === JSON.stringify([2, 1, 1, 2]), s.counts],
    ["their counts", JSON.stringify(s.ns) === JSON.stringify(["(2)", "(1)", "(1)", "(2)"]), s.ns],
    ["a row binds the sample's text", /^#42A search with no match says so/.test(s.firstRow ?? ""), s.firstRow],
    ["the host's note on a ticket", s.note === "2 votes", s.note],
    ["no read-only section", s.roHidden === true && s.roPill === true, [s.roHidden, s.roPill]],
    ["two console tabs", JSON.stringify(s.tabs) === JSON.stringify(["#41", "#36 ended"]), s.tabs],
    ["the running cycle is shown", /^Working on #41  Export the report as CSV$/.test(s.title ?? ""), s.title],
    ["its console is tailed", /#41 line 40 of the console/.test(s.console ?? "") && s.consoleNote === "live", [s.console?.slice(-40), s.consoleNote]],
    ["a tab click shows the last cycle", /^Last cycle: #36  The login form/.test(s.clicked ?? "") && /^ended as pr at step handoff/.test(s.clickedStep ?? ""), [s.clicked, s.clickedStep]],
    ["and its console", /^#36 line 1 /.test(s.clickedConsole ?? ""), s.clickedConsole?.slice(0, 40)],
    ["an arrow key moves to the next tab", s.arrowed === "#41" && s.arrowFocus === "#41", [s.arrowed, s.arrowFocus]],
    ["three previews", s.previews === 3 && s.previewsHidden === false, [s.previews, s.previewsHidden]],
    ["a preview that does not serve says why", s.notServing === "unable to create: the build failed", s.notServing],
    ["the host section", s.hostRows === 3 && s.units === 2 && s.hostHidden === false, [s.hostRows, s.units, s.hostHidden]],
    // The server's Date is the sample's time, days behind the viewer's clock.
    ["the snapshot's age is by the server's clock", /, snapshot written just now\.$/.test(s.hostNote ?? ""), s.hostNote],
    ["the ten days", s.analyticsHidden === false && s.figures >= 8 && s.days === 10 && s.line === true, [s.analyticsHidden, s.figures, s.days, s.line]],
    ["bar heights set as custom properties", s.barHeight === true, s.barHeight],
    ["the versions table", s.versions === 2, s.versions],
    ["the seven islands that read the queue share one poller", s.fetches?.["/live/status.json"] <= 2, s.fetches],
    ["the host file is asked for again within ten seconds", s.fetches?.["/live/top.json"] >= 2, s.fetches],
    ["with no status yet: no error, no CSP violation", !late.error && late.errors.length === 0 && late.violations.length === 0, [late.error, late.errors, late.violations]],
    ["the lists keep their headings while there is no status", r.headingsAfterFailure === 4, r.headingsAfterFailure],
    ["the page says there is no status yet", r.freshAfterFailure === "no status yet: the dispatcher has not written one", r.freshAfterFailure],
    ["the queue is asked for again and drawn when it comes", r.fetches?.["/live/status.json"] >= 2 && r.queued === 2 && r.headings === 4, [r.fetches?.["/live/status.json"], r.queued, r.headings]],
    ["with no running.json the state is drawn from the queue", r.pill === "idle" && /^queue read /.test(r.fresh ?? ""), [r.pill, r.fresh]],
    ["the ten days do not wait for the queue", r.analyticsHidden === false, r.analyticsHidden],
  ];
  let failed = 0;
  for (const [name, ok, got] of checks) {
    if (ok) console.log(`ok    ${name}`);
    else {
      failed += 1;
      console.log(`FAIL  ${name}: ${JSON.stringify(got)}`);
    }
  }
  console.log(`${failed ? "FAIL" : "ok  "}  the dashboard in a browser: ${checks.length - failed} of ${checks.length}`);
  process.exitCode = failed ? 1 : 0;
} finally {
  server.close();
}
