### Added

- `TEHAS_MILLWRIGHT_APP=<slug>[bot]` in `instance.env` names a second GitHub App the factory trusts as its millwright in place of its own app. The load gives it to `TEHAS_MILLWRIGHT_LOGIN`; the factory still acts as its own app, whose `tehas:ready` then counts as any outsider's. A value that is not `<slug>[bot]` stops every command with 78. A `TEHAS_MILLWRIGHT_LOGIN` line still warns and is ignored.
