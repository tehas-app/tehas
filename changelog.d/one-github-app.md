### Changes

- The factory acts as one GitHub App, named by `TEHAS_GITHUB_APP_ID` alone. The load refuses an instance without it. The app's login is read from `state/github/login` and given to `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN`, so the millwright may be the bot itself.
- `TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN` in `instance.env` each print a warning and are ignored once the login file exists.
- `GH_TOKEN` in `~/.config/tehas/env`, `GH_CYCLE_TOKEN` and `TEHAS_CYCLE_LOGIN` are refused with exit 78. A cycle's pull requests are the app's own, so the host no longer counts a second account as the factory's.
- A cycle's `gh issue create`, `gh issue edit` and `gh issue comment` go through the host, which runs them with the app's token and refuses labels, assignees, state, another repository and any `<!-- tehas:` marker. A cycle comments only on its own issue and on issues it created, edits only issues it created, and has at most 50 such writes. The cycle's own token reads issues only.
- `tehas setup --new-app` makes the factory's GitHub App from a manifest: it prints one link for your browser, takes back the address GitHub opens, and writes the app's key and `TEHAS_GITHUB_APP_ID`. `tehas setup --app-id <id> --app-key-file <key>` takes an app you have. `--token-file` is refused; `--app-installation` is ignored, as the installation is looked up. Setup no longer writes `TEHAS_GITHUB_APP_INSTALLATION` or `TEHAS_BOT_LOGIN`, and ends with `tehasbot github-token --check`.
- A machine user's token is no longer a way to run a factory. Each refusal prints `<NAME> is no longer used: the factory acts as a GitHub App. Run tehas setup --new-app or give --app-id and --app-key-file.`
- `install.sh` keeps `libexec/github-token` while a cycle that started before the update runs, so that cycle can still renew its token, and removes it at the next run with no cycle. A new cycle renews its token with `tehasbot github-token`. The next release drops this.
- `install.sh` and `tehas update` show a setting that is no longer used as a warning and go on; a refused setting still stops them.
- A planning turn and a dev session renew their narrowed token every 5 minutes while they run (`tehasbot github-token --session`), so a session longer than an hour keeps GitHub.
- A comment of the millwright's counts as its answer to a planning question only when its first line is `<!-- tehas:millwright issue=<n> action=answer -->`. The millwright is the bot, so the host's own notes are its comments too, and they no longer count as replies.
- The narrowed token of a container has Contents and Pull requests write, Issues read and Metadata read. GitHub lets Pull requests write rename the repository's labels, so each granting `tehas:` label is pinned to its GitHub id in `state/labels`: `tehas labels` pins the labels it makes, a host from before the pins is pinned by its first poll, and after that the poll pins nothing. An application of a label with another id, or of a granting label with no pin, grants nothing and is logged.
- Once the app's login is known (the first token minted after the update), the millwright is the app. A separate millwright account loses its rights from then on: its `tehas:ready` is taken off as an unauthorised label, and its marked comments carry no grant.
- The manual's GitHub App page starts with `tehas setup --new-app`, and the pages on the host, the millwright and bootstrap describe one app for everything the factory does.

### Adopter action

Set `TEHAS_GITHUB_APP_ID` in `instance.env` (`tehas setup --new-app` makes the app), take `GH_TOKEN` and `GH_CYCLE_TOKEN` out of `~/.config/tehas/env`, and take `TEHAS_CYCLE_LOGIN` out of the settings. Take `TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN` out too, to stop their warnings.

There is nothing to configure for the millwright: the app is the millwright. Whatever acted as a separate millwright account must act as the app from the update on.

If you delete and recreate a `tehas:` label, remove its line from `~/tehas/state/labels`, delete the label, and run `tehas labels` to make and pin it.
