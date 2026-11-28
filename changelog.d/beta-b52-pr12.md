### Changes

- `console pins check` and `console pins move` read the pinned tools'
  GitHub releases and tags through the code host's anonymous reads, the
  way `tehas update` reads its own project: no GitHub token is sent for
  those reads, and they spend the anonymous budget kept in the ledger.
  Their output lines are unchanged.
