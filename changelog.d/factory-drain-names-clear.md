### Changes

- A drained factory says what frees it. While `tehas drain` holds the factory,
  the dispatcher's hold line ends `; tehas limit clear starts the factory`, and
  `tehas update` that ended well on such a factory prints one line last naming
  the same command and saying this version has run no cycle yet. A limit that
  merely holds every slot, and one that expired, keep their words; the new line
  of an update changes no exit status, and `--check` and `--pending` print
  nothing new (#2229).
