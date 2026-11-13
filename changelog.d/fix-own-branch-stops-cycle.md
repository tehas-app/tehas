### Changes

- A running cycle is stopped for a merged or closed pull request only when
  that pull request is on its issue's own branch, `feature/issue-N`. A pull
  request on another branch whose description says `Fixes #N` no longer stops
  the cycle of #N.
