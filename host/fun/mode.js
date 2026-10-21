// Light or dark by the viewer's hand, on a theme that has a dark set (its
// --modes token names dark). Light unless the viewer picks dark; the system
// setting is not followed. The choice is kept in this browser where storage
// allows. Loaded in the head, after the theme, so a kept choice applies before
// the page is drawn.
(() => {
  const root = document.documentElement;
  const KEY = "tehas-mode";
  const hasDark = () => /\bdark\b/.test(getComputedStyle(root).getPropertyValue("--modes"));
  try {
    if (localStorage.getItem(KEY) === "dark" && hasDark()) root.dataset.mode = "dark";
  } catch { /* no storage: light */ }
  addEventListener("DOMContentLoaded", () => {
    const button = document.getElementById("mode");
    if (!button || !hasDark()) return;
    const label = () => { button.textContent = root.dataset.mode === "dark" ? "Light mode" : "Dark mode"; };
    button.hidden = false;
    label();
    button.addEventListener("click", () => {
      if (root.dataset.mode === "dark") delete root.dataset.mode; else root.dataset.mode = "dark";
      try { localStorage.setItem(KEY, root.dataset.mode || "light"); } catch { /* kept for this page only */ }
      label();
      dispatchEvent(new Event("resize")); // the charts redraw in the new colours
    });
  });
})();
