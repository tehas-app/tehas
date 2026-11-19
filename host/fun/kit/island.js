// The base of an island element (host/docs/frontend.md, "The island
// contract"). It holds no library: it reads the attributes and the child JSON
// once, waits for the hydrate trigger, loads the island's render module and
// mounts it, and calls its unmount when the element leaves the page. Until the
// mount the server HTML stands; the mount replaces it.
//
//   import { island } from "../kit/island.js";
//   island("tehas-factory-queue", {
//     props: { src: String, limit: Number },
//     load: () => import("./tehas-factory-queue.render.js"),
//   });
//
// The render module exports mount(host, ctx) -> { update(props)?, unmount() }.
// ctx is { props, data, first, emit(verb, detail), signal }: the attributes
// read through `props`, the child <script type="application/json"> or null,
// the server HTML's nodes as they stood, the event sender and an AbortSignal
// that aborts when the element disconnects. update(props) gets the props
// again when an observed attribute changes.

const TRIGGERS = ["load", "visible", "idle", "interaction"];

// Waits for the trigger the element names, then calls go. Aborting `signal`
// stops the wait.
function whenTriggered(el, signal, go) {
  const how = el.getAttribute("hydrate") || "load";
  if (!TRIGGERS.includes(how) || how === "load") {
    queueMicrotask(go);
    return;
  }
  if (how === "visible") {
    const seen = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        seen.disconnect();
        go();
      }
    });
    seen.observe(el);
    signal.addEventListener("abort", () => seen.disconnect());
    return;
  }
  if (how === "idle") {
    // Safari has no requestIdleCallback; a short timeout stands for idle.
    if (typeof requestIdleCallback === "function") {
      const id = requestIdleCallback(go, { timeout: 2000 });
      signal.addEventListener("abort", () => cancelIdleCallback(id));
    } else {
      const id = setTimeout(go, 200);
      signal.addEventListener("abort", () => clearTimeout(id));
    }
    return;
  }
  const once = () => go();
  for (const type of ["pointerdown", "focusin", "keydown"]) {
    el.addEventListener(type, once, { once: true, signal });
  }
}

// The attributes as typed props. `kinds` maps a property name to a converter
// (String, Number, Boolean); the attribute is the name in kebab case.
export function readProps(el, kinds) {
  const props = {};
  for (const [name, kind] of Object.entries(kinds)) {
    const attr = attrName(name);
    if (kind === Boolean) props[name] = el.hasAttribute(attr);
    else if (el.hasAttribute(attr)) {
      const raw = el.getAttribute(attr);
      props[name] = kind === Number ? Number(raw) : kind(raw);
    }
  }
  return props;
}

export function attrName(name) {
  return name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

// A CustomEvent `<element>:<verb>` that bubbles, its detail plain JSON.
export function eventFor(el, verb, detail) {
  const plain = detail === undefined ? null : JSON.parse(JSON.stringify(detail));
  return new CustomEvent(`${el.localName}:${verb}`, { bubbles: true, detail: plain });
}

// Defines the custom element `name` on the island base. `spec.props` names the
// attributes, `spec.load` returns the render module (or a promise of it).
export function island(name, spec) {
  const kinds = spec.props ?? {};
  class Island extends HTMLElement {
    static observedAttributes = Object.keys(kinds).map(attrName);
    #abort = null;
    #handle = null;
    #data = undefined;

    connectedCallback() {
      if (this.#abort) return;
      const abort = new AbortController();
      this.#abort = abort;
      if (this.#data === undefined) {
        const json = this.querySelector(":scope > script[type='application/json']");
        try {
          this.#data = json ? JSON.parse(json.textContent) : null;
        } catch {
          this.#data = null;
        }
      }
      whenTriggered(this, abort.signal, () => this.#mount(abort));
    }

    disconnectedCallback() {
      this.#abort?.abort();
      this.#abort = null;
      this.#handle?.unmount();
      this.#handle = null;
      this.removeAttribute("data-tehas-mounted");
    }

    attributeChangedCallback(_attr, before, after) {
      if (before !== after) this.#handle?.update?.(readProps(this, kinds));
    }

    async #mount(abort) {
      let view;
      try {
        view = await spec.load();
      } catch (error) {
        console.error(`${name}: the render module did not load`, error);
        return;
      }
      if (abort.signal.aborted || this.#handle) return;
      const first = [...this.childNodes].filter(
        (n) => !(n.nodeName === "SCRIPT" && n.type === "application/json"),
      );
      this.replaceChildren();
      this.#handle = view.mount(this, {
        props: readProps(this, kinds),
        data: this.#data,
        first,
        emit: (verb, detail) => this.dispatchEvent(eventFor(this, verb, detail)),
        signal: abort.signal,
      });
      this.setAttribute("data-tehas-mounted", "");
    }
  }
  if (!customElements.get(name)) customElements.define(name, Island);
  return Island;
}
