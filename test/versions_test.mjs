// Tests line() of the footers' versions: the pure step from live/health.json
// to the line the two front pages show.
//
//   node --test test/versions_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/versions.js";

const { line } = globalThis.TEHAS_VERSIONS;
const a = "a".repeat(40);
const health = (checks) => ({ status: "pass", checks });

test("a release host run by an industry names all three", () => {
  const h = health({
    platform: { release: "v0.24.6", newest: "v0.24.6", behind: 0 },
    rukas: { image: a, imageVersion: "v0.14.54", target: a, behind: false },
    industry: { version: "v0.9.0" },
  });
  assert.equal(line(h), "tehas v0.24.6 · pi-rukas v0.14.54 · industry v0.9.0");
});

test("a standalone factory leaves the industry out", () => {
  const h = health({ platform: { release: "v0.24.6" }, rukas: { imageVersion: "v0.14.54" } });
  assert.equal(line(h), "tehas v0.24.6 · pi-rukas v0.14.54");
});

test("a checkout host and an image of no known version are named by commit", () => {
  const h = health({ platform: { commit: "9b1b7dc11b73b93f7296b5f3131fcc6ef6b964e9", behind: 0 }, rukas: { image: a, imageVersion: null } });
  assert.equal(line(h), "tehas 9b1b7dc · pi-rukas aaaaaaa");
});

test("the pi-rukas named is the image's, not the target's or the clone's", () => {
  const h = health({ rukas: { imageVersion: "v0.14.49", targetVersion: "v0.14.53", clone: a, behind: true } });
  assert.equal(line(h), "pi-rukas v0.14.49");
});

test("values of another shape are left out, and no version at all is null", () => {
  const h = health({
    platform: { release: "<b>v1</b>" },
    rukas: { imageVersion: "v0.14.54 x", image: "javascript:1" },
    industry: { version: 9 },
  });
  assert.equal(line(h), null);
  for (const f of [null, undefined, 3, [], {}, { checks: null }]) assert.equal(line(f), null);
});
