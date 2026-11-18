### Changes

- `TEHAS_POLL_SECS` sets how often the dispatcher timer polls, from 30 to 3600 seconds, 120 by default. `tehas setup` and `tehas update` apply it as a drop-in for `tehas-dispatch.timer` and restart the timer, and remove the drop-in at the default. `tehas status` names an interval off the default on its poll line.
- `tehas dispatch --soon` asks for a poll and returns at once: it writes `state/dispatch.again`, and `tehas-dispatch.path` starts the poll, or starts it after the poll that is running. Another program on the host, such as a webhook receiver, can use it to wake the dispatcher.
