### Changes

- The GitHub adapter reads and writes a renamed project as its own: where
  an answer names the project under its new name, the adapter asks once for
  the name GitHub gives with the repository's id and holds the answer to
  that. An instance whose `TEHAS_REPO` still names the old name no longer
  reads its own issues, comments and pull requests as unread after a write.
