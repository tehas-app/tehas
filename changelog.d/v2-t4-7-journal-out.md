### Changes

- A read-only host's dispatcher and cycle no longer record a journal row or send a request for a write to GitHub. They log the write they did not make in `state/read-only.log` and go on as before.
- A comment whose text holds one of tehas's markers, with no marker given beside it, is not posted. The dispatcher says so in `logs/dispatch.log`. Nothing in tehas writes such a comment today.
