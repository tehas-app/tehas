### Changes

- Only the factory's own pull requests hold an issue. An open pull request takes `tehas:ready` off an issue it closes only when its branch is in the product repository and the bot or an account in `TEHAS_ALLOWED_LABELERS` opened it. A pull request from a fork, or by any other account, holds nothing.
