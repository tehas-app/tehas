### Changes

- The `tehas:prototype` label and the `prototype` kind are gone. `tehas labels` no longer makes or pins the label, and nothing reads it. A ticket not yet in line is still an Idea: an open issue with none of the queue's state labels.
- `TEHAS_ISSUE_TYPES` maps only `improvement`. A `prototype=` pair still loads, with one warning line, and is dropped.
- Neither `tehas update` nor `tehas setup` deletes the label from a repository. Where it is still there it does nothing, and a leftover `tehas:prototype` line in `state/labels` is not read.

### Adopter action

Take any `prototype=` pair out of `TEHAS_ISSUE_TYPES` in `instance.env`. You can delete the `tehas:prototype` label from the product repository, or leave it.
