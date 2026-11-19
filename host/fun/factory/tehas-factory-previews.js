// The plain dashboard's previews island (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-previews", {
  props: { src: String },
  load: () => import("./tehas-factory-previews.render.js"),
});
