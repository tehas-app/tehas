// A method a feed does not have.
import { feed } from "../../../host/fun/kit/feed.js";

feed("/live/status.json").putt(1); // rejected: TS2551
