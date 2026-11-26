### Changes

- The four events that had names and schemas but no raiser are raised now:
  `factory.limit-changed` when `tehas limit` or `tehas drain` writes
  another limit, `ticket.acted` when the dispatcher records an allowed
  account's approval, planning ask or finish ask, `ticket.closed` when a
  close the dispatcher wrote lands, and `change.merged` when a cycle's
  report ends its line `merged`. Each is raised once, by the process that
  records the fact, and written as one line to the log: the factory's
  `logs/dispatch.log` for the commands, the cycle's log and the poll's for
  the others. Nothing is pushed to another program.
