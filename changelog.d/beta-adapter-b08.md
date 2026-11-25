### Changes

- The poll reads the edit history of an issue's body through the GitHub adapter, as its other reads of GitHub go. What the start rules judge is unchanged: every edit after the approval counts, and a history longer than 100 versions holds the issue as changed by `unknown`. A history whose parts do not agree, such as one with a version deleted from it, now holds the issue the same way instead of being read as it stood. A history that cannot be read still ends the poll, its line in tehas's words (`ERROR: unreadable edit history of #N: the site could not be reached`).
