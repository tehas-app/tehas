### Changes

- The poll reads the issue queue through the forge ports instead of writing
  GitHub's queries itself. Nothing it decides changes. The queue's one large
  page is read as four smaller reads, so a poll's count of GitHub calls
  rises a little (an idle poll: 1 call before, 6 now) and its cost line
  counts them the same as before.
