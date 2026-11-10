### Changes

- The factory acts as one GitHub App, named by `TEHAS_GITHUB_APP_ID` alone. The load refuses an instance without it. The app's login is read from `state/github/login` and given to `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN`, so the millwright may be the bot itself.
- `TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN` in `instance.env` each print a warning and are ignored once the login file exists.
- `GH_TOKEN` in `~/.config/tehas/env`, `GH_CYCLE_TOKEN` and `TEHAS_CYCLE_LOGIN` are refused with exit 78. A cycle's pull requests are the app's own, so the host no longer counts a second account as the factory's.
- A cycle's `gh issue create`, `gh issue edit` and `gh issue comment` go through the host, which runs them with the app's token and refuses labels, assignees, state, another repository and any `<!-- tehas:` marker. The cycle's own token reads issues only.

### Adopter action

Set `TEHAS_GITHUB_APP_ID` in `instance.env` (`tehas setup --new-app` makes the app), and take `GH_TOKEN` out of `~/.config/tehas/env` and `TEHAS_CYCLE_LOGIN` and `GH_CYCLE_TOKEN` out of the settings.
