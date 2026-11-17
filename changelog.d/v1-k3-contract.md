### Changes

- A new manual page, the contract, says what is public and stays compatible through 1.x and what is internal: the settings and the knob rule, the cycle inputs, analytics.db at schema 4, the pull request spec block, what a feature still being built leaves visible, and one table of exit statuses per public command.
- Every public command gives an exit status one meaning: 1 is a failure, 3 is "the command worked and the answer is no or not yet", 78 is settings only.
- `tehas status` exits 3, not 1, on `fail`, a stale health file or no health file yet.
- `tehas drain` exits 3, not 1, when cycles still run at the end of the wait.
- `tehas update --check` exits 3, not 1, when there is something to update. It still exits 1 when it finds something an update does not fix, or when the sandbox build's own check fails. `sandbox/build.sh --check` exits 3, not 1, when the pi-rukas commit moved.
- `tehas start` exits 3, not 1, when GitHub has no open issue of that number. GitHub not answering is still 1.
- `tehas check-product` exits 3, not 78, when a check fails.
- `tehas setup` exits 1 when a step fails, and 78 only when the step stopped on its settings. It no longer passes on a step's other statuses.
- `tehas analytics` exits 78, not 69, when analytics is off, and 1, not 69, 70 or 73, when the program is not installed, the database cannot be read or the file cannot be made.
- The forge page describes an internal trait: tehas 1.0 promises no forge interface.
- `api/renames-1.0.tsv` lists each changed exit status as a row of kind `value`.

### Adopter action

A script or a program that reads these exit statuses reads the new numbers: 3 where it read 1 from `tehas status`, `tehas drain`, `tehas update --check` and `tehas start`, 3 where it read 78 from `tehas check-product`, 78 or 1 where it read 69, 70 or 73 from `tehas analytics`, and 1 where it read 78 from a `tehas setup` whose product check failed.
