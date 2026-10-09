// Tests bin/tehas-energy, the sampler behind the Energy use section,
// against a local server that plays the mesh.
//
//   deno test -A --no-config test/energy_test.ts
import { assert, assertEquals } from "jsr:@std/assert@^1.0.19";

const root = new URL("..", import.meta.url).pathname;
const fixture = await Deno.readTextFile(`${root}test/fixtures/cluster.json`);
const T = 1791526545; // a multiple of 15: the start of a slot

// A mesh that answers every request with the given status and body.
function mesh(status: number, body: string) {
  let calls = 0;
  const server = Deno.serve({ port: 0, hostname: "127.0.0.1", onListen() {} }, () => {
    calls++;
    return new Response(body, { status, headers: { "content-type": "application/json" } });
  });
  return {
    url: `http://127.0.0.1:${server.addr.port}/v1/cluster`,
    calls: () => calls,
    close: () => server.shutdown(),
  };
}

async function home() {
  const dir = await Deno.makeTempDir({ prefix: "tehas-energy-" });
  await Deno.mkdir(`${dir}/live`);
  return { dir, file: `${dir}/live/energy.json` };
}

async function sample(dir: string, env: Record<string, string>, args: string[] = []) {
  const out = await new Deno.Command(`${root}bin/tehas-energy`, {
    args,
    env: { PATH: Deno.env.get("PATH") ?? "", HOME: dir, TEHAS_HOME: dir, ...env },
    clearEnv: true,
    stdout: "piped",
    stderr: "piped",
  }).output();
  return { code: out.code, out: new TextDecoder().decode(out.stdout) };
}

const read = async (file: string) => JSON.parse(await Deno.readTextFile(file));
const exists = (file: string) => Deno.stat(file).then(() => true, () => false);

Deno.test("a cluster snapshot becomes the energy file", async () => {
  const m = mesh(200, fixture), h = await home();
  try {
    const r = await sample(h.dir, {
      TEHAS_ENERGY_URL: m.url,
      TEHAS_ENERGY_NOW: String(T),
      PI_ENSEMBLE_SUBAGENT_MODEL: "model-large",
    });
    assertEquals(r, { code: 0, out: "" });
    const e = await read(h.file);
    assertEquals(e.generatedAt, "2026-10-09T06:15:45Z");
    assertEquals(e.source, "viiwork");
    assertEquals(e.periodSeconds, 15);
    assertEquals(e.factoryModels, ["model-large"]);
    // Only the alive members that serve a factory model: node-a serves another
    // model, node-c serves none and node-d has left. The totals are theirs alone.
    assertEquals(e.totals, { watts: 734, kWh24h: 1.44, kWh30d: 55.08, hosts: 1, reporting: 1, energyHosts: 1 });
    assertEquals(e.hosts, [
      { name: "node-b", source: "nvidia-smi", watts: 734, kWh24h: 1.44, kWh30d: 55.08, ramUsedMb: 52624, ramTotalMb: 253212 },
    ]);
    // Only the factory's models, though node-b serves model-small as well.
    assertEquals(e.models, [
      { name: "model-large", hosts: ["node-b"], slots: 1, busy: 1, queued: 2 },
    ]);
    assertEquals(e.samples, [{ t: T, w: { "node-b": 734 } }]);
  } finally {
    await m.close();
  }
});

Deno.test("a sample carries each machine's token count for the factory's models", async () => {
  const cluster = JSON.parse(fixture);
  // deno-lint-ignore no-explicit-any
  const models = (node: string) => cluster.members.find((m: any) => m.node === node).status.models;
  // node-b counts tokens for both of its models; only model-large is the factory's.
  models("node-b").forEach((m: { name: string; tokens_total?: unknown }) => m.tokens_total = m.name === "model-large" ? 620263.6 : 99);
  // node-a serves another model, and a count that is no number is no count.
  models("node-a").forEach((m: { tokens_total?: unknown }) => m.tokens_total = "many");
  const m = mesh(200, JSON.stringify(cluster)), h = await home();
  try {
    await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T), PI_ENSEMBLE_SUBAGENT_MODEL: "model-large" });
    assertEquals((await read(h.file)).samples, [{ t: T, w: { "node-b": 734 }, k: { "node-b": 620263 } }]);
    await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T + 15), PI_ENSEMBLE_SUBAGENT_MODEL: "model-large" });
    assertEquals((await read(h.file)).samples.map((s: { k: unknown }) => s.k), [{ "node-b": 620263 }, { "node-b": 620263 }]);
  } finally {
    await m.close();
  }
});

Deno.test("the file holds no address, node id or GPU", async () => {
  const m = mesh(200, fixture), h = await home();
  try {
    await sample(h.dir, { TEHAS_ENERGY_URL: m.url });
    const text = await Deno.readTextFile(h.file);
    for (const secret of ["192.0.2.", "viiwork-0000", "vram", "v2.9.0", "llamacpp"]) {
      assert(!text.includes(secret), `the file contains ${secret}`);
    }
  } finally {
    await m.close();
  }
});

