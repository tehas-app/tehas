### Changes

- `console pins check` and `console pins move` read the pinned tools'
  GitHub releases and tags through the code host's anonymous reads, the
  way `tehas update` reads its own project: no GitHub token is sent for
  those reads, and they spend the anonymous budget kept in the ledger.
  What a check and a move print is unchanged when the reads answer; when
  one of those two reads fails, its line now says
  `pins: cannot read <repo>: <what the code host answered>` in place of
  the old curl words.
