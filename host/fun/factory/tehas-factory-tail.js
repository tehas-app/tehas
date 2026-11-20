// <tehas-factory-tail file="console-41.txt" live>: a plain element that tails
// a console file under /live/ into the <pre> inside it (host/docs/frontend.md,
// "Three tiers"). It asks only for the bytes it has not read, every two
// seconds, and follows the output while the checkbox inside it is checked. A
// console is text that grows, not a JSON feed, so it is read here and not
// through kit/feed.js. The text is untrusted: it is only ever set as text.
const EVERY = 2000;
const KEEP = 400000; // past this many characters, the oldest go
const CUT = 300000; // and this many stay
// The name is put into a path under /live/.
const NAME = /^[\w-][\w.-]*$/;

class Tail extends HTMLElement {
  static observedAttributes = ["file"];
  #file = null;
  #offset = 0;
  #busy = false;
  #timer = 0;

  connectedCallback() {
    this.#timer = setInterval(() => this.#read(), EVERY);
    this.#start();
  }

  disconnectedCallback() {
    clearInterval(this.#timer);
    this.#file = null;
  }

  attributeChangedCallback(_name, before, after) {
    if (before !== after && this.isConnected) this.#start();
  }

  #start() {
    const file = this.getAttribute("file");
    this.#file = file && NAME.test(file) ? file : null;
    this.#offset = 0;
    const pre = this.querySelector("pre");
    if (pre) pre.textContent = "";
    this.#read();
  }

  #note(text) {
    const note = this.querySelector(".console-bar .muted");
    if (note) note.textContent = text;
  }

  async #read() {
    // Two requests for the same offset would both append their answer.
    if (!this.#file || this.#busy) return;
    this.#busy = true;
    const file = this.#file;
    try {
      const r = await fetch("/live/" + file, { headers: { Range: "bytes=" + this.#offset + "-" }, cache: "no-store" });
      // Another file was chosen while this request was out: its answer is for the old one.
      if (file !== this.#file) return;
      const pre = this.querySelector("pre");
      if (r.status === 206 || (r.status === 200 && this.#offset === 0)) {
        const buf = new Uint8Array(await r.arrayBuffer());
        if (file !== this.#file) return;
        if (buf.length && pre) {
          this.#offset += buf.length;
          pre.append(new TextDecoder().decode(buf));
          if (pre.textContent.length > KEEP) pre.textContent = pre.textContent.slice(-CUT);
          if (this.querySelector("input[type=checkbox]")?.checked) pre.scrollTop = pre.scrollHeight;
        }
      } else if (r.status === 200 && pre) {
        // The file was replaced by a new cycle.
        this.#offset = 0;
        pre.textContent = "";
      }
      this.#note(this.hasAttribute("live") ? "live" : "ended");
    } catch {
      this.#note("console unavailable");
    } finally {
      this.#busy = false;
    }
  }
}

if (!customElements.get("tehas-factory-tail")) customElements.define("tehas-factory-tail", Tail);
