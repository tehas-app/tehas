---
title: A GitHub App for a factory
description: "Making the GitHub App a tehas factory acts as, instead of a machine user."
---

# A GitHub App for a factory

A factory acts on GitHub as one identity: it labels, comments, pushes branches
and opens pull requests. Make that identity a **GitHub App**. A machine user (a
second GitHub account with a personal access token) still works, but GitHub can
suspend a user account that automation drives hard, and then everything the
account wrote is hidden and every poll fails. That happened to tehas-tehas on
2026-10-24.

An app is not a user. Its installation tokens last one hour, reach only the
repositories it is installed on, and its writes show as `<slug>[bot]`. tehas
makes and renews the tokens itself: nothing on the host holds a long-lived
GitHub token.

## Make the app

In the organisation (or account) that owns the product repository: **Settings
→ Developer settings → GitHub Apps → New GitHub App**.

| Field | Value |
| --- | --- |
| GitHub App name | One per factory, for example `<product>-factory`. Its slug becomes the bot's login, `<slug>[bot]` |
| Homepage URL | The product repository's URL. Nothing calls it |
| Callback URL, Setup URL | Empty |
| Expire user authorization tokens | Leave on. tehas uses installation tokens, which always last one hour |
| Request user authorization (OAuth) during installation | Off |
| Enable Device Flow | Off |
| Webhook → Active | Off. tehas polls; it needs no public address |
| Where can this GitHub App be installed? | Only on this account |

**Repository permissions**, everything else "No access":

| Permission | Access | What tehas does with it |
| --- | --- | --- |
| Contents | Read and write | Push cycle branches, read the code and releases |
| Issues | Read and write | Labels, comments, issue edits, closing |
| Pull requests | Read and write | Open, edit and ready pull requests |
| Metadata | Read-only | Required by GitHub |
| Actions | Read-only | Read CI runs |
| Checks | Read-only | Read a pull request's checks |
| Commit statuses | Read-only | The same, for statuses |

Leave **Workflows** at "No access" on purpose: with it off, no cycle can push a
change to `.github/workflows/`. No organisation or account permissions, and
subscribe to no events.

Create the app. On its **General** page note the **App ID**, and under
**Private keys** generate one: a `.pem` file downloads. Keep it secret; it is
the app's only credential.

One app per factory is the safe choice: a suspended app, or a leaked key, then
stops one factory, not all of them.

## Install it

On the app's page, **Install App**, then **Install** next to the organisation,
and choose the repositories: the product repository, and the repository
improvement findings go to if it is another one. The installation's settings
page ends in its **installation ID**:
`https://github.com/organizations/<org>/settings/installations/<id>`.

The repository's own **Settings → GitHub Apps** lists the app only after this
step.

## Give it to the host

As the tehas user:

```bash
install -m 600 <downloaded>.private-key.pem ~/.config/tehas/github-app.pem
```

The key stays in `~/.config/tehas`, outside the instance directory, and is
never given to a cycle's container. Add the three settings to the instance's
`instance.env`, all three or none:

```bash
TEHAS_GITHUB_APP_ID=<App ID>
TEHAS_GITHUB_APP_INSTALLATION=<installation ID>
TEHAS_BOT_LOGIN=<slug>[bot]
```

Commits a cycle makes are authored as the tehas user's git identity. Set it to
the app, so they are attributed to it. The bot's user ID is
`gh api 'users/<slug>[bot]' --jq .id`:

```bash
git config --global user.name '<slug>[bot]'
git config --global user.email '<user id>+<slug>[bot]@users.noreply.github.com'
```

Then check it:

```bash
. ~/tehas/env.sh
tehasbot github-token --check
```

It prints the app, the installation, the bot's login, when the token runs out
and whether the installation reaches `TEHAS_REPO`, and never the token itself.
`command -v gh` now answers `~/tehas/shim/gh`.

`tehas setup` still asks for a token and does not take an app yet: on a new
host, run it with a token as [a new project](/new-project) says, then move to
the app with the steps above and remove the token from `~/.config/tehas/env`.

## What tehas does with it

- `tehasbot github-token` signs a short JSON Web Token with the key, exchanges
  it for an installation token, and keeps that in `~/tehas/state/github/token`
  (mode 600) until fewer than 10 minutes are left.
- `env.sh` puts `~/tehas/shim` first on `PATH`. Its `gh` asks for the token at
  every call, so a command that runs for hours never holds an expired one;
  git's github.com credential helper asks the same `gh`. A `GH_TOKEN` left in
  `~/.config/tehas/env` is dropped.
- A cycle's container is given `~/tehas/state/github` read-only, never the key.
  The image's own `gh` reads the token there at every call, and `tehas work`
  renews it every 5 minutes while the cycle runs.
- The dispatcher, the refresh and the Protolab take the bot's login from
  `TEHAS_BOT_LOGIN`, because an installation token cannot ask GitHub who it is.

[The commands page](/tools#tehasbot-github-token) has the details.

## Things seen on the first app

- A token's answer for a repository says `"push": false`. That field is a
  user's access and is not filled for an app; Contents write is what lets it
  push.
- GitHub names the app `<slug>[bot]` in REST answers and `<slug>` in GraphQL
  and `gh --json`. tehas drops the `[bot]` where it compares the two.
- Moving an existing factory from a machine user to an app: comments the old
  account wrote stay where they are, but if that account is suspended they are
  hidden, and the dispatcher cannot see a question it asked there
  (`skip #N: its question is not among the last 100 comments`). Apply
  `tehas:ready` or `tehas:refine` again to go on with such an issue.
