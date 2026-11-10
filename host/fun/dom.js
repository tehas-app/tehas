// The one el() of the pages under host/fun: a tag, a class and its text, as a
// node of this document. The plain dashboard (dashboard-view.js), the business
// page (business-view.js), the factory floor (game-view.js), the epics
// (epics-view.js) and the panel dock (panels.js) all build their markup the
// same way, and a page that held two copies of it could not tell which drew
// what it shows. Text is untrusted, so the node is filled through textContent
// and never as markup (test/dom_test.mjs).
//
// A classic script, not a module: epics-view.js and panels.js are classic too
// and cannot import. A page with one of those loads this file before them; a
// module needs no tag, because it imports the file and the import runs this
// body before its own.
(() => {
  function el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  globalThis.TEHAS_DOM = { el };
})();
