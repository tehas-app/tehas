// The host bridge's answer (host-bridge.js) as a page store, so the islands
// that draw tickets read it reactively. The bridge calls seen() when its
// answer changes.
import { pageStore, shared } from "../kit/state.js";

export const useBridge = pageStore("host-bridge", {
  state: () => ({ answer: globalThis.TEHAS_HOST_BRIDGE?.current() ?? null }),
  actions: (set) => ({
    seen(answer) {
      set((d) => {
        d.answer = answer;
      });
    },
  }),
});

// The bridge's rules for an answer, for a model: where a ticket links and its
// notes. With no bridge on the page, no link changes and no note is drawn.
export function links(answer) {
  const b = globalThis.TEHAS_HOST_BRIDGE;
  if (!b) return { ticket: () => null, notes: () => [] };
  return { ticket: (repo, url) => b.ticketHref(answer, repo, url), notes: (issue) => b.notesOf(answer, issue) };
}

// Tells the store each new answer, once per page.
export function listen() {
  shared("host-bridge:listen", () => {
    const store = useBridge();
    globalThis.TEHAS_HOST_BRIDGE?.onChange((answer) => store.actions.seen(answer));
    return true;
  });
}
