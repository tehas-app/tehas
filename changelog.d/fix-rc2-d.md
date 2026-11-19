### Changes

- `install.sh` writes the store's backup copies, and removes them again, before it copies the first file. A `state/backup` that is a link out of the home, or a disk with no room for the copies, now stops the install with "nothing was installed", and `tehas update` puts `current` back, instead of leaving a new store that refuses the old database.
- `tehas update` writes `state/release-history` before it moves `current`, so a history it cannot write leaves `current` where it was and the message saying so is true.
- A cycle that waits on its issue's lock while `tehas stop` removes the lock file now takes the lock on the new file, where the dispatcher and the cleaner look.
- The state store checks the whole database once a poll (`poll-begin`) and in `upkeep`, `prune`, `migrate` and `check`, not on every heartbeat and record. On an old factory this saves a full read of the database every 10 seconds per running cycle.
- `store upkeep` also cuts `logs/preview.log`, `logs/protolab.log` and `logs/clean.log` past 64 MiB, as it does the dispatcher's log.
- `install.sh` no longer keeps `libexec/github-token` while a cycle runs; no cycle from before v0.30 can still run.
