### Changes

- The dispatcher judges every edit of an issue's body after the approval, not only the last. An edit by an account outside `TEHAS_ALLOWED_LABELERS` now holds the issue even when an allowed account saved the body after it, and so does an edit history longer than the 100 edits the poll reads. The poll reads that history only for edited issues it may start. This is a security fix.
- A GitHub write whose connection broke after it was made (a reset, an EOF, a GOAWAY, a closed connection) is read back before it is sent again, as a timeout or a 5xx already was. Before, a comment could be posted twice.
- `tehasbot status` reads the list of every issue for the breakroom's words at most once an hour, and keeps the words between reads. Each status run was about ten GitHub requests heavier before.
