### Changes

- The four events that had names and schemas but no raiser are raised now:
  `factory.limit-changed` when `tehas limit` or `tehas drain` writes
  another limit, `ticket.acted` when a start's claim or a planning turn's
  mark takes the issue its person asked for (an approval, a planning ask or
  a finish ask), `ticket.closed` when a close the dispatcher wrote lands,
  keyed on the site's own time of it, and `change.merged` when a cycle's
  report ends its line `merged`. Each is raised once, by the process that
  proved the fact, and written as one line to the log: the factory's
  `logs/dispatch.log` for the commands, the cycle's log and the poll's for
  the others. A claim or mark that fails raises nothing, and the next poll
  raises the act once; a close whose answer was lost is raised by the poll
  whose settle reads the ticket closed. Nothing is pushed to another
  program.
