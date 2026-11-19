// The demo page's shared store: which issue is picked. Both islands import
// this one definition.
import { pageStore } from "../fun/kit/state.js";

export const useBoard = pageStore("demo-board", {
  state: () => ({ picked: null }),
  actions: (set) => ({
    pick(number) {
      set((d) => {
        d.picked = number;
      });
    },
  }),
});
