// A typo'd `load` in an island's spec.
import { island } from "../../../host/fun/kit/island.js";

island("kit-demo-lode", { props: { n: Number }, lode: () => import("./uses.js") }); // rejected: TS2353
