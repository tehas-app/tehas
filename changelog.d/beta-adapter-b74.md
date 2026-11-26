### Changes

- The GitHub adapter confirms the repository before it writes: a handle's
  first write asks GitHub once for the id and the name the configured
  repository resolves to. A repository transferred to another owner, or
  the old name made again as another repository, gets no pull request,
  status, comment, label or issue edit from the factory: the write is
  refused as unread with nothing sent, where before GitHub's redirect
  decided where it landed (#3432). Each handle that writes asks one more
  GraphQL question.
