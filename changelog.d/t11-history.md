### Changes

- Every start of a cycle has its line in the history. A start the cycle refused writes a line with the outcome `refused` and the exit status its unit left; `tehas stop` writes one with `stopped`; a cycle that died without a word writes one with `lost`, ended when it was last heard from. Before, these three wrote only the short list of ends the dashboard shows. `tehas history` and `tehas export` list a work line that has no exit status.
- A lost cycle is ended by the next start of its issue, as before, and now also by the poll: an open record whose lock no process holds is ended through the state store's new `cycle-end --lost`, which the store takes only while the lock is free and the cycle has not been heard from for two minutes, as the dashboard shows it lost.
- A work line written after its end, a lost cycle's, has `recorded`, when it was written; `--after` lists it once, in the order the lines were written. A line of a cycle a signal stopped has `signalled: true`.
- The history keeps an issue's 50 newest lines by the order they were written, not by their end, so a line written late is never the one dropped first.

- The state store moves to 2.0's schema, 32 (`tehas update` migrates it); in its step to schema 31 a line of the history says whether a cycle's report wrote it (`cycle_history.reported`). The record of an approval reads only those lines, so a refused, stopped or lost start does not move the base a refresh rests on, the count of refreshes or the easy merge, as before.
- `tehas stop` says the record of a cycle was already ended, and how, when the poll ended it as lost just before the stop did, in place of saying it ended it as stopped.

### Adopter action

A program that reads `tehas history` or `tehas export` sees a line for each refused, stopped and lost start, which it did not see before, and a work line may have no `status`. One that counts runs from the history counts these too; one that keys lines on their cursor reads `recorded` on a lost line and takes it in the order it was written.
