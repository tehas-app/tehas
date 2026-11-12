### Changes

- The release workflow publishes only a tag whose commit is on `main` and that
  an admin of the repository pushed. A tag on any other commit, or one pushed by
  a bot or an account without admin rights, publishes nothing.
