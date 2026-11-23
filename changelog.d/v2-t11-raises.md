### Changes

- A cycle raises its events: `cycle.started` once its start is on record, and after its report `cycle.parked`, `question.asked` for a park's question, `cycle.ended` and `change.offered`, in that order. Each carries what `tehas status`, `tehas history` and `tehas question` then print. The cycle's log has one line per event, `event` and its envelope as JSON. `tehas api --json` lists the feature word `events`. Nothing is sent to another program.
- The plugins' hook after a cycle is asked on `change.offered`. Its request and the hook's `/run/context.json` carry the event's envelope as `event`. Plugins stay a feature still being built.
- `tehas history` counts `repeat` over every line the store keeps, so a page read with `--after` gives a park the same count as the whole history does. Before, a page that began partway through a run of parks counted only what it listed.
