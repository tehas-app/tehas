### Changes

- A write of the GitHub adapter confirms the repository it is about to
  change: one that is not this project's gets no pull request, status,
  comment, label or issue edit, and the write is refused as unread with
  nothing sent (#3432). A handle's first write asks GitHub one GraphQL
  question for this.
