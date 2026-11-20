import { island } from "../fun/kit/island.js";

island("kit-demo-picked", {
  props: { src: String },
  load: () => import("./kit-demo-picked.render.js"),
});
