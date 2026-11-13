### Changes

- A `tehas:finish` the poll ignores no longer counts toward the three asks a
  poll runs, and no longer keeps a failed end from being finished by the poll.
- The poll reads every open pull request for the hold on a closing keyword,
  not the newest 100, and `tehasbot refresh` reads them all before it removes a
  request. More than 5000 open pull requests is a read that failed.
- The note that a `tehas:blocked` hold has cleared counts as said only when
  the bot wrote it.
