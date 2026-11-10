### Changes

- The factory acts as one GitHub App, named by `TEHAS_GITHUB_APP_ID` alone. The load refuses an instance without it. The app's login is read from `state/github/login` and given to `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN`, so the millwright may be the bot itself.
- `TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN` in `instance.env` each print a warning and are ignored once the login file exists.
- `GH_TOKEN` in `~/.config/tehas/env`, `GH_CYCLE_TOKEN` and `TEHAS_CYCLE_LOGIN` are refused with exit 78. A cycle's pull requests are the app's own, so the host no longer counts a second account as the factory's.
- A cycle's `gh issue create`, `gh issue edit` and `gh issue comment` go through the host, which runs them with the app's token and refuses labels, assignees, state, another repository and any `<!-- tehas:` marker. A cycle comments only on its own issue and on issues it created, edits only issues it created, and has at most 50 such writes. The cycle's own token reads issues only.
- `tehas setup --new-app` makes the factory's GitHub App from a manifest: it prints one link for your browser, takes back the address GitHub opens, and writes the app's key and `TEHAS_GITHUB_APP_ID`. `tehas setup --app-id <id> --app-key-file <key>` takes an app you have. `--token-file` is refused; `--app-installation` is ignored, as the installation is looked up. Setup no longer writes `TEHAS_GITHUB_APP_INSTALLATION` or `TEHAS_BOT_LOGIN`, and ends with `tehasbot github-token --check`.
- A machine user's token is no longer a way to run a factory. Each refusal prints `<NAME> is no longer used: the factory acts as a GitHub App. Run tehas setup --new-app or give --app-id and --app-key-file.`
- `install.sh` removes `libexec/github-token` even while a cycle runs; a cycle renews its token with `tehasbot github-token`.
- The manual's GitHub App page starts with `tehas setup --new-app`, and the pages on the host, the millwright and bootstrap describe one app for everything the factory does.

### Adopter action

Set `TEHAS_GITHUB_APP_ID` in `instance.env` (`tehas setup --new-app` makes the app), take `GH_TOKEN` and `GH_CYCLE_TOKEN` out of `~/.config/tehas/env`, and take `TEHAS_CYCLE_LOGIN` out of the settings. Take `TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN` out too, to stop their warnings.
