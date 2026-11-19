### Changes

- Security fix: `tehas:blocked` is now a pinned label, like `tehas:ready`. A lone `tehas:blocked` starts an issue once its blockers close, so another label renamed to `tehas:blocked` no longer brings its old applications with it as approvals. `tehas labels` pins it. A `tehas:blocked` the pins refuse starts nothing but still holds the issue it is on.

### Adopter action

A host that already has `~/tehas/state/labels` has no pin for `tehas:blocked`. Until it has one, a lone `tehas:blocked` starts nothing, and the poll logs `refused: the label tehas:blocked (<id>) has no pin`. Run `tehas labels`: it names the label's id and exits 1. Check on GitHub that the label called `tehas:blocked` is the one tehas made, then add the line `tehas:blocked<tab><id>` to `~/tehas/state/labels` as the command says. A host with no such file pins it on its own.
