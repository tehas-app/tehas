### Changes

- `tehas labels` makes `tehas:prototype` and pins it in `~/tehas/state/labels`
  with the granting labels. The Protolab puts no ticket at the prototype stage
  while the label under that name is not the pinned one, and does not publish
  a title changed after the labelling by an account outside
  `TEHAS_ALLOWED_LABELERS`. With `TEHAS_ISSUE_TYPES`, who set the type is
  checked by user id as well as login.

### Adopter action

On a host where `tehas:prototype` already exists, the labels step of the
update says `not pinned: the label tehas:prototype (<id>) was there before
with no pin` and stays owed. Check on GitHub that it is your prototype label,
add the line `tehas:prototype<tab><id>` to `~/tehas/state/labels`, and run
`tehas labels` again. Until then the Protolab lists no ticket.
