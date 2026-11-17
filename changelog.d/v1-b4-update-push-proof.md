### Changes

- `tehas update` and `install.sh --check` prove that the GitHub App's cycle token can push, with a dry-run push from the product checkout. When it cannot, they print `Cycle token:  CANNOT PUSH` with the app's login, git's message and what the app needs, and exit 1; a drained factory stays drained (#1740).
