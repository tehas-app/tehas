// Tests the shape of host/fun/dashboard-view.js, the plain dashboard's module:
// node imports it with no document, it says which element marks its page, and
// host/docs/dashboard.frag holds that element and loads the module after the
// tabs. What the module draws is test/dashboard_host_test.mjs.
//
//   node --test test/dashboard_view_test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import "../host/fun/dashboard-view.js";

const FRAG = readFileSync(new URL("../host/docs/dashboard.frag", import.meta.url), "utf8");
const VIEW = readFileSync(new URL("../host/fun/dashboard-view.js", import.meta.url), "utf8");

test("the module loads with no document and names the element of its page", () => {
  assert.equal(typeof globalThis.document, "undefined");
  assert.deepEqual(globalThis.TEHAS_DASHBOARD, { probe: "state" });
  assert.ok(FRAG.includes(`id="${globalThis.TEHAS_DASHBOARD.probe}"`));
});

test("the module imports its siblings relatively", () => {
  assert.ok(VIEW.includes('import { agoText, dueToRead, layout } from "./host-data.js";'));
  assert.ok(VIEW.includes('import { analyticsView, dueToRead as dueAnalytics } from "./analytics-data.js";'));
  assert.ok(!VIEW.includes('from "/fun/'));
});

test("the fragment loads the tabs, then the module, and holds no script of its own", () => {
  const tags = FRAG.split("\n").filter((l) => l.includes("<script"));
  assert.deepEqual(tags, [
    '<script src="/fun/console-tabs.js"></script>',
    '<script type="module" src="/fun/dashboard-view.js"></script>',
  ]);
});
