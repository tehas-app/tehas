// The plain dashboard's console island (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-console", {
  props: { src: String, status: String },
  load: () => import("./tehas-factory-console.render.js"),
});
