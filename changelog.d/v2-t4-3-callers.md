### Changes

- `tehas labels` makes the labels from the table compiled into the console and no longer reads `~/tehas/lib/labels.tsv`. It prints one line, `made the tehas labels`, in place of one line per label, and a label that cannot be made is said by its outcome. A missing or malformed installed table no longer stops it with 78.
- `tehas resume` and the label checks of `tehas plan` and `tehas dev` read GitHub through the tickets port. A login of `TEHAS_ALLOWED_LABELERS` now matches the account that applied a label in any case, as GitHub logins do, and the login they print is in lower case.
