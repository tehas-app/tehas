# The factory acts as a GitHub App

Spec for #639. Owner decisions of 2026-10-24 are in the issue's comments.

## Why

A factory acted as a machine user: one personal access token, `GH_TOKEN`, in
`~/.config/tehas/env`, used by every `gh` and `git` call on the host and given to
each cycle's container. On 2026-10-24 GitHub suspended that account
(`janit-or`). Every poll failed, cycles could not push, and everything the
account had written was hidden until reinstatement.

A GitHub App is the identity GitHub intends for automation. It is not a user,
its installation tokens last one hour and reach only the repositories it is
installed on, and its writes show as `<slug>[bot]`.

## How a token is made

1. The app's private key signs a JSON Web Token: header `{"alg":"RS256","typ":"JWT"}`,
   payload `{"iat": now-60, "exp": now+540, "iss": "<app id>"}`, each part
   base64url without padding, signed with `openssl dgst -sha256 -sign <key>`.
   No dependency beyond openssl, curl and jq, which a host already has.
2. `POST https://api.github.com/app/installations/<installation id>/access_tokens`
   with `Authorization: Bearer <jwt>` answers `{"token": "...", "expires_at": "..."}`.
3. The token is kept in `$TEHAS_HOME/state/github-token` (mode 600) with its
   expiry in `github-token.expires` (epoch seconds), and made again when fewer
   than 10 minutes are left. One mint at a time: `github-token.lock`.

`lib/github-app.sh` does this (`tehas_github_token`).

## Where a token is used

- **The host's commands** call `gh` and `git` many times, and a cycle's
  `libexec/cycle` runs for hours. A token read once at a command's start would
  run out. So the token is asked for at each call: a `gh` shim first on the
  PATH and a git credential helper, both asking `tehas_github_token`.
- **A cycle's container** runs pi-rukas, which pushes, opens and readies pull
  requests itself. It cannot be handed a token once. It is given the token file
  read-only, the same `gh` shim and credential helper reading that file, and
  the host keeps the file fresh while the cycle runs. This changes what a
  container is given and is a security change (AGENTS.md rule 4).
- **The bot's login** comes from `TEHAS_BOT_LOGIN`. An installation token
  cannot ask `GET /user` (403), which is how the dispatcher learns it today.

## Settings

In `instance.env` (not secrets):

| Setting | Value |
| --- | --- |
| `TEHAS_GITHUB_APP_ID` | the app's ID, a whole number |
| `TEHAS_GITHUB_APP_INSTALLATION` | the installation's ID, a whole number |
| `TEHAS_BOT_LOGIN` | the app's login, `<slug>[bot]` |

All three or none. With none, the factory uses `GH_TOKEN` as before.

The private key is a secret and lives outside the instance directory:
`~/.config/tehas/github-app.pem`, mode 600, readable by the tehas user only,
never given to a container.

## Parts

1. The minter and its settings (`lib/github-app.sh`, `lib/config.sh`), with
   tests that stub `openssl` and `curl`.
2. The host's commands: the `gh` shim, the git credential helper, the PATH in
   `host/env.sh`, `TEHAS_BOT_LOGIN` in the dispatcher.
3. The container: the token file kept fresh, the shim and helper in the
   sandbox image, the mounts in `lib/sandbox.sh` and `test/work_test.sh`.
4. `tehas setup` takes the app, and the manual's pages say how to make one.

## For adopters: making an app for a factory

These notes become the manual's page in part 4.

1. In the organisation that owns the product repository: Settings, Developer
   settings, GitHub Apps, New GitHub App.
   - Name: one per factory is the safest (a suspended or leaked app stops one
     factory). Its slug is the bot's login, `<slug>[bot]`.
   - Homepage URL: the repository's URL. Nothing calls it.
   - Webhook: off (untick Active). tehas polls; it needs no public address.
   - Callback URL and setup URL empty; "Request user authorization during
     installation" and "Enable Device Flow" off. "Expire user authorization
     tokens" can stay on: tehas uses installation tokens, which always last
     one hour.
   - Where it can be installed: only on this account.
2. Repository permissions, everything else "No access":

   | Permission | Access |
   | --- | --- |
   | Contents | Read and write |
   | Issues | Read and write |
   | Pull requests | Read and write |
   | Metadata | Read-only |
   | Actions | Read-only |
   | Checks | Read-only |
   | Commit statuses | Read-only |

   Workflows stays "No access" on purpose: no cycle can write `.github/workflows/`.
   No organisation or account permissions, no events.
3. Create the app. Note the App ID on its General page, and generate a private
   key (a `.pem` file downloads).
4. Install it: the app's page, Install App, Install next to the organisation,
   and choose the repositories. The installation's ID is the number at the end
   of its settings page's URL (`.../settings/installations/<id>`).
5. On the host, as the tehas user: put the key at
   `~/.config/tehas/github-app.pem` and `chmod 600` it. Put the three settings
   in the instance's `instance.env`.
6. Check it: `tehasbot github-token --check` mints one token and prints its
   expiry and permissions, never the token.

Seen on the first app (`tehas-app-bot`): a token's repository answer says
`"push": false`. That field is a user's access and is not filled for an app;
Contents write is what lets it push.
