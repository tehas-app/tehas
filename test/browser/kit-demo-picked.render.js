import { html, isPending } from "../fun/kit/vendor/solid.js";
import { feed } from "../fun/kit/feed.js";
import { Busy, Guard, view } from "../fun/kit/parts.js";
import { useBoard } from "./board.state.js";
import { pickedModel } from "./kit-demo-picked.model.js";

export const mount = view((props) => {
  const live = feed(props.src, { every: 600000 });
  const board = useBoard();
  const model = () => pickedModel(live.data(), board.state.picked);
  return html`<${Guard} retry=${(_e) => live.refresh()}>
    <p class="count">${() => model().count} in line</p>
    <p class="picked">${() => model().picked}</p>
    <${Busy} when=${() => isPending(() => live.data())} />
  <//>`;
});
