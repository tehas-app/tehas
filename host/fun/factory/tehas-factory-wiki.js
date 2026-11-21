// The wiki page's island (host/docs/wiki.frag).
import { island } from "../kit/island.js";

island("tehas-factory-wiki", {
  props: { src: String },
  load: () => import("./tehas-factory-wiki.render.js"),
});
