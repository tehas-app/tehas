// The dashboard's link to the current wiki build (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-wikilink", {
  props: { src: String },
  load: () => import("./tehas-factory-wikilink.render.js"),
});
