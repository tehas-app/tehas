// Tests el() of host/fun/dom.js: the one way a page of tehas makes an element,
// which every view that draws markup shares. The file is a classic script, so
// importing it is what a <script> tag does on a page: it puts the function on
// globalThis.TEHAS_DOM and asks the document for nothing until it is called.
//
//   node --test test/dom_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/dom.js";

const { el } = globalThis.TEHAS_DOM;

test("dom.js is a classic script that names one helper", () => {
  assert.equal(typeof el, "function");
  assert.deepEqual(Object.keys(globalThis.TEHAS_DOM), ["el"]);
  // No element was asked for by importing the file: a page without a document
  // (node, and the tests that import a view) can load it.
  assert.equal(typeof globalThis.document, "undefined");
});

test("el makes an element of the tag, with the class and the text it is given", () => {
  const made = [];
  globalThis.document = {
    createElement: (tag) => {
      const node = { tag, className: "", textContent: undefined };
      made.push(node);
      return node;
    },
  };
  try {
    const li = el("li", "empty", "Nothing in line.");
    assert.equal(made.length, 1);
    assert.deepEqual(li, { tag: "li", className: "empty", textContent: "Nothing in line." });
    // No class and no text are left as the element has them, so a caller that
    // fills the node itself is not fought over its content.
    const bare = el("span");
    assert.deepEqual(bare, { tag: "span", className: "", textContent: undefined });
    // An empty class is no class; a null text is text, and an empty string.
    assert.deepEqual(el("p", "", null), { tag: "p", className: "", textContent: null });
    assert.deepEqual(el("b", null, ""), { tag: "b", className: "", textContent: "" });
    assert.deepEqual(made.map((n) => n.tag), ["li", "span", "p", "b"]);
  } finally {
    delete globalThis.document;
  }
});
