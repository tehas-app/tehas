// The plain dashboard's days island (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-days", {
  props: { src: String },
  load: () => import("./tehas-factory-days.render.js"),
});
