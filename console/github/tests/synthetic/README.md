# Synthetic answers

Hand-made stubs, not recordings. Each starts from the live recording
`recorded/what/live/from-1.answer.json` (the one live answer of an aliased
question here; no edit-history question is recorded live yet) and changes
only what makes the case. Their `locations` and `message` are that
recording's, copied as they are.

- `edits/inner-not-found/bodies.json`: to be replaced by a live recording,
  see the kit list (`d`, edits-not-found). Live answer's error object, `path`
  set to `["repository","i7","userContentEdits"]` (a NOT_FOUND inside issue 7)
  and `["repository","i8"]`; data set to `i7` and `i8` null.
- `edits/inner-not-found/nodes.json`: to be replaced by a live recording,
  see the kit list (`d`, edits-not-found). As above, by node: `path`
  `["nodes",0,"userContentEdits"]` and `["nodes",1]`; data `nodes` two nulls
  and `repository` with no `id`, which the reader does not read and no live
  recording gives for the fixture repository.
- `edits/missing-items/bodies.json`: an item missing from the answer, which
  GitHub cannot be made to give. Live answer with every alias and the errors
  taken out.
- `edits/missing-items/nodes.json`: the same by node: `nodes` empty, beside
  `repository` with no `id` as above.
- `what/limited/from-3.answer.json`: a whole-question RATE_LIMITED, which
  cannot be made on demand without spending a real budget. Live answer's
  `data` as it is; its `errors` replaced by one RATE_LIMITED error with no
  `path`. No real rate-limit answer is recorded to copy from, so the message
  is GitHub's documented text with no account id.
- `what/limited/from-3.fail`: `recorded/what/live/from-1.fail` as it is.
