// The data of a refresh's answer, read without asking whether it is ok.
import { feed } from "../../../host/fun/kit/feed.js";

const status = feed("/live/status.json");
status.refresh().then((a) => a.data); // rejected: TS2339
