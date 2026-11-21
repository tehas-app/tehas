// A page's code that uses the kit as host/docs/frontend.md says. The kit's
// type check (console/ops/src/update/types.rs) passes it with the kit.
import { feed } from "../../../host/fun/kit/feed.js";
import { island } from "../../../host/fun/kit/island.js";
import { view } from "../../../host/fun/kit/parts.js";
import { press } from "../../../host/fun/kit/press.js";
import { pageStore } from "../../../host/fun/kit/state.js";
import { stream } from "../../../host/fun/kit/stream.js";
import { html } from "../../../host/fun/kit/vendor/solid.js";

const useBoard = pageStore("board", {
  state: () => ({ picked: /** @type {number | null} */ (null) }),
  actions: (set) => ({
    /** @param {number} n */
    pick(n) {
      set((d) => {
        d.picked = n;
      });
    },
  }),
});

export const mount = view((props, ctx) => {
  const board = useBoard();
  const status = feed("/live/status.json", { every: 10000, accept: { 404: null } });
  status.refresh().then((a) => (a.ok ? a.data : a.error));
  status.put({ ok: true });
  const live = stream("/api/events", { format: "ndjson", broken: "fail", on: (e) => board.actions.pick(e.data) });
  ctx.emit("picked", { n: board.state.picked });
  press("/api/pick", { body: { n: props.n } }).then((a) => a.status);
  return html`<p>${() => live.state()}</p>`;
});

island("kit-demo-uses", { props: { n: Number }, load: () => ({ mount }) });
