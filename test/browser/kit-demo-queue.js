import { island } from "../fun/kit/island.js";

island("kit-demo-queue", {
  props: { src: String, every: Number },
  load: () => import("./kit-demo-queue.render.js"),
});
