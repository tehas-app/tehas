// The plain dashboard's state island (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-state", {
  props: { src: String, running: String },
  load: () => import("./tehas-factory-state.render.js"),
});