Deno.test("one sample per slot, and none older than 30 minutes", async () => {
  const m = mesh(200, fixture), h = await home();
  const at = (t: number) => sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(t) });
  try {
    await at(T);
    await at(T + 14); // the same slot: replaces the first
    assertEquals((await read(h.file)).samples.map((s: { t: number }) => s.t), [T + 14]);
    await at(T + 15);
    await at(T + 45); // a slot was missed: nothing is made up for it
    assertEquals((await read(h.file)).samples.map((s: { t: number }) => s.t), [T + 14, T + 15, T + 45]);
    await at(T + 1815);
    assertEquals((await read(h.file)).samples.map((s: { t: number }) => s.t), [T + 45, T + 1815]);
  } finally {
    await m.close();
  }
});

Deno.test("a sample from the future is dropped when the clock has gone back", async () => {
  const m = mesh(200, fixture), h = await home();
  try {
    await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T + 3600) });
    await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T) });
    assertEquals((await read(h.file)).samples.map((s: { t: number }) => s.t), [T]);
  } finally {
    await m.close();
  }
});

Deno.test("a damaged file is replaced, not fatal", async () => {
  const m = mesh(200, fixture), h = await home();
  try {
    await Deno.writeTextFile(h.file, '{"samples": [{"t": "soon"}, 7, {"t": 5}');
    assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T) })).code, 0);
    assertEquals((await read(h.file)).samples.length, 1);
    await Deno.writeTextFile(h.file, JSON.stringify({ samples: [{ t: "soon" }, 7, { t: T - 15 }, { t: T - 30, w: {} }] }));
    await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T) });
    assertEquals((await read(h.file)).samples.map((s: { t: number }) => s.t), [T - 30, T]);
    // Files that parse but are not an energy file, or hold no single value.
    for (const text of ['{"samples":5}', "  \n", '{"samples":[]} {"samples":[]}']) {
      await Deno.writeTextFile(h.file, text);
      assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T) })).code, 0, text);
      assertEquals((await read(h.file)).samples.length, 1, text);
    }
  } finally {
    await m.close();
  }
});

Deno.test("a mesh that does not answer leaves the file as it is", async () => {
  const m = mesh(200, fixture), h = await home();
  await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T) });
  await m.close();
  const before = await Deno.readTextFile(h.file);
  // The port is closed now.
  assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T + 15) })).code, 0);
  assertEquals(await Deno.readTextFile(h.file), before);
  for (const status of [502, 408, 429]) {
    const down = mesh(status, "try later");
    try {
      await sample(h.dir, { TEHAS_ENERGY_URL: down.url, TEHAS_ENERGY_NOW: String(T + 30) });
      assertEquals(await Deno.readTextFile(h.file), before, `${status}`);
    } finally {
      await down.close();
    }
  }
});

Deno.test("an answer that is not a cluster snapshot removes the file", async () => {
  const cases: [number, string][] = [
    [404, "404 page not found"],
    [200, '{"object":"list","data":[]}'],
    [200, "[]"],
    [200, "<html>"],
  ];
  for (const [status, body] of cases) {
    const good = mesh(200, fixture), other = mesh(status, body), h = await home();
    try {
      await sample(h.dir, { TEHAS_ENERGY_URL: good.url });
      assert(await exists(h.file));
      assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: other.url })).code, 0);
      assert(!(await exists(h.file)), `${status} ${body} left the file in place`);
    } finally {
      await good.close();
      await other.close();
    }
  }
});

Deno.test("TEHAS_ENERGY=off removes the file and does not ask the mesh", async () => {
  const m = mesh(200, fixture), h = await home();
  try {
    await sample(h.dir, { TEHAS_ENERGY_URL: m.url });
    assertEquals(m.calls(), 1);
    assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY: "off" })).code, 0);
    assert(!(await exists(h.file)));
    assertEquals(m.calls(), 1);
  } finally {
    await m.close();
  }
});

Deno.test("the mesh address comes from the model provider", async () => {
  const m = mesh(200, fixture), h = await home();
  try {
    const models = `${h.dir}/models.json`;
    const base = m.url.replace("/v1/cluster", "");
    await Deno.writeTextFile(models, JSON.stringify({ providers: { fleet: { baseUrl: `${base}/v1/` }, other: { baseUrl: "http://127.0.0.1:9/v1" } } }));
    await sample(h.dir, { TEHAS_MODELS_FILE: models });
    assert(await exists(h.file));
    assertEquals(m.calls(), 1);
    // Another provider, and one that is not configured: no file.
    await sample(h.dir, { TEHAS_MODELS_FILE: models, PI_ENSEMBLE_SUBAGENT_PROVIDER: "missing" });
    assert(!(await exists(h.file)));
    assertEquals((await sample(h.dir, { TEHAS_MODELS_FILE: `${h.dir}/none.json` })).code, 0);
    assert(!(await exists(h.file)));
  } finally {
    await m.close();
  }
});

