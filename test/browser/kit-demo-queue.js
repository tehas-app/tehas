import { island } from "../fun/kit/island.js";

island("kit-demo-queue", {
  props: { src: String },
  load: () => import("./kit-demo-queue.render.js"),
});
