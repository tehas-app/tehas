### Changes

- A write to GitHub that timed out or got a 5xx answer is no longer sent again blindly: the action journal keeps it as `unknown`, and a poll reads GitHub for it (a comment by its body, a label by name) before it is sent again. A comment whose post timed out after GitHub took it is no longer posted twice, and a park question is no longer followed by a second, plain park comment (#1754).
- The state store moves to schema 16. `install.sh` and `tehas update` migrate it as usual.