Deno.test("a role's own model is a factory model too", async () => {
  const m = mesh(200, fixture), h = await home();
  try {
    await sample(h.dir, {
      TEHAS_ENERGY_URL: m.url,
      PI_ENSEMBLE_SUBAGENT_MODEL: "model-large",
      PI_ENSEMBLE_MODEL_REVIEWER: "model-small",
      PI_ENSEMBLE_MODEL_IDLE: "",
      PI_ENSEMBLE_MAX_WORKSTREAMS: "1",
    });
    const e = await read(h.file);
    assertEquals(e.factoryModels, ["model-large", "model-small"]);
    // In name order whatever order the mesh lists them in; two hosts of a model are summed.
    assertEquals(e.hosts.map((x: { name: string }) => x.name), ["node-a", "node-b"]);
    assertEquals(e.models.map((x: { name: string; slots: number }) => [x.name, x.slots]), [["model-large", 1], ["model-small", 6]]);
  } finally {
    await m.close();
  }
});

Deno.test("an empty mesh and odd members give a file with nothing in it", async () => {
  const odd = JSON.stringify({ members: [null, 7, { node: 5, state: "alive", status: {} }, { node: "node-x", state: "alive", status: {} }] });
  const m = mesh(200, odd), h = await home();
  try {
    assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T) })).code, 0);
    const e = await read(h.file);
    assertEquals(e.totals, { watts: null, kWh24h: null, kWh30d: null, hosts: 0, reporting: 0, energyHosts: 0 });
    assertEquals(e.hosts, []);
    assertEquals(e.models, []);
    assertEquals(e.samples, [{ t: T, w: {} }]);
  } finally {
    await m.close();
  }
});

Deno.test("objects where text or numbers belong stay out of the file", async () => {
  const body = JSON.stringify({ members: [{ node: "node-x", state: "alive", status: {
    power: { available: true, watts: 100, source: { m: "LEAKMARK" } },
    host_mem_used_mb: { m: "LEAKMARK" },
    host_mem_total_mb: ["LEAKMARK"],
    models: [{ name: "model-large", slots: 1 }],
  } }] });
  const m = mesh(200, body), h = await home();
  try {
    assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T), PI_ENSEMBLE_SUBAGENT_MODEL: "model-large" })).code, 0);
    const text = await Deno.readTextFile(h.file);
    assert(!text.includes("LEAKMARK"));
    const [host] = JSON.parse(text).hosts;
    assertEquals([host.source, host.ramUsedMb, host.ramTotalMb, host.watts], [null, null, null, 100]);
  } finally {
    await m.close();
  }
});

Deno.test("a negative reading is no reading", async () => {
  const body = JSON.stringify({ members: [{ node: "node-x", state: "alive", status: {
    power: { available: true, watts: -5, source: "rapl" },
    models: [{ name: "model-large", slots: 1 }],
  } }] });
  const m = mesh(200, body), h = await home();
  try {
    assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T), PI_ENSEMBLE_SUBAGENT_MODEL: "model-large" })).code, 0);
    const e = await read(h.file);
    assertEquals(e.totals.watts, null);
    assertEquals(e.totals.reporting, 0);
    assertEquals(e.hosts[0].watts, null);
    assertEquals(e.hosts[0].source, null);
    assertEquals(e.samples, [{ t: T, w: {} }]);
  } finally {
    await m.close();
  }
});

Deno.test("odd models and odd slots do not make a member a factory host; a text power field is no reading", async () => {
  const body = JSON.stringify({ members: [
    { node: "node-x", state: "alive", status: { power: "n/a", models: [7, null, "m", { name: "model-large", slots: "many", busy: "x", queued: [] }] } },
    { node: "node-y", state: "alive", status: { power: "n/a", models: [{ name: "model-large", slots: 2, busy: "x", queued: {} }] } },
  ] });
  const m = mesh(200, body), h = await home();
  try {
    assertEquals((await sample(h.dir, { TEHAS_ENERGY_URL: m.url, TEHAS_ENERGY_NOW: String(T), PI_ENSEMBLE_SUBAGENT_MODEL: "model-large" })).code, 0);
    const e = await read(h.file);
    assertEquals(e.hosts.map((x: { name: string; watts: null }) => [x.name, x.watts]), [["node-y", null]]);
    assertEquals(e.models.map((x: { name: string; slots: number; busy: number; queued: number }) => [x.name, x.slots, x.busy, x.queued]), [["model-large", 2, 0, 0]]);
  } finally {
    await m.close();
  }
});

Deno.test("an unknown argument is a usage error", async () => {
  const h = await home();
  assertEquals((await sample(h.dir, {}, ["--loop"])).code, 64);
});
