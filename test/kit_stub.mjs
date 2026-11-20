// What the kit's Solid needs of a document to load in node: @solidjs/html
// makes a tree walker and a <template> when its module is evaluated. Imported
// before the kit by the kit's node tests; nothing here draws.
const listeners = new Map();

globalThis.document = {
  hidden: false,
  createTreeWalker: () => ({}),
  createElement: () => ({ content: {} }),
  addEventListener: (type, fn) => listeners.set(fn, type),
  removeEventListener: (_type, fn) => listeners.delete(fn),
};

// Sets document.hidden and tells the listeners, as a browser does.
export function setHidden(hidden) {
  globalThis.document.hidden = hidden;
  for (const [fn, type] of [...listeners]) if (type === "visibilitychange") fn();
}

export const listening = () => listeners.size;
