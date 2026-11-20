import { island } from "../fun/kit/island.js";

island("kit-demo-wire", {
  props: { session: String },
  load: () => import("./kit-demo-wire.render.js"),
});
