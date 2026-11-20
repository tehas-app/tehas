// A render module whose mount returns a prop, not a handle.
import { island } from "../../../host/fun/kit/island.js";

island("kit-demo-handle", { load: () => ({ mount: (_host, ctx) => ctx.props.name }) }); // rejected: TS2322
