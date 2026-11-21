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

/**
 * @typedef {StringConstructor | NumberConstructor | BooleanConstructor} Kind
 * @typedef {Record<string, Kind>} Kinds
 * @typedef {Record<string, string | number | boolean>} Props
 * @typedef {{ props: Props, data: any, first: ChildNode[], emit: (verb: string, detail?: unknown) => boolean,
 *   signal: AbortSignal }} Ctx
 * @typedef {{ update?: (props: Props) => void, unmount: () => void }} Handle
 * @typedef {{ mount: (host: HTMLElement, ctx: Ctx) => Handle }} RenderModule
 * @typedef {{ props?: Kinds, load: () => RenderModule | Promise<RenderModule> }} IslandSpec
 */

const TRIGGERS = ["load", "visible", "idle", "interaction"];

// Waits for the trigger the element names, then calls go. Aborting `signal`
// stops the wait.
/**
 * @param {HTMLElement} el
 * @param {AbortSignal} signal
 * @param {() => void} go
 */
function whenTriggered(el, signal, go) {
  const how = el.getAttribute("hydrate") || "load";
  if (!TRIGGERS.includes(how) || how === "load") {
    queueMicrotask(go);
    return;
  }
  if (how === "visible") {
    // An element laid out as its children has no box of its own, so it is
    // never seen: watch its children instead.
    const watched =
      getComputedStyle(el).display === "contents"
        ? [...el.children].filter((c) => c.localName !== "script")
        : [el];
    if (watched.length === 0) {
      queueMicrotask(go);
      return;
    }
    const seen = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        seen.disconnect();
        go();
      }
    });
    for (const w of watched) seen.observe(w);
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
/**
 * @param {Element} el
 * @param {Kinds} kinds
 * @returns {Props}
 */
export function readProps(el, kinds) {
  /** @type {Props} */
  const props = {};
  for (const [name, kind] of Object.entries(kinds)) {
    const attr = attrName(name);
    if (kind === Boolean) props[name] = el.hasAttribute(attr);
    else if (el.hasAttribute(attr)) {
      const raw = /** @type {string} */ (el.getAttribute(attr));
      props[name] = kind === Number ? Number(raw) : kind(raw);
    }
  }
  return props;
}

/** @param {string} name */
export function attrName(name) {
  return name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

// A CustomEvent `<element>:<verb>` that bubbles, its detail plain JSON.
/**
 * @param {Element} el
 * @param {string} verb
 * @param {unknown} [detail]
 */
export function eventFor(el, verb, detail) {
  const plain = detail === undefined ? null : JSON.parse(JSON.stringify(detail));
  return new CustomEvent(`${el.localName}:${verb}`, { bubbles: true, detail: plain });
}

// Defines the custom element `name` on the island base. `spec.props` names the
// attributes, `spec.load` returns the render module (or a promise of it).
/**
 * @param {string} name
 * @param {IslandSpec} spec
 */
export function island(name, spec) {
  const kinds = spec.props ?? {};
  class Island extends HTMLElement {
    static observedAttributes = Object.keys(kinds).map(attrName);
    /** @type {AbortController | null} */
    #abort = null;
    /** @type {Handle | null} */
    #handle = null;
    /** @type {unknown} */
    #data = undefined;

    connectedCallback() {
      if (this.#abort) return;
      const abort = new AbortController();
      this.#abort = abort;
      if (this.#data === undefined) {
        const json = this.querySelector(":scope > script[type='application/json']");
        try {
          this.#data = json ? JSON.parse(/** @type {string} */ (json.textContent)) : null;
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

    /**
     * @param {string} _attr
     * @param {string | null} before
     * @param {string | null} after
     */
    attributeChangedCallback(_attr, before, after) {
      if (before !== after) this.#handle?.update?.(readProps(this, kinds));
    }

    /** @param {AbortController} abort */
    async #mount(abort) {
      /** @type {RenderModule} */
      let view;
      try {
        view = await spec.load();
      } catch (error) {
        console.error(`${name}: the render module did not load`, error);
        return;
      }
      if (abort.signal.aborted || this.#handle) return;
      const first = [...this.childNodes].filter(
        (n) => !(n.nodeName === "SCRIPT" && /** @type {HTMLScriptElement} */ (n).type === "application/json"),
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
