// The wire demo's store: what its two streams brought. The streams hand each
// event to these actions.
import { pageStore } from "../fun/kit/state.js";

export const useWire = pageStore("demo-wire", {
  state: () => ({ events: [], lines: [] }),
  actions: (set) => ({
    event(e) {
      set((d) => {
        d.events.push({ type: e.type, id: e.id, ...e.data });
      });
    },
    line(e) {
      set((d) => {
        d.lines.push(e.data.n);
      });
    },
  }),
});
