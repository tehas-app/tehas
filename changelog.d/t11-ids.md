### Changes

- `tehas api --json` prints `store`, the id of the factory's state store, and the feature word `cycle-ids`. `tehas state pack`, `unpack` and `load` keep the store's id and its attempts, so a moved factory never gives a cycle id twice.
- `tehas status --json` prints `cycle`, the cycle id `<issue>-<attempt>` such as `12-3`, with `how` and `by`, on each running cycle; the health file's cycles carry `attempt`, `how` and `by`. `since` is the second the cycle's history line prints as `started`.
- `tehas history --json` and `tehas export` print `cycle`, `end` (one of `offered`, `merged`, `handoff`, `parked`, `failed`, `refused`, `stopped`) and `change` (the number of the pull request, beside its address in `pr`) on every work line, and every work line has `started`: a line the store holds no start for started when it ended.

### Adopter action

A program that reads many factories can key a cycle on `cycle` together with the factory and its `store`, and decide on `end` in place of `outcome`, once `tehas api --json` lists `cycle-ids`.
