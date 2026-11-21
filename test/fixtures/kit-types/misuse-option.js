// A typo'd option of feed().
import { feed } from "../../../host/fun/kit/feed.js";

feed("/live/status.json", { evry: 10000 }); // rejected: TS2561
