// The plain dashboard's host island (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-host", {
  props: { src: String },
  load: () => import("./tehas-factory-host.render.js"),
});
