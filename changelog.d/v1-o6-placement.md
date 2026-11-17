### Changes

- Outsource, still being built, gains placement in the dispatcher. With the
  feature on, a start from the ready queue goes to a contractor only when its
  repository is in `TEHAS_OUTSOURCE_REPOS`, the issue has lost fewer than
  `TEHAS_OUTSOURCE_RETRIES` jobs, and a contractor fits: heard from within
  `TEHAS_OUTSOURCE_TIMEOUT`, not revoked, draining, resting or busy, with this
  host's image and the room a cycle needs. The one with the most room wins.
  Anything else, or a read that fails, runs the cycle in-house. The start's
  line in `dispatch.log` says where it runs. A release still refuses
  `TEHAS_OUTSOURCE=on`, so nothing changes on a factory (#2275).
