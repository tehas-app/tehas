### Changes

- The All hands label is gone. `tehas:all-hands` on the control issue is no longer read: `tehas status` prints no `all-hands` line, `tehas status --json`, `live/status.json` and `checks` of the health file have no `allHands`, and the dashboard shows no "all hands" mark. `tehas labels` no longer creates the label, and `tehas api --json` no longer lists the feature word `all-hands`.

### Adopter action

Nothing to do. A `tehas:all-hands` label left on a control issue does nothing; delete it on GitHub if you like. `~/tehas/state/all-hands.ignored` is left on the host where an earlier version wrote it; remove it when you like. A program that read `allHands` or checked `features` for `all-hands` finds them gone.
