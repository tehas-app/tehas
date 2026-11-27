### Changes

- The anonymous read of the release project keeps a journal line for
  what it sees of GitHub: when a read finds the site failing or its
  budget spent, one plain line says so on the command's error output,
  which the unit's journal keeps; a change told by one process is not
  told again by the next. When the ledger cannot be read or written the
  line says that too, once per process.
