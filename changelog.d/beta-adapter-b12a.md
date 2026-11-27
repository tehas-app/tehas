### Changes

- The GitHub adapter reads and writes a renamed project as its own: where
  an answer names the project under its new name, the adapter asks once for
  the name GitHub gives with the repository's id and holds the answer to
  that, where that name has the configured owner. On an instance whose
  `TEHAS_REPO` still names the old name, the adapter no longer reads its
  own issues, comments and pull requests as unread after a write, and a
  blueprint search finds them. The dispatcher's
  poll, the plugin hooks, the preview sweep and the cycle's issue relay
  still compare the configured name: set `TEHAS_REPO` to the new name.
