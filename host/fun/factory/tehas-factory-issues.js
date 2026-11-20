// The plain dashboard's issues island (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-issues", {
  props: { src: String, list: String },
  load: () => import("./tehas-factory-issues.render.js"),
});
