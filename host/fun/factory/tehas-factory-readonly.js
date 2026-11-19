// The plain dashboard's readonly island (host/docs/dashboard.frag).
import { island } from "../kit/island.js";

island("tehas-factory-readonly", {
  props: { src: String },
  load: () => import("./tehas-factory-readonly.render.js"),
});
