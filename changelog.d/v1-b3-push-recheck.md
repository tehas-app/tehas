### Changes

- The poll ends a sandbox push fault on its own. Once
  `state/push-fault.json` is ten minutes old, the poll runs the dry-run push
  a start runs, with the App's cycle token and no cycle spent on it, in a
  fresh copy of the checkout as a start's workspace is. A pass
  takes the file off and the queue starts; a refusal writes the fault fresh
  and holds starts for another ten minutes. When the poll cannot ask at all,
  it records nothing and the next start's own check decides, as before
  (#2130).
- While the fault stands, `tehas status` prints
  `push      the sandbox cannot push: <git's line> (<login>)`,
  `tehasbot health` fails with it, and `tehas status --json` carries an
  optional `pushFault` object (#2130).
