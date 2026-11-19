import { For, html } from "../fun/kit/vendor/solid.js";
import { feed } from "../fun/kit/feed.js";
import { Empty, Guard, TimeAgo, view } from "../fun/kit/parts.js";
import { useBoard } from "./board.state.js";
import { queueModel } from "./kit-demo-queue.model.js";

export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 600000 });
  const board = useBoard();
  const model = () => queueModel(live.data(), props);
  const pick = (number) => {
    board.actions.pick(number);
    ctx.emit("pick", { number });
  };
  return html`<${Guard} retry=${(_e) => live.refresh()} fallback=${html`<p class="loading">Loading the queue</p>`}>
    <ul class="rows">
      <${For} each=${() => model().rows} fallback=${html`<${Empty} text="Nothing in line" />`}>${(row) =>
        html`<li class=${() => (board.state.picked === row.number ? "is-picked" : "")}>
          <button type="button" data-number=${() => row.number} onClick=${() => pick(row.number)}>
            #${() => row.number} ${() => row.title}
          </button>
          <${TimeAgo} at=${() => row.updatedAt} />
        </li>`}<//>
    </ul>
  <//>`;
});
