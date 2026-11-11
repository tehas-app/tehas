// The one way a page of tehas makes an element. Every view that draws markup
// takes el() from here, so a class and a piece of text become a node the same
// way on every page.
//
// A classic script, like the pages' other shared files: it holds no import and
// exports nothing, and a page that uses it loads it with a <script> tag before
// its own script (host/fun/index.html, host/fun/business.html). A module that
// wants it imports the file, which is the same script run once for the page.
//
// Text is always set through textContent, never as markup: the words a page
// draws come from files the factory writes and are not trusted.
(() => {
  function el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  globalThis.TEHAS_DOM = { el };
})();
