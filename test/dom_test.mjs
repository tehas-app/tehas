// Tests el() of host/fun/dom.js: the one node factory the pages under
// host/fun share. A stub document records what it is asked for and what each
// node is given, and nothing else: the page's own markup is the business of
// the test of the page that draws it.
//
//   node --test test/dom_test.mjs
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const SOURCE = readFileSync(new URL("../host/fun/dom.js", import.meta.url), "utf8");

function documentOf() {
  const made = [];
  const document = {
    createElement: (tag) => {
      const node = { tag, className: "", textContent: "" };
      made.push(node);
      return node;
    },
  };
  return { document, made };
}

// The file as the browser runs it: a classic script, no module system in front
// of it, and the page's document where the script runs. The function's own
// scope stands in for the script's, so `document` is the stub's and nothing
// leaks from one case to the next.
function load(document) {
  new Function("document", "globalThis", SOURCE)(document, globalThis);
  return globalThis.TEHAS_DOM;
}

test("the file is a classic script with the one el of the pages", () => {
  // No import, no export: a page loads it with a plain tag, and a module that
  // wants el() imports the file instead.
  assert.equal((SOURCE.match(/^\s*(import|export)\b/gm) ?? []).length, 0, "dom.js is not a classic script");
  assert.equal((SOURCE.match(/function el\(/g) ?? []).length, 1);
});

test("loading it gives the page an el that makes a node of its own", () => {
  const { document, made } = documentOf();
  const { el } = load(document);
  assert.equal(typeof el, "function");

  const both = el("li", "empty", "Nothing is waiting.");
  assert.deepEqual(both, { tag: "li", className: "empty", textContent: "Nothing is waiting." });
  // A class of null is no class, text of undefined is no text, and a tag
  // stands alone: the forms every page asks for.
  assert.deepEqual(el("a", null, "All 12 in line"), { tag: "a", className: "", textContent: "All 12 in line" });
  assert.deepEqual(el("li"), { tag: "li", className: "", textContent: "" });
  assert.deepEqual(made.map((n) => n.tag), ["li", "a", "li"]);
});

test("a title that is markup reaches the node as its text", () => {
  const { document } = documentOf();
  const { el } = load(document);
  const node = el("span", "when", "<img src=x onerror=alert(1)>");
  assert.equal(node.textContent, "<img src=x onerror=alert(1)>");
  assert.equal("innerHTML" in node, false, "the node has an innerHTML");
});

test("the page's el is the one the module of the same name gives back", () => {
  const { document } = documentOf();
  const dom = load(document);
  assert.deepEqual(Object.keys(dom), ["el"]);
  assert.equal(dom.el("b", null, "#7").textContent, "#7");
});
