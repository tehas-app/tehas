### Changes

- The improvement loop is gone: `tehasbot improve`, `tehas-improve.timer` and its service, the analyst's prompt and the record each cycle left under `~/tehas/tmp/improve`. `TEHAS_IMPROVE_REPO`, `TEHAS_IMPROVE_RUKAS_REPO`, `TEHAS_IMPROVE_RUKAS_SKIP` and `TEHAS_IMPROVE_MAX_OPEN` are no longer read, and an instance that still sets one of them to a value stops every command with 78 and names it. A blank value, and `TEHAS_IMPROVE_RUKAS_SKIP=0`, are ignored (#1927).
- `TEHAS_CYCLE_REPORT` is no longer read: the cycle's report has one form. An instance that still sets it to a value, `rust` too, stops with 78 and names it; a blank value is ignored (#2194).
- The `cross-project` label is gone from the label list, and the dispatcher no longer holds an issue that carries it. `tehas:improvement` stays, described as a proposal the factory filed: the review files under it, and the millwright still does not start such an issue.
- `install.sh` and `tehas update` stop and disable `tehas-improve.timer` and remove both of its unit files, as they do for the old refresh timer.

### Adopter action

Take `TEHAS_IMPROVE_REPO`, `TEHAS_IMPROVE_RUKAS_REPO`, `TEHAS_IMPROVE_RUKAS_SKIP`, `TEHAS_IMPROVE_MAX_OPEN` and `TEHAS_CYCLE_REPORT` out of `instance.env` before you update: with any of them set to a value, every command stops with 78. The update removes the timer. `~/tehas/tmp/improve` and `~/tehas/logs/improve.log` are left on the host; remove them when you no longer need them. The `cross-project` label stays on GitHub until you delete it.
