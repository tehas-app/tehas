// The theme's tokens for what cannot use var(): the canvases and their fonts.
// A token is read from the page's stylesheet once, then kept; the theme does
// not change while a page is open. See host/fun/theme-pixel.css.
(() => {
  const kept = new Map();
  function token(name) {
    if (!kept.has(name)) {
      kept.set(name, getComputedStyle(document.documentElement).getPropertyValue("--" + name).trim());
    }
    return kept.get(name);
  }
  globalThis.TEHAS_THEME = { token };
})();
