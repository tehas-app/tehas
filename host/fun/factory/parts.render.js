// Pieces drawn in Solid that the dashboard's islands share. A render module,
// so it may draw; pages.max counts it as an island with no element.
import { For, Show, html } from "../kit/vendor/solid.js";

// A space between two elements, which a template drops.
export const SP = " ";

// A host's notes on a ticket, as chips after a space; nothing when there are none.
export function Notes(props) {
  return html`<${Show} when=${() => props.list.length > 0}>${SP}<span class="host-notes"><${For} each=${() => props.list} keyed=${false}>${(c) =>
    html`<${Show} when=${() => c().href} fallback=${html`<span class="host-note">${() => c().text}</span>`}><a class="host-note" href=${() => c().href} target=${() => (c().away ? "_blank" : null)} rel=${() => (c().away ? "noopener noreferrer" : null)}>${() => c().text}</a><//>`}<//></span><//>`;
}
