### Changes

- A person's `tehas:ready` after a parked cycle and an automatic refine starts
  the issue again. GitHub can cut an issue's label timeline right after the
  label its last cycle started on, and the poll then read that old approval and
  skipped the issue on every poll with "its approval of ... started a cycle
  already". A candidate whose newest `tehas:ready` or `tehas:blocked` on the
  page is no newer than the approval of its last start now has its label events
  read whole.
