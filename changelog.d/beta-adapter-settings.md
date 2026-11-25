### Changes

- Two settings name the adapters a factory works with: `TEHAS_CODE_HOST` for the code host and `TEHAS_TICKETS` for the ticket tracker. This release builds `github` alone for both, and a missing setting means `github`. Any other value, `jira` among them, stops every command with 78.

### Adopter action

Nothing is required: an instance without the two lines loads as before. An instance that names its adapters writes `TEHAS_CODE_HOST=github` and `TEHAS_TICKETS=github` in `instance.env`.
