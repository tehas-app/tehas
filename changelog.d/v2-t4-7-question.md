### Changes

- The state store moves to schema 33 (`tehas update` migrates it): `questions.body_form` says what a question's sum sums. 2.0 sums a question's visible text alone, without tehas's markers (`visible`); every sum of before is marked `rendered`, the comment as it was posted with its markers in. The first poll that reads such a question's comment adopts it in place, summed by its visible text when the comment reads as it was posted and with no sum when it was ticked or edited since, with one `INFO` line: it writes nothing to GitHub, edits no comment, changes no label and asks nothing again. `question-known` names the form as `sum_form`, and the store's new `question-adopt` makes the change.

### Adopter action

None: `tehas update` migrates the store, and the poll adopts the rest.
