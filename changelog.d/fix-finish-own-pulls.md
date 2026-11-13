### Changes

- `tehas-cycle finish` counts only the factory's own open pull requests, as the poll does: one on a branch of the repository that the bot or an account of `TEHAS_ALLOWED_LABELERS` opened. A pull request from a fork, or from any other account, that names the issue no longer stops it. It reads the list through GraphQL and refuses when `TEHAS_ALLOWED_LABELERS` names no account.
